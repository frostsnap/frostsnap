extern crate alloc;
use alloc::format;
use alloc::vec::Vec;

use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use esp32c3_hal::prelude::_embedded_hal_serial_Read;
use esp32c3_hal::{uart::Instance, Uart};

pub const MAGICBYTES: [u8; 4] = [0xb, 0xe, 0xe, 0xf];

pub struct DeviceUart<'a, T> {
    pub uart: Uart<'a, T>,
    pub read_buffer: Vec<u8>,
}

impl<'a, T> DeviceUart<'a, T> {
    pub fn new(uart: Uart<'a, T>) -> Self {
        Self {
            uart,
            read_buffer: Vec::new(),
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

    pub fn read_for_magic_bytes(&mut self, search_lim: usize) -> bool
    where
        T: Instance,
    {
        for _ in 0..search_lim {
            if self.poll_read() {
                let search_bytes = MAGICBYTES.to_vec();
                if self.read_buffer.len() >= search_bytes.len() {
                    let start_index = self.read_buffer.len() - search_bytes.len();
                    if self.read_buffer[start_index..] == search_bytes {
                        return true;
                    }
                }
            } else {
                return false;
            }
        }
        false
    }
}

impl<'a, T> Reader for DeviceUart<'a, T>
where
    T: Instance,
{
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        while self.read_buffer.len() < bytes.len() {
            self.poll_read();
        }
        let extra_bytes = self.read_buffer.split_off(bytes.len());

        bytes.copy_from_slice(&self.read_buffer);
        self.read_buffer = extra_bytes;
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
