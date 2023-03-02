use bincode::enc::write::Writer;
use serde_json::json;
use serialport::SerialPort;
use std::io::{self, Write};
use std::str;
use std::thread::sleep;
use std::time::Duration;

pub struct PortWriter {
    port: Box<dyn SerialPort>,
}

impl PortWriter {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self { port }
    }
}

impl Writer for PortWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        loop {
            let mut writebuf: Vec<u8> = vec![0; 1024];
            write!(
                writebuf.as_mut_slice(),
                "{}",
                json!({ "success": true }).to_string()
            )
            .unwrap();
            writebuf.as_mut_slice().write(bytes).unwrap();
            match self.port.write(&writebuf.as_slice()) {
                Ok(_t) => {
                    println!("to client: {}", str::from_utf8(&writebuf[..]).unwrap());
                }
                Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
                Err(e) => eprintln!("{:?}", e),
            }

            sleep(Duration::from_millis(1000));
        }
    }
}
