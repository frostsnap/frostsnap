extern crate alloc;
use crate::println;
use alloc::{format, vec::Vec};
use bincode::{
    de::read::Reader,
    enc::write::Writer,
    error::{DecodeError, EncodeError},
};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::{FlashStorage, FlashStorageError};
use frostsnap_core::schnorr_fun::fun::Scalar;

pub const NVS_PARTITION_START: u32 = 0x9000;
pub const NVS_PARTITION_SIZE: usize = 0x6000;

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct State {
    #[bincode(with_serde)]
    pub secret: Scalar,
}

pub struct EspNvsRw<'a> {
    nvs: &'a mut EspNvs,
    pos: u32,
}

pub struct EspNvs {
    flash: FlashStorage,
    start_pos: u32,
}

impl EspNvs {
    pub fn new(flash: FlashStorage, start_pos: u32) -> Self {
        Self { flash, start_pos }
    }

    pub fn rw(&mut self) -> EspNvsRw<'_> {
        let start = self.start_pos;
        EspNvsRw {
            nvs: self,
            pos: start,
        }
    }

    pub fn erase(&mut self) -> Result<(), FlashStorageError> {
        let buf = [0u8; NVS_PARTITION_SIZE];
        self.flash.write(self.start_pos, &buf)
    }

    pub fn load(&mut self) -> Result<State, DecodeError> {
        bincode::decode_from_reader(self.rw(), bincode::config::standard())
    }

    pub fn save(&mut self, state: &State) -> Result<(), EncodeError> {
        bincode::encode_into_writer(state, self.rw(), bincode::config::standard())
    }
}

impl<'a> Reader for EspNvsRw<'a> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        self.nvs
            .flash
            .read(self.pos, bytes)
            .map_err(|e| DecodeError::OtherString(format!("Flash read error {:?}", e)))?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}

impl<'a> Writer for EspNvsRw<'a> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.nvs
            .flash
            .write(self.pos, bytes)
            .map_err(|e| EncodeError::OtherString(format!("Flash write error {:?}", e)))?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}
