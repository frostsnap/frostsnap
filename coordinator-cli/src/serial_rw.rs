use bincode::{de::read::Reader, enc::write::Writer};
use serialport::SerialPort;
use std::io::{self, Write};

pub struct SerialPortBincode {
    port: Box<dyn SerialPort>,
}

impl SerialPortBincode {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self { port }
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
        loop {
            match self.port.read(bytes) {
                Ok(t) => {
                    return if t != bytes.len() {
                        Err(bincode::error::DecodeError::UnexpectedEnd {
                            additional: t - bytes.len(),
                        })
                    } else {
                        Ok(())
                    }
                }
                Err(e) => {
                    return Err(bincode::error::DecodeError::OtherString(format!(
                        "Coordinator read error {:?}",
                        e
                    )))
                }
                // Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => (),
                // Err(e) => (eprintln!("{:?}", e)),
            };
        }
    }
}
