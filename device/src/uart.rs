extern crate alloc;
use alloc::format;
use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::prelude::_embedded_hal_serial_Write;
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

impl<'a, T> Writer for DeviceUart<'a, T>
where
    T: Instance,
{
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        match self.uart.write_bytes(bytes) {
            Err(e) => return Err(EncodeError::OtherString(format!("{:?}", e))),
            Ok(()) => Ok(()),
        }
    }
}
