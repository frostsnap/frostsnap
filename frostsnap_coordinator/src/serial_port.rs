use frostsnap_comms::{DeviceReceiveSerial, DeviceSendSerial, Downstream, MagicBytes};
use std::io::{BufRead, BufReader, Read, Write};

pub trait Serial {
    type Port: Read + Write;
    type OpenError: std::error::Error;

    fn available_ports(&self) -> Vec<PortDesc>;
    fn open_device_port(
        &self,
        unique_id: &str,
        baud_rate: u32,
    ) -> Result<Self::Port, Self::OpenError>;
    /// Allows querying whether there is anything to read without blocking
    fn anything_to_read(port: &Self::Port) -> bool;
}

#[derive(Debug, Clone)]
pub struct PortDesc {
    pub unique_id: String,
    pub vid: u16,
    pub pid: u16,
}

pub struct FramedSerialPort<S: Serial> {
    magic_bytes_progress: usize,
    inner: BufReader<S::Port>,
}

impl<S: Serial> FramedSerialPort<S> {
    pub fn new(port: S::Port) -> Self {
        Self {
            inner: BufReader::new(port),
            magic_bytes_progress: 0,
        }
    }

    pub fn read_for_magic_bytes(&mut self) -> Result<bool, std::io::Error> {
        if !S::anything_to_read(self.inner.get_ref()) {
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
        message: &DeviceReceiveSerial<Downstream>,
    ) -> Result<(), bincode::error::EncodeError> {
        let _bytes_written = bincode::encode_into_std_write(
            message,
            self.inner.get_mut(),
            bincode::config::standard(),
        )?;
        Ok(())
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        self.send_message(&DeviceReceiveSerial::<Downstream>::MagicBytes(
            MagicBytes::default(),
        ))
    }

    pub fn try_read_message(
        &mut self,
    ) -> Result<Option<DeviceSendSerial<Downstream>>, bincode::error::DecodeError> {
        if !S::anything_to_read(self.inner.get_ref()) && self.inner.buffer().is_empty() {
            return Ok(None);
        }
        Ok(Some(bincode::decode_from_reader(
            &mut self.inner,
            bincode::config::standard(),
        )?))
    }
}
