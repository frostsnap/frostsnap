use bincode::{de::read::Reader, enc::write::Writer};
use serialport::SerialPort;
use std::io::{self, Write};

pub const MAGICBYTES: [u8; 4] = [0xb, 0xe, 0xe, 0xf];

pub struct SerialPortBincode {
    port: Box<dyn SerialPort>,
}

impl SerialPortBincode {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self { port }
    }

    pub fn read_for_magic_bytes(&mut self, search_lim: usize) -> bool {
        let mut search_buff: Vec<u8> = Vec::new();
        let search_bytes = MAGICBYTES.to_vec();
        for _ in 0..search_lim {
            let mut byte_buff: [u8; 1] = [0; 1];
            match self.port.read(&mut byte_buff) {
                Ok(_) => {
                    search_buff.push(byte_buff[0]);
                    if search_buff.len() >= search_bytes.len() {
                        let start_index = search_buff.len() - search_bytes.len();
                        if search_buff[start_index..] == search_bytes {
                            return true;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        false
    }
}

impl Writer for SerialPortBincode {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        // for byte in bytes {
        // print!("{:02X}", byte);
        // }
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
            };
        }
    }
}
