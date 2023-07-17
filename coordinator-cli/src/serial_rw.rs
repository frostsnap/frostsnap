use bincode::{de::read::Reader, enc::write::Writer};
use frostsnap_comms::{DeviceReceiveSerial, Downstream, MagicBytes};
use serialport::SerialPort;
use std::io::{self, Write};

pub struct SerialPortBincode {
    pub port: Box<dyn SerialPort>,
    pub serial_number: String,
    pub(crate) buffer: Vec<u8>,
    pub next_write_magic: u128,
}

impl SerialPortBincode {
    pub fn new(port: Box<dyn SerialPort>, serial_number: String) -> Self {
        Self {
            port,
            serial_number,
            buffer: Vec::new(),
            next_write_magic: 0,
        }
    }

    pub fn poll_read(&mut self, limit: Option<usize>) -> Result<bool, io::Error> {
        let n = match limit {
            Some(limit) => limit,
            None => self.port.bytes_to_read()? as usize,
        };
        if n > 0 {
            let mut buffer = vec![0u8; n];
            match self.port.read(&mut buffer) {
                Ok(_) => self.buffer.append(&mut buffer),
                Err(e) => return Err(e),
            };
        }
        Ok(!self.buffer.is_empty())
    }

    pub fn send_message(
        &mut self,
        message: DeviceReceiveSerial<Downstream>,
    ) -> Result<(), bincode::error::EncodeError> {
        let mut slice = [0u8; 200];
        bincode::encode_into_slice(&message, &mut slice, bincode::config::standard()).unwrap();

        for byte in slice {
            print!("{} ", byte);
        }
        println!();

        bincode::encode_into_writer(&message, self, bincode::config::standard())
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis();

        if now > self.next_write_magic {
            self.next_write_magic = now + 1_000; // 100ms
            self.send_message(DeviceReceiveSerial::<Downstream>::MagicBytes(
                MagicBytes::default(),
            ))
        } else {
            Ok(())
        }
    }

    pub fn read_for_magic_bytes(&mut self) -> Result<bool, std::io::Error> {
        self.poll_read(None)?;
        Ok(frostsnap_comms::find_and_remove_magic_bytes::<Downstream>(
            &mut self.buffer,
        ))
    }
}

impl Writer for SerialPortBincode {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        loop {
            match self.port.write(&bytes) {
                Ok(_t) => {
                    return Ok(());
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                Err(e) => return Err(bincode::error::EncodeError::Io { inner: e, index: 0 }),
            }
        }
    }
}

impl Reader for SerialPortBincode {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
        while self.buffer.len() < bytes.len() {
            if let Err(e) = self.poll_read(Some(bytes.len() - self.buffer.len())) {
                return Err(bincode::error::DecodeError::Io {
                    inner: e,
                    additional: bytes.len() - self.buffer.len(),
                });
            };
        }

        let extra_bytes = self.buffer.split_off(bytes.len());
        bytes.copy_from_slice(&self.buffer);
        self.buffer = extra_bytes;

        Ok(())
    }
}
