use esp_idf_hal::uart::UartDriver;
use std::fmt::Write;
use std::thread;
use std::time::Duration;

use esp_idf_hal::delay::BLOCK;

use crate::io::DeviceIO;
use frostcore::message::FrostMessage;

pub fn write_to_serial(uart: &mut UartDriver, message: &str) {
    writeln!(uart, "{}", message).unwrap();
}

pub fn write_bytes_to_serial(uart: &mut UartDriver, message: &Vec<u8>) {
    uart.write(message.as_slice()).unwrap();
}

// Read bytes one by one (Note: not using any delay block! See readme notes)
fn read_from_serial(uart: &mut UartDriver, n_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..n_bytes {
        let mut buf = [0_u8; 1];
        match uart.read(&mut buf, BLOCK) {
            Err(e) => panic!("Failed to read from serial: {:?}", e),
            Ok(len) => bytes.push(buf[..len][0]),
        };
    }
    bytes
}

pub fn read_all_from_serial(uart: &mut UartDriver) -> Vec<u8> {
    let mut bytes = Vec::new();
    let remaining = uart.remaining_read().unwrap();
    bytes.append(&mut read_from_serial(uart, remaining.try_into().unwrap()));
    bytes
}

pub fn read_str_from_serial(uart: &mut UartDriver) -> String {
    std::string::String::from_utf8(read_all_from_serial(uart)).expect("valid utf8")
}

impl DeviceIO for UartDriver<'_> {
    /// Read a [`FrostMessage`] from serial.
    ///
    /// Returns an option of a message. The read is flushed if an error occurs.
    fn read_messages(&mut self) -> Vec<FrostMessage> {
        let binding = read_all_from_serial(self);
        let received = binding.as_slice();
        if received.len() > 0 {
            match bincode::deserialize::<Vec<FrostMessage>>(received) {
                Ok(messages) => {
                    // println!("Read from serial:");
                    // dbg!(&message);
                    return messages;
                }
                Err(e) => {
                    eprintln!("Error reading message: {:?}", e);
                    self.flush_read().expect("flushed serial read");
                    return vec![];
                }
            }
        }
        return vec![];
    }

    fn write_messages(&mut self, messages: Vec<FrostMessage>) {
        let write_bytes = bincode::serialize(&messages).unwrap();
        write_bytes_to_serial(self, &write_bytes);
        thread::sleep(Duration::from_millis(1000));
    }
}
