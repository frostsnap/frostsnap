extern crate alloc;
use alloc::format;
use alloc::vec::Vec;

use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::timer::Timer;
use esp32c3_hal::{uart::Instance, Uart};
pub const MAGICBYTES: [u8; 4] = [0xb, 0xe, 0xe, 0xf];

pub struct DeviceUart<'a, T, U> {
    pub uart: Uart<'a, T>,
    pub read_buffer: Vec<u8>,
    timer: Timer<U>,
}

impl<'a, T, U> DeviceUart<'a, T, U>
where
    U: esp32c3_hal::timer::Instance,
{
    pub fn new(uart: Uart<'a, T>, timer: Timer<U>) -> Self {
        Self {
            uart,
            read_buffer: Vec::new(),
            timer,
        }
    }

    pub fn poll_read(&mut self) -> bool
    where
        T: Instance,
    {
        while let Ok(c) = self.uart.read() {
            self.read_buffer.push(c);
        }
        !self.read_buffer.is_empty()
    }

    pub fn read_for_magic_bytes(&mut self) -> bool
    where
        T: Instance,
    {
        if !self.poll_read() {
            return false;
        };

        let position = self
            .read_buffer
            .windows(MAGICBYTES.len())
            .position(|window| window == &MAGICBYTES[..]);
        match position {
            Some(position) => {
                self.read_buffer = self.read_buffer.split_off(position + MAGICBYTES.len());
                return true;
            }
            None => {
                self.read_buffer = self
                    .read_buffer
                    .split_off(self.read_buffer.len().saturating_sub(MAGICBYTES.len() + 1));
                return false;
            }
        }
    }
}

impl<'a, T, U> Reader for DeviceUart<'a, T, U>
where
    T: Instance,
    U: esp32c3_hal::timer::Instance,
{
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        let start_time = self.timer.now();

        while self.read_buffer.len() < bytes.len() {
            self.poll_read();
            if (self.timer.now() - start_time) / 40_000 > 1_000 {
                return Err(DecodeError::UnexpectedEnd {
                    additional: bytes.len() - self.read_buffer.len(),
                });
            }
        }
        let extra_bytes = self.read_buffer.split_off(bytes.len());

        bytes.copy_from_slice(&self.read_buffer);
        self.read_buffer = extra_bytes;
        Ok(())
    }
}

impl<'a, T, U> Writer for DeviceUart<'a, T, U>
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
