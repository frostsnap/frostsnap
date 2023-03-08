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
use esp_hal_common::{timer::Instance as TimerInstance, Timer};
use esp_println::println;
pub struct DeviceUart<'a, T, U> {
    pub uart: Uart<'a, T>,
    timer: Timer<U>,
}

impl<'a, T, U> DeviceUart<'a, T, U> {
    pub fn new(uart: Uart<'a, T>, timer: Timer<U>) -> Self {
        Self { uart, timer }
    }
}

impl<'a, T, U> Reader for DeviceUart<'a, T, U>
where
    T: Instance,
    U: TimerInstance,
{
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        let mut i = 0;
        let mut error_state = false;
        let mut start: u64 = 0;
        while i < bytes.len() {
            match self.uart.read() {
                Err(e) => {
                    if !error_state {
                        start = self.timer.now();
                        error_state = true;
                    }
                    let elapsed = (self.timer.now() - start) / 40000;
                    // timeout, return error after 100ms
                    if elapsed > 100 {
                        return Err(DecodeError::OtherString(format!("{:?}", e)));
                    }
                    // retries uart read
                    continue;
                }
                Ok(c) => {
                    error_state = false;
                    start = 0;
                    bytes[i] = c;
                    i += 1;
                }
            };
        }
        Ok(())
    }
}

impl<'a, T, U> Writer for DeviceUart<'a, T, U>
where
    T: Instance,
    U: TimerInstance,
{
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        match self.uart.write_bytes(bytes) {
            Err(e) => return Err(EncodeError::OtherString(format!("{:?}", e))),
            Ok(()) => Ok(()),
        }
    }
}
