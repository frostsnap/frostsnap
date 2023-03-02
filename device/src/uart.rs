extern crate alloc;
use crate::uart::uart::Instance;
use alloc::{string::String, vec};
use bincode::de::read::Reader;
use bincode::error::DecodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::{
    peripherals::{self, Peripherals, UART0},
    uart, Cpu, Delay, Rtc, Uart,
};

// TODO use generic UART
pub struct DeviceUart<'a> {
    uart: Uart<'a, UART0>,
}

impl DeviceUart<'a> {
    pub fn new(uart: Uart<'a, UART0>) -> Self {
        Self { uart }
    }
}

impl<'a> Reader for DeviceUart<'a> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        let n = bytes.len();
        let mut buf: vec::Vec<u8> = vec::Vec::new();

        for i in 0..n {
            let c = match self.uart.read() {
                Err(_) => return Err(DecodeError::LimitExceeded),
                Ok(c) => c,
            };
            bytes[i] = c;
        }

        Ok(())
    }
}
