use frostsnap_comms::{Downstream, MagicBytes, ReceiveSerial, Upstream, BINCODE_CONFIG};
pub use serialport;
use std::io::{BufRead, BufReader, Read};

pub type SerialPort = Box<dyn serialport::SerialPort>;

// NOTE: This trait is not really necessary anymore because it seesm the serialport library works on
// enough platforms that we could just use it everywhere. This trait is sticking around because it's
// work to remove and maybe I'm wrong.
pub trait Serial: Send {
    fn available_ports(&self) -> Vec<PortDesc>;
    fn open_device_port(
        &self,
        unique_id: &str,
        baud_rate: u32,
    ) -> Result<SerialPort, PortOpenError>;
}

pub enum PortOpenError {
    DeviceBusy,
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Clone)]
pub struct PortDesc {
    pub id: String,
    pub vid: u16,
    pub pid: u16,
}

pub struct FramedSerialPort {
    magic_bytes_progress: usize,
    inner: BufReader<SerialPort>,
}

impl FramedSerialPort {
    pub fn new(port: SerialPort) -> Self {
        Self {
            inner: BufReader::new(port),
            magic_bytes_progress: 0,
        }
    }

    pub fn anything_to_read(&self) -> bool {
        match self.inner.get_ref().bytes_to_read() {
            Ok(len) => len > 0,
            // just say there's something there to get the caller to read and get the error rather than returing it here
            Err(_) => true,
        }
    }

    pub fn read_for_magic_bytes(&mut self) -> Result<bool, std::io::Error> {
        if !self.anything_to_read() {
            return Ok(false);
        }
        self.inner.fill_buf()?;
        let mut consumed = 0;
        let (progress, found) = frostsnap_comms::make_progress_on_magic_bytes::<Downstream>(
            self.inner
                .buffer()
                .iter()
                .cloned()
                .inspect(|_| consumed += 1),
            self.magic_bytes_progress,
        );
        self.inner.consume(consumed);
        self.magic_bytes_progress = progress;
        Ok(found)
    }

    pub fn send_message(
        &mut self,
        message: &ReceiveSerial<Upstream>,
    ) -> Result<(), bincode::error::EncodeError> {
        let _bytes_written =
            bincode::encode_into_std_write(message, self.inner.get_mut(), BINCODE_CONFIG)?;
        Ok(())
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        self.send_message(&ReceiveSerial::<Upstream>::MagicBytes(MagicBytes::default()))
    }

    pub fn try_read_message(
        &mut self,
    ) -> Result<Option<ReceiveSerial<Downstream>>, bincode::error::DecodeError> {
        if !self.anything_to_read() && self.inner.buffer().is_empty() {
            return Ok(None);
        }
        Ok(Some(bincode::decode_from_reader(
            &mut self.inner,
            BINCODE_CONFIG,
        )?))
    }

    pub fn raw_write(&mut self, bytes: &[u8]) -> Result<(), std::io::Error> {
        let io_device = self.inner.get_mut();
        io_device.write_all(bytes)?;
        Ok(())
    }

    pub fn raw_read(&mut self, bytes: &mut [u8]) -> Result<(), std::io::Error> {
        self.inner.read_exact(bytes)?;
        Ok(())
    }
}

use std::time::Duration;

/// impl using the serialport crate
#[derive(Clone, Default, Debug)]
pub struct DesktopSerial;

impl Serial for DesktopSerial {
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

    fn open_device_port(&self, id: &str, baud_rate: u32) -> Result<SerialPort, PortOpenError> {
        serialport::new(id, baud_rate)
            // This timeout should never be hit in any normal circumstance but it's important to
            // have in case a device is bisbehaving. Note: 10ms is too low and leads to errors when
            // writing.
            .timeout(Duration::from_millis(5_000))
            .open()
            .map_err(|e| {
                if e.to_string() == "Device or resource busy" {
                    PortOpenError::DeviceBusy
                } else {
                    PortOpenError::Other(Box::new(e))
                }
            })
    }
}

impl<T: Serial + Sync> Serial for std::sync::Arc<T> {
    fn available_ports(&self) -> Vec<PortDesc> {
        self.as_ref().available_ports()
    }

    fn open_device_port(
        &self,
        unique_id: &str,
        baud_rate: u32,
    ) -> Result<SerialPort, PortOpenError> {
        self.as_ref().open_device_port(unique_id, baud_rate)
    }
}
