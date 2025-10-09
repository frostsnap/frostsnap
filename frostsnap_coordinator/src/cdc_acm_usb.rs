use anyhow::{anyhow, Result};
use nusb::{
    descriptors::TransferType,
    io::{EndpointRead, EndpointWrite},
    transfer::{Bulk, ControlOut, ControlType, Direction, In, Out, Recipient},
    Device, Interface, MaybeFuture,
};
use std::cell::RefCell;
use std::io;
use std::os::fd::OwnedFd;
use std::time::Duration;
use tracing::{event, Level};

#[allow(unused)]
pub struct CdcAcmSerial {
    dev: Device, // keep last for correct drop order
    name: String,
    baud: u32,
    if_comm: Interface,
    if_data: Interface,
    writer: EndpointWrite<Bulk>,
    reader: RefCell<EndpointRead<Bulk>>,
    timeout: Duration,
}

impl CdcAcmSerial {
    /// Wrap an already-opened usbfs FD.
    ///
    /// Create a blocking CDC-ACM port from an already-open usbfs FD. It automatically difures out
    /// the interface numbers and bulk out/in endpoint addresses.
    pub fn new_auto(fd: OwnedFd, name: String, baud: u32) -> Result<Self> {
        let dev = Device::from_fd(fd).wait()?;
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
                        if ep.transfer_type() == TransferType::Bulk {
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
        let if_comm = dev.detach_and_claim_interface(comm_if).wait()?;
        event!(Level::DEBUG, if_num = data_if, "claiming data interface");
        let if_data = dev.detach_and_claim_interface(data_if).wait()?;

        let writer = if_data
            .endpoint::<Bulk, Out>(ep_out)?
            .writer(4096)
            .with_num_transfers(4);
        let reader = if_data
            .endpoint::<Bulk, In>(ep_in)?
            .reader(4096)
            .with_num_transfers(4)
            .with_read_timeout(Duration::from_millis(10_000));

        // ---------------- mandatory CDC setup packets ---------------------
        send_cdc_setup(&if_comm, baud)?;

        let self_ = Self {
            dev,
            baud,
            name,
            if_comm,
            if_data,
            writer,
            reader: RefCell::new(reader),
            timeout: Duration::from_secs(1),
        };

        event!(Level::INFO, name = self_.name, "opened USB port");

        Ok(self_)
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

    if_comm
        .control_out(
            ControlOut {
                control_type: ControlType::Class,
                recipient: Recipient::Interface,
                request: 0x20,
                value: 0,
                index: if_comm.interface_number() as u16,
                data: &line_coding,
            },
            Duration::from_millis(100),
        )
        .wait()?;

    // USB-CDC §6.2.3.7 – SET_CONTROL_LINE_STATE (assert DTR | RTS)
    if_comm
        .control_out(
            ControlOut {
                control_type: ControlType::Class,
                recipient: Recipient::Interface,
                request: 0x22,
                value: 0x0003,
                index: if_comm.interface_number() as u16,
                data: &[],
            },
            Duration::from_millis(100),
        )
        .wait()?;

    Ok(())
}

//--------------------------------------------------------------------------
// std::io::Write impl  –– bulk-OUT
//--------------------------------------------------------------------------
impl io::Write for CdcAcmSerial {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.submit();
        Ok(())
    }
}

//--------------------------------------------------------------------------
// std::io::Read impl  –– bulk-IN
//--------------------------------------------------------------------------
impl io::Read for CdcAcmSerial {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.borrow_mut().read(buf)
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
            use futures::task::noop_waker;
            use std::pin::Pin;
            use std::task::{Context, Poll};
            use tokio::io::AsyncBufRead;

            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);

            // Poll without blocking to check if data is available
            let mut reader = self.reader.borrow_mut();
            let pin_reader = Pin::new(&mut *reader);

            match pin_reader.poll_fill_buf(&mut cx) {
                Poll::Ready(Ok(buf)) => Ok(buf.len() as u32),
                Poll::Ready(Err(_)) => Ok(0),
                Poll::Pending => Ok(0), // No data available yet, don't block
            }
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
