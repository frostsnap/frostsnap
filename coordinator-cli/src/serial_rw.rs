use bincode::{de::read::Reader, enc::write::Writer};
use serialport::SerialPort;
use std::{
    io::{self, Write},
    time::Duration,
};

use crate::wait_for_device_port;

pub struct SerialPortBincode {
    pub port: Box<dyn SerialPort>,
    pub(crate) buffer: Vec<u8>,
}

impl SerialPortBincode {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self {
            port,
            buffer: Vec::new(),
        }
    }

    pub fn get_buffer(&self) -> Vec<u8> {
        self.buffer.clone()
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
                Err(e) => {
                    eprintln!("{:?}", e);
                    return Err(bincode::error::EncodeError::OtherString(format!(
                        "Writing error {:?}",
                        e
                    )));
                }
            }
        }
    }
}

impl Reader for SerialPortBincode {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
        while self.buffer.len() < bytes.len() {
            let bytes_to_read = bytes.len() - self.buffer.len();
            match self.port.read(bytes) {
                Ok(t) => {
                    return if t < bytes_to_read {
                        Err(bincode::error::DecodeError::UnexpectedEnd {
                            additional: bytes_to_read - t,
                        })
                    } else {
                        for byte in bytes {
                            self.buffer.push(*byte);
                        }
                        Ok(())
                    }
                }
                Err(e) => {
                    return Err(bincode::error::DecodeError::OtherString(format!(
                        "Coordinator read error {:?}",
                        e
                    )))
                }
            };
        }

        let extra_bytes = self.buffer.split_off(bytes.len());
        bytes.copy_from_slice(&self.buffer);
        self.buffer = extra_bytes;

        // println!("{:?}", bytes);
        // println!("{:?}", self.buffer);
        Ok(())
    }
}
