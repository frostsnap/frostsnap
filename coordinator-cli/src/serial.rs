use std::time::Duration;

use frostsnap_coordinator::{PortDesc, Serial};

#[derive(Clone, Default, Debug)]
pub struct DesktopSerial;

pub type SerialPort = Box<dyn serialport::SerialPort>;

impl Serial for DesktopSerial {
    type Port = SerialPort;
    type OpenError = serialport::Error;

    fn available_ports(&self) -> Vec<PortDesc> {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|port| match port.port_type {
                serialport::SerialPortType::UsbPort(usb_port) => Some(PortDesc {
                    id: port.port_name,
                    vid: usb_port.vid,
                    pid: usb_port.pid,
                }),
                _ => None,
            })
            .collect()
    }

    fn open_device_port(&self, id: &str, baud_rate: u32) -> Result<Self::Port, Self::OpenError> {
        serialport::new(id, baud_rate)
            // This timeout should never be hit in any normal circumstance but it's important to
            // have in case a device is bisbehaving. Note: 10ms is too low and leads to errors when
            // writing.
            .timeout(Duration::from_millis(1_000))
            .open()
    }

    fn anything_to_read(port: &Self::Port) -> bool {
        match port.bytes_to_read() {
            Ok(len) => len > 0,
            // just say there's something there to get the caller to read and get the error rather than returing it here
            Err(_) => true,
        }
    }
}
