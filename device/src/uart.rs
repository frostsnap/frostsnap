extern crate alloc;
use alloc::format;
use alloc::vec::Vec;
use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::Uart;
use esp_hal_common::uart::Instance;
use esp_hal_common::{timer::Instance as TimerInstance, Timer};
use esp_println::println;

pub struct DeviceUart<'a, T, U> {
    pub uart: Uart<'a, T>,
    from_buffer_cache: Vec<u8>,
    timer: Timer<U>,
}

impl<'a, T, U> DeviceUart<'a, T, U> {
    pub fn new(uart: Uart<'a, T>, timer: Timer<U>) -> Self {
        Self {
            uart,
            timer,
            from_buffer_cache: Vec::new(),
        }
    }
}

impl<'a, T, U> Reader for DeviceUart<'a, T, U>
where
    T: Instance,
    U: TimerInstance,
{
    fn peek_read(&mut self, n_bytes: usize) -> Option<&[u8]> {
        for _ in 0..n_bytes {
            match self.uart.read() {
                Err(_) => break,
                Ok(c) => self.from_buffer_cache.push(c),
            }
        }

        println!("Peeking read {:?}", self.from_buffer_cache);
        return if self.from_buffer_cache.len() == 0 {
            None
        } else {
            Some(self.from_buffer_cache.as_slice())
        };
    }

    fn consume(&mut self, n_bytes: usize) {
        println!("Consoooming");
        if self.from_buffer_cache.len() <= n_bytes {
            self.from_buffer_cache = Vec::new();
        } else {
            self.from_buffer_cache = self.from_buffer_cache[n_bytes..].to_vec()
        }
    }

    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        let mut i = 0;
        let mut error_state = false;
        let mut start: u64 = 0;
        println!("Normal read, existing cache: {:?}", self.from_buffer_cache);

        // Take bytes from existing cached first
        for byte in &self.from_buffer_cache {
            bytes[i] = *byte;
            i += 1
        }
        self.consume(i);

        while i < bytes.len() {
            match self.uart.read() {
                Err(e) => {
                    if !error_state {
                        start = self.timer.now();
                        error_state = true;
                    }
                    let elapsed = (self.timer.now() - start) / 40000;
                    // timeout, return error after 100ms
                    if true {
                        //elapsed > 100 {
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
