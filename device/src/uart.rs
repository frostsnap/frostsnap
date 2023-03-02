extern crate alloc;
use alloc::format;
use bincode::de::read::Reader;
use bincode::error::DecodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::{peripherals::UART0, Uart};

// TODO use generic UART
pub struct DeviceUart<'a> {
    uart: Uart<'a, UART0>,
}

impl<'a> DeviceUart<'a> {
    pub fn new(uart: Uart<'a, UART0>) -> Self {
        Self { uart }
    }
}

impl<'a> Reader for DeviceUart<'a> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        for i in 0..bytes.len() {
            let c = match self.uart.read() {
                Err(e) => return Err(DecodeError::OtherString(format!("{:?}", e))),
                Ok(c) => c,
            };
            bytes[i] = c;
        }
        Ok(())
    }
}
