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
                    return Err(bincode::error::EncodeError::OtherString(format!("{:?}", e)));
                }
            }
        }
    }
}

impl Reader for SerialPortBincode {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), bincode::error::DecodeError> {
        return match self.port.read(bytes) {
            Ok(t) => {
                if t != bytes.len() {
                    Err(bincode::error::DecodeError::ArrayLengthMismatch {
                        required: t,
                        found: bytes.len(),
                    })
                } else {
                    Ok(())
                }
            }
            // Err(ref e) if e.kind() == io::ErrorKind::BrokenPipe => {
            //     eprintln!("{:?} disconnected", &self.port.name());
            //     std::process::exit(1);
            // }
            Err(e) => Err(bincode::error::DecodeError::OtherString(format!("{:?}", e))),
        };
    }
}
