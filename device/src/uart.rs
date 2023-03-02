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
