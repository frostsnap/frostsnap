use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::i2c::{I2cDriver, I2cSlaveDriver};
use std::thread;
use std::time::Duration;

use crate::io::DeviceIO;
use frostcore::message::FrostMessage;

// Master I2C read/write all bytes
pub fn read_from_i2c(i2c: &mut I2cDriver) -> Vec<u8> {
    let mut buf = vec![0; 1024];
    // let empty = [0xff; 1023];

    // loop {
    //     i2c.read(0x21, &mut buf.as_mut_slice(), 1000)
    //         .unwrap_or_else(|e| eprintln!("Failed to flush i2c: {:?}", e));
    //     // println!("{:?}", buf);
    //     thread::sleep(Duration::from_millis(1000));
    //     if buf[0] == 1 {
    //         break;
    //     }
    // }

    i2c.read(0x21, &mut buf.as_mut_slice(), BLOCK)
        .unwrap_or_else(|e| eprintln!("Failed to read from i2c: {:?}", e));
    buf
}

pub fn write_to_i2c(i2c: &mut I2cDriver, message: &Vec<u8>) {
    i2c.write(0x21, &message.as_slice(), 1000)
        .unwrap_or_else(|e| eprintln!("Failed to write to i2c: {:?}", e));
}

pub fn flush_i2c(i2c: &mut I2cDriver) {
    let mut buf = [0_u8; 1024];
    let empty = [0xff; 1023];
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
        println!("reading");
        let binding = read_from_i2c(self);
        let received = binding.as_slice();
        if received.len() > 0 {
            match bincode::deserialize::<Vec<FrostMessage>>(received) {
                Ok(messages) => {
                    // println!("Read from master i2c:");
                    // dbg!(&messages);
                    return messages;
                }
                Err(e) => {
                    eprintln!("Error reading message: {:?}", e);
                    // flush_i2c(self);
                    return vec![];
                }
            }
        }
        return vec![];
    }

    fn write_messages(&mut self, messages: Vec<FrostMessage>) {
        println!("writing");
        let write_bytes = bincode::serialize(&messages).unwrap();
        write_to_i2c(self, &write_bytes);
        thread::sleep(Duration::from_millis(1000));
    }
}

// Slave I2C read/write all bytes
pub fn read_from_slave_i2c(i2c: &mut I2cSlaveDriver) -> Vec<u8> {
    let mut buf = vec![0; 1024];
    let len = i2c.read(&mut buf, 1000).unwrap_or_else(|e| {
        eprintln!("Failed to read from i2c: {:?}", e);
        0
    });
    buf[..len].to_vec()
}

pub fn write_to_slave_i2c(i2c: &mut I2cSlaveDriver, message: &Vec<u8>) {
    i2c.write(&message.as_slice(), 1000).unwrap_or_else(|e| {
        eprintln!("Failed to write from i2c: {:?}", e);
        0
    });
}

pub fn flush_slave_i2c(i2c: &mut I2cSlaveDriver) {
    let mut buf = [0_u8; 1024];
    let empty = [0xff; 1023];
    while buf[1..] != empty {
        i2c.read(&mut buf, 1000).unwrap_or_else(|e| {
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
                    // println!("Read from slave i2c:");
                    // dbg!(&messages);
                    return messages;
                }
                Err(e) => {
                    eprintln!("Error reading message: {:?}", e);
                    // flush_slave_i2c(self);
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
