use anyhow::{anyhow, Result};
use futures::executor::block_on;
use futures::future::{select, Either};
use futures::task::noop_waker;
use futures_timer::Delay;
use nusb::{
    transfer::{
        Control, ControlType, Direction, EndpointType, Recipient, RequestBuffer, TransferFuture,
    },
    Device, Interface,
};
use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::os::fd::OwnedFd;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tracing::{event, Level};

#[allow(unused)]
pub struct CdcAcmSerial {
    dev: Device, // keep last for correct drop order
    name: String,
    baud: u32,
    if_comm: Interface,
    if_data: Interface,
    ep_out: u8,
    ep_in: u8,
    timeout: Duration,
    // we need interior mutability because bytes_to_read doesn't take &mut self
    rx_fut: std::cell::RefCell<Option<TransferFuture<RequestBuffer>>>,
    rx_buf: std::cell::RefCell<VecDeque<u8>>,
}

impl CdcAcmSerial {
    /// Wrap an already-opened usbfs FD.
    ///
    /// Create a blocking CDC-ACM port from an already-open usbfs FD. It automatically difures out
    /// the interface numbers and bulk out/in endpoint addresses.
    pub fn new_auto(fd: OwnedFd, name: String, baud: u32) -> Result<Self> {
        let dev = Device::from_fd(fd)?;
        let cfg = dev.active_configuration()?;

        // ---------------- scan for CDC interfaces & bulk endpoints -------
        let mut comm_if_num = None;
        let mut data_if_num = None;
        let mut ep_in_addr = None;
        let mut ep_out_addr = None;

        for grp in cfg.interfaces() {
            // alt-setting 0 (over 99 % of CDC devices have only alt 0)
            let alt0 = grp.alt_settings().next().unwrap();

            match (alt0.class(), alt0.subclass()) {
                // -- 0x02/0x02  = CDC Communications, Abstract Control Model
                (0x02, 0x02) if comm_if_num.is_none() => {
                    comm_if_num = Some(alt0.interface_number());
                }
                // -- 0x0A/**    = CDC Data
                (0x0A, _) if data_if_num.is_none() => {
                    data_if_num = Some(alt0.interface_number());

                    // Pick the first bulk-IN and bulk-OUT on this interface.
                    for ep in alt0.endpoints() {
                        if ep.transfer_type() == EndpointType::Bulk {
                            match ep.direction() {
                                Direction::In if ep_in_addr.is_none() => {
                                    ep_in_addr = Some(ep.address());
                                }
                                Direction::Out if ep_out_addr.is_none() => {
                                    ep_out_addr = Some(ep.address());
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let comm_if = comm_if_num.ok_or_else(|| anyhow!("No CDC comm IF"))?;
        let data_if = data_if_num.ok_or_else(|| anyhow!("No CDC data IF"))?;
        let ep_in = ep_in_addr.ok_or_else(|| anyhow!("No bulk-IN ep"))?;
        let ep_out = ep_out_addr.ok_or_else(|| anyhow!("No bulk-OUT ep"))?;

        // ---------------- claim interfaces (detaching any kernel driver) ---
        event!(Level::DEBUG, if_num = comm_if, "claiming comm interface");
        let if_comm = dev.detach_and_claim_interface(comm_if)?;
        event!(Level::DEBUG, if_num = data_if, "claiming data interface");
        let if_data = dev.detach_and_claim_interface(data_if)?;

        // ---------------- mandatory CDC setup packets ---------------------
        send_cdc_setup(&if_comm, baud)?;

        let self_ = Self {
            dev,
            baud,
            name,
            if_comm,
            if_data,
            ep_out,
            ep_in,
            rx_buf: Default::default(),
            rx_fut: Default::default(),
            timeout: Duration::from_secs(1),
        };

        // start the reading from the usb device to kick things off.
        self_.fill_buf_non_blocking();

        event!(
            Level::INFO,
            name = self_.name,
            bulk_in = self_.ep_in,
            bulk_out = self_.ep_out,
            "opened USB port"
        );

        Ok(self_)
    }

    fn fill_buf_non_blocking(&self) {
        let mut rx_buf = self.rx_buf.borrow_mut();
        if !rx_buf.is_empty() {
            return;
        }

        // buffer is not empty so we're going to poll the inflight URB to see if there's any data
        // available right now.
        let mut rx_fut_opt = self.rx_fut.borrow_mut();

        if let Some(rx_fut) = rx_fut_opt.as_mut() {
            let pin_fut = Pin::new(rx_fut);
            let waker = noop_waker(); // no async runtime needed
            let mut cx = Context::from_waker(&waker);

            match pin_fut.poll(&mut cx) {
                // ───── URB completed successfully ─────
                Poll::Ready(transfer_completion) => {
                    match transfer_completion.into_result() {
                        Ok(data) => rx_buf.extend(data),
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                error = e.to_string(),
                                device = self.name,
                                "error while trying to fill read buffer for cdc-acm device"
                            );
                        }
                    }; // queue packet
                    *rx_fut_opt = None; // drop finished future
                }
                // ───── still pending ───────────────────
                Poll::Pending => {
                    // leave `fut` untouched
                }
            }
        }

        if rx_fut_opt.is_none() {
            let fut = self.if_data.bulk_in(self.ep_in, RequestBuffer::new(64));
            *rx_fut_opt = Some(fut);
        }
    }
}

fn send_cdc_setup(if_comm: &Interface, baud: u32) -> Result<()> {
    event!(
        Level::DEBUG,
        if_num = if_comm.interface_number(),
        "doing usb-cdc SET_LINE_CODING and SET_CONTROL_LINE_STATE"
    );
    // USB-CDC §6.2.3.8 – SET_LINE_CODING
    let line_coding = [
        (baud & 0xFF) as u8,
        ((baud >> 8) & 0xFF) as u8,
        ((baud >> 16) & 0xFF) as u8,
        ((baud >> 24) & 0xFF) as u8,
        0x00, // 1 stop bit
        0x00, // no parity
        0x08, // 8 data bits
    ];

    let ctl = Control {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0x20,
        value: 0,
        index: if_comm.interface_number() as u16,
    };

    if_comm.control_out_blocking(ctl, &line_coding, Duration::from_millis(100))?;

    // USB-CDC §6.2.3.7 – SET_CONTROL_LINE_STATE (assert DTR | RTS)
    let ctl = Control {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0x22,
        value: 0x0003,
        index: if_comm.interface_number() as u16,
    };
    if_comm.control_out_blocking(ctl, &[], Duration::from_millis(100))?;

    Ok(())
}

//--------------------------------------------------------------------------
// std::io::Write impl  –– bulk-OUT
//--------------------------------------------------------------------------
impl io::Write for CdcAcmSerial {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        block_on(self.if_data.bulk_out(self.ep_out, buf.to_vec()))
            .into_result()
            .map_err(io::Error::other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

//--------------------------------------------------------------------------
// std::io::Read impl  –– bulk-IN
//--------------------------------------------------------------------------
impl io::Read for CdcAcmSerial {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.fill_buf_non_blocking();
        let mut rx_buf = self.rx_buf.borrow_mut();

        let mut copy_over = |rx_buf: &mut VecDeque<u8>| -> usize {
            let n = rx_buf.len().min(buf.len());
            for byte in buf.iter_mut().take(n) {
                *byte = rx_buf.pop_front().unwrap();
            }
            n
        };
        // ───────── 1 – fast path: drain already-queued bytes ─────────
        if !rx_buf.is_empty() {
            return Ok(copy_over(&mut rx_buf));
        }

        let mut rx_fut_opt = self.rx_fut.borrow_mut();

        // ───────── 2 – slow path: wait for URB *or* timeout ──────────
        let rx_fut = rx_fut_opt
            .take()
            .expect("rx_fut will be Some here because we fill_buf");
        let delay_future = Delay::new(self.timeout);

        match block_on(select(rx_fut, delay_future)) {
            Either::Left((req, _unused_delay)) => {
                let data = req.into_result().map_err(io::Error::other)?;
                rx_buf.extend(data);
                Ok(copy_over(&mut rx_buf))
            }

            // ───────── 2b. Timeout fired first ─────────
            Either::Right((_, pending_fut)) => {
                // `pending_fut` is still the same in-flight TransferFuture
                *rx_fut_opt = Some(pending_fut);
                Ok(0)
            }
        }
    }
}

mod _impl {

    use crate::serialport::*;

    #[allow(unused)]
    impl SerialPort for super::CdcAcmSerial {
        fn name(&self) -> Option<String> {
            Some(self.name.clone())
        }
        fn baud_rate(&self) -> Result<u32> {
            Ok(self.baud)
        }
        fn bytes_to_read(&self) -> Result<u32> {
            self.fill_buf_non_blocking();
            Ok(self.rx_buf.borrow().len() as u32)
        }

        fn data_bits(&self) -> Result<DataBits> {
            unimplemented!()
        }

        fn flow_control(&self) -> Result<FlowControl> {
            unimplemented!()
        }

        fn parity(&self) -> Result<Parity> {
            unimplemented!()
        }

        fn stop_bits(&self) -> Result<StopBits> {
            unimplemented!()
        }

        fn timeout(&self) -> core::time::Duration {
            unimplemented!()
        }

        fn set_baud_rate(&mut self, baud_rate: u32) -> Result<()> {
            unimplemented!()
        }

        fn set_data_bits(&mut self, data_bits: DataBits) -> Result<()> {
            unimplemented!()
        }

        fn set_flow_control(&mut self, flow_control: FlowControl) -> Result<()> {
            unimplemented!()
        }

        fn set_parity(&mut self, parity: Parity) -> Result<()> {
            unimplemented!()
        }

        fn set_stop_bits(&mut self, stop_bits: StopBits) -> Result<()> {
            unimplemented!()
        }

        fn set_timeout(&mut self, timeout: core::time::Duration) -> Result<()> {
            unimplemented!()
        }

        fn write_request_to_send(&mut self, level: bool) -> Result<()> {
            unimplemented!()
        }

        fn write_data_terminal_ready(&mut self, level: bool) -> Result<()> {
            unimplemented!()
        }

        fn read_clear_to_send(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_data_set_ready(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_ring_indicator(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_carrier_detect(&mut self) -> Result<bool> {
            unimplemented!()
        }
        fn bytes_to_write(&self) -> Result<u32> {
            unimplemented!()
        }

        fn clear(&self, buffer_to_clear: ClearBuffer) -> Result<()> {
            unimplemented!()
        }

        fn try_clone(&self) -> Result<Box<dyn SerialPort>> {
            unimplemented!()
        }

        fn set_break(&self) -> Result<()> {
            unimplemented!()
        }

        fn clear_break(&self) -> Result<()> {
            unimplemented!()
        }
    }
}
