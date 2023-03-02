extern crate alloc;
use alloc::format;
use bincode::de::read::Reader;
use bincode::error::DecodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::{peripherals::UART0, Uart};
use esp_hal_common::peripheral::Peripheral;
use esp_hal_common::uart::Instance;

pub struct DeviceUart<'a, T> {
    uart: Uart<'a, T>,
}

impl<'a, T> DeviceUart<'a, T> {
    pub fn new(uart: Uart<'a, T>) -> Self {
        Self { uart }
    }
}

impl<'a, T> Reader for DeviceUart<'a, T>
where
    T: Instance,
{
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
