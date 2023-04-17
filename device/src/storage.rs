extern crate alloc;
use crate::println;
use alloc::{format, vec::Vec};
use bincode::{
    de::{read::Reader},
    enc::{write::Writer},
    error::{DecodeError, EncodeError},
};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;
use frostsnap_core::schnorr_fun::fun::Scalar;

#[derive(bincode::Encode, bincode::Decode, Debug, Clone)]
pub struct State {
    #[bincode(with_serde)]
    pub secret: Scalar,
}

pub struct EspNvsRw<'a> {
    nvs: &'a mut EspNvs,
    pos: u32,
}

// #[derive(Debug)]
// pub enum EspNVSError {
//     ReadError,
//     WriteError,
//     EncodeError(EncodeError),
//     DecodeError(DecodeError),
// }

pub struct EspNvs {
    flash: FlashStorage,
    start_pos: u32
}

impl EspNvs
// where
//     D: Decode
{
    pub fn new(flash: FlashStorage, start_pos: u32) -> Self {
        // let mut flash = FlashStorage::new();
        Self { flash, start_pos }
    }

    pub fn rw(&mut self) -> EspNvsRw<'_> {
        let start = self.start_pos;
        EspNvsRw {
            nvs: self,
            pos: start,
        }
    }

    // pub fn load(&mut self) -> Result<State, DecodeError> {
    //     let magic
    //     bincode::decode_from_reader(self.rw(), bincode::con)
    // }

    // is_factory

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
