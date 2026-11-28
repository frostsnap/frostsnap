use frostsnap_coordinator::frostsnap_comms::{
    DeviceSendBody, DeviceSendMessage, Downstream, MagicBytes, ReceiveSerial, Sha256Digest,
    BINCODE_CONFIG,
};
use frostsnap_coordinator::frostsnap_core::schnorr_fun::fun::Point;
use frostsnap_coordinator::frostsnap_core::DeviceId;
use frostsnap_coordinator::serialport::{
    self, ClearBuffer, DataBits, FlowControl, Parity, StopBits,
};
use frostsnap_coordinator::{
    DeviceChange, PortDesc, PortOpenError, Serial, SerialPort, UsbSerialManager,
    BUSY_PORT_RETRY_PERIOD,
};
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;
const PORT: &str = "fake-port";

/// One device on one port that another process holds while `busy` is set.
struct FakeSerial {
    device_id: DeviceId,
    opens: Arc<AtomicUsize>,
    busy: Arc<AtomicBool>,
}

impl Serial for FakeSerial {
    fn available_ports(&self) -> Vec<PortDesc> {
        vec![PortDesc {
            id: PORT.into(),
            vid: USB_VID,
            pid: USB_PID,
        }]
    }

    fn open_device_port(&self, _: &str, _: u32) -> Result<SerialPort, PortOpenError> {
        self.opens.fetch_add(1, Ordering::SeqCst);
        if self.busy.load(Ordering::SeqCst) {
            return Err(PortOpenError::DeviceBusy);
        }
        let mut to_read = vec![];
        let frames: [ReceiveSerial<Downstream>; 2] = [
            ReceiveSerial::MagicBytes(MagicBytes::default()),
            ReceiveSerial::Message(
                DeviceSendMessage {
                    from: self.device_id,
                    body: DeviceSendBody::Announce {
                        firmware_digest: Sha256Digest([0u8; 32]),
                    },
                }
                .into(),
            ),
        ];
        for frame in frames {
            bincode::encode_into_std_write(frame, &mut to_read, BINCODE_CONFIG).unwrap();
        }
        Ok(Box::new(FakePort {
            to_read: to_read.into(),
        }))
    }
}

struct FakePort {
    to_read: VecDeque<u8>,
}

impl io::Read for FakePort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.to_read.read(buf)
    }
}

impl io::Write for FakePort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl serialport::SerialPort for FakePort {
    fn name(&self) -> Option<String> {
        Some(PORT.into())
    }
    fn baud_rate(&self) -> serialport::Result<u32> {
        Ok(frostsnap_coordinator::frostsnap_comms::BAUDRATE)
    }
    fn bytes_to_read(&self) -> serialport::Result<u32> {
        Ok(self.to_read.len() as u32)
    }
    fn data_bits(&self) -> serialport::Result<DataBits> {
        unimplemented!()
    }
    fn flow_control(&self) -> serialport::Result<FlowControl> {
        unimplemented!()
    }
    fn parity(&self) -> serialport::Result<Parity> {
        unimplemented!()
    }
    fn stop_bits(&self) -> serialport::Result<StopBits> {
        unimplemented!()
    }
    fn timeout(&self) -> std::time::Duration {
        unimplemented!()
    }
    fn set_baud_rate(&mut self, _: u32) -> serialport::Result<()> {
        unimplemented!()
    }
    fn set_data_bits(&mut self, _: DataBits) -> serialport::Result<()> {
        unimplemented!()
    }
    fn set_flow_control(&mut self, _: FlowControl) -> serialport::Result<()> {
        unimplemented!()
    }
    fn set_parity(&mut self, _: Parity) -> serialport::Result<()> {
        unimplemented!()
    }
    fn set_stop_bits(&mut self, _: StopBits) -> serialport::Result<()> {
        unimplemented!()
    }
    fn set_timeout(&mut self, _: std::time::Duration) -> serialport::Result<()> {
        unimplemented!()
    }
    fn write_request_to_send(&mut self, _: bool) -> serialport::Result<()> {
        unimplemented!()
    }
    fn write_data_terminal_ready(&mut self, _: bool) -> serialport::Result<()> {
        unimplemented!()
    }
    fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
        unimplemented!()
    }
    fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
        unimplemented!()
    }
    fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
        unimplemented!()
    }
    fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
        unimplemented!()
    }
    fn bytes_to_write(&self) -> serialport::Result<u32> {
        unimplemented!()
    }
    fn clear(&self, _: ClearBuffer) -> serialport::Result<()> {
        unimplemented!()
    }
    fn try_clone(&self) -> serialport::Result<Box<dyn serialport::SerialPort>> {
        unimplemented!()
    }
    fn set_break(&self) -> serialport::Result<()> {
        unimplemented!()
    }
    fn clear_break(&self) -> serialport::Result<()> {
        unimplemented!()
    }
}

fn poll(manager: &mut UsbSerialManager) -> Vec<DeviceChange> {
    (0..5).flat_map(|_| manager.poll_ports()).collect()
}

fn connected(changes: &[DeviceChange], device_id: DeviceId) -> bool {
    changes
        .iter()
        .any(|c| matches!(c, DeviceChange::Connected { id, .. } if *id == device_id))
}

#[test]
fn busy_port_is_retried_after_release() {
    let device_id = DeviceId::new(Point::random(&mut rand::thread_rng()));
    let opens = Arc::new(AtomicUsize::new(0));
    let busy = Arc::new(AtomicBool::new(true));
    let mut manager = UsbSerialManager::new(Box::new(FakeSerial {
        device_id,
        opens: opens.clone(),
        busy: busy.clone(),
    }));

    let changes = poll(&mut manager);
    assert!(!connected(&changes, device_id));
    assert_eq!(opens.load(Ordering::SeqCst), 1);

    busy.store(false, Ordering::SeqCst);
    let changes = poll(&mut manager);
    assert!(!connected(&changes, device_id));
    assert_eq!(
        opens.load(Ordering::SeqCst),
        1,
        "a busy port must not be retried on every poll"
    );

    std::thread::sleep(BUSY_PORT_RETRY_PERIOD);
    let changes = poll(&mut manager);
    assert!(connected(&changes, device_id));
    assert_eq!(opens.load(Ordering::SeqCst), 2);

    let changes = poll(&mut manager);
    assert!(changes.is_empty());
    assert_eq!(
        opens.load(Ordering::SeqCst),
        2,
        "an opened port must leave the retry schedule"
    );
}
