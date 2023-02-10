//! Interfaces though which the device can communicate -- Currently just contains serial, but
//! would be nice to have serial, i2c, http, all in the one `io/` directory.

use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::i2c::{I2cDriver, I2cSlaveDriver};
use esp_idf_hal::uart::{self, UartDriver};
use std::fmt::Write;
use std::thread;
use std::time::Duration;

use crate::message::FrostMessage;

// Currently devices communication through rounds of FrostMessages
pub trait DeviceIO {
    fn read_messages(&mut self) -> Vec<FrostMessage>;
    fn write_messages(&mut self, messages: Vec<FrostMessage>);
}

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

// Master I2C read/write all bytes
pub fn read_from_i2c(i2c: &mut I2cDriver) -> Vec<u8> {
    let mut buf = vec![0; 1024];
    i2c.read(0x21, &mut buf.as_mut_slice(), 1000)
        .unwrap_or_else(|e| eprintln!("Failed to read from i2c: {:?}", e));
    buf
}

pub fn write_to_i2c(i2c: &mut I2cDriver, message: &Vec<u8>) {
    i2c.write(0x21, &message.as_slice(), 1000)
        .unwrap_or_else(|e| eprintln!("Failed to write to i2c: {:?}", e));
}

pub fn flush_i2c(i2c: &mut I2cDriver) {
    let mut buf = [0_u8; 8];
    let empty = [0xff; 7];
    while buf[1..] != empty {
        i2c.read(0x21, &mut buf, 2)
            .unwrap_or_else(|e| eprintln!("Failed to flush i2c: {:?}", e));
    }
}

impl DeviceIO for I2cDriver<'_> {
    /// Read a [`FrostMessage`] from serial.
    ///
    /// Returns an option of a message. The read is flushed if an error occurs.
    fn read_messages(&mut self) -> Vec<FrostMessage> {
        let binding = read_from_i2c(self);
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
                    flush_i2c(self);
                    return vec![];
                }
            }
        }
        return vec![];
    }

    fn write_messages(&mut self, messages: Vec<FrostMessage>) {
        let write_bytes = bincode::serialize(&messages).unwrap();
        write_to_i2c(self, &write_bytes);
        thread::sleep(Duration::from_millis(1000));
    }
}

// Slave I2C read/write all bytes
pub fn read_from_slave_i2c(i2c: &mut I2cSlaveDriver) -> Vec<u8> {
    let mut buf = vec![0; 1024];
    let len = i2c.read(&mut buf, 1000)
        .unwrap_or_else(|e| {
            eprintln!("Failed to read from i2c: {:?}", e);
            0
        });
    buf[..len].to_vec()
}

pub fn write_to_slave_i2c(i2c: &mut I2cSlaveDriver, message: &Vec<u8>) {
    i2c.write(&message.as_slice(), 1000)
        .unwrap_or_else(|e| {
            eprintln!("Failed to write from i2c: {:?}", e);
            0
        });
}

pub fn flush_slave_i2c(i2c: &mut I2cSlaveDriver) {
    let mut buf = [0_u8; 8];
    let empty = [0xff; 7];
    while buf[1..] != empty {
        i2c.read(&mut buf, 2)
        .unwrap_or_else(|e| {
            eprintln!("Failed to flush i2c: {:?}", e);
            0
        });
    }
}

impl DeviceIO for I2cSlaveDriver<'_> {
    /// Read a [`FrostMessage`] from serial.
    ///
    /// Returns an option of a message. The read is flushed if an error occurs.
    fn read_messages(&mut self) -> Vec<FrostMessage> {
        let binding = read_from_slave_i2c(self);
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
                    flush_slave_i2c(self);
                    return vec![];
                }
            }
        }
        return vec![];
    }

    fn write_messages(&mut self, messages: Vec<FrostMessage>) {
        let write_bytes = bincode::serialize(&messages).unwrap();
        write_to_slave_i2c(self, &write_bytes);
        thread::sleep(Duration::from_millis(1000));
    }
}
