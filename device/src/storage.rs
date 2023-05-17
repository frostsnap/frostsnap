extern crate alloc;
use crate::state::FrostState;
use alloc::format;
use bincode::{
    de::read::Reader,
    enc::write::Writer,
    error::{DecodeError, EncodeError},
};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::{FlashStorage, FlashStorageError};

pub const NVS_PARTITION_START: u32 = 0x9000;
pub const NVS_PARTITION_SIZE: usize = 0x6000;

pub struct DeviceStorageRw<'a> {
    nvs: &'a mut DeviceStorage,
    pos: u32,
}

pub struct DeviceStorage {
    flash: FlashStorage,
    start_pos: u32,
}

impl DeviceStorage {
    pub fn new(flash: FlashStorage, start_pos: u32) -> Self {
        Self { flash, start_pos }
    }

    pub fn rw(&mut self) -> DeviceStorageRw<'_> {
        let start = self.start_pos;
        DeviceStorageRw {
            nvs: self,
            pos: start,
        }
    }

    pub fn erase(&mut self) -> Result<(), FlashStorageError> {
        let buf = [0u8; NVS_PARTITION_SIZE];
        self.flash.write(self.start_pos, &buf)
    }

    pub fn load(&mut self) -> Result<FrostState, DecodeError> {
        bincode::decode_from_reader(self.rw(), bincode::config::standard())
    }

    pub fn save(&mut self, state: &FrostState) -> Result<(), EncodeError> {
        // We write it in one go to try and get atomicity
        let bytes = bincode::encode_to_vec(state, bincode::config::standard()).unwrap();
        self.rw().write(&bytes)
    }
}

impl<'a> Reader for DeviceStorageRw<'a> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        self.nvs
            .flash
            .read(self.pos, bytes)
            .map_err(|e| DecodeError::OtherString(format!("Flash read error {:?}", e)))?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}

impl<'a> Writer for DeviceStorageRw<'a> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.nvs
            .flash
            .write(self.pos, bytes)
            .map_err(|e| EncodeError::OtherString(format!("Flash write error {:?}", e)))?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}
