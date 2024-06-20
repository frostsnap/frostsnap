extern crate alloc;
use alloc::{format, string::String};
use bincode::{
    de::read::Reader,
    enc::write::Writer,
    error::{DecodeError, EncodeError},
};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::{FlashStorage, FlashStorageError};
use frostsnap_core::{message::DeviceToStorageMessage, schnorr_fun::fun::Scalar};

const NVS_PARTITION_START: u32 = 0x3D0000;
const _NVS_PARTITION_SIZE: usize = 0x30000;
const HEADER_LEN: usize = 256;
const DATA_START: u32 = NVS_PARTITION_START + HEADER_LEN as u32;
const MAGIC_BYTES_LEN: usize = 8;
const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = *b"fsheader";

pub struct DeviceStorage {
    flash: FlashStorage,
    pos: u32,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum Change {
    Core(DeviceToStorageMessage),
    Name(String),
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct Header {
    pub secret_key: Scalar,
}

#[derive(Clone, Copy, Debug, Default)]
struct MagicBytes;

impl bincode::Encode for MagicBytes {
    fn encode<E: bincode::enc::Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        encoder.writer().write(MAGIC_BYTES.as_ref())
    }
}

impl bincode::Decode for MagicBytes {
    fn decode<D: bincode::de::Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let mut bytes = [0u8; MAGIC_BYTES_LEN];
        decoder.reader().read(&mut bytes)?;
        if bytes == MAGIC_BYTES {
            Ok(MagicBytes)
        } else {
            Err(bincode::error::DecodeError::Other("invalid magic bytes"))
        }
    }
}

impl DeviceStorage {
    pub fn new(flash: FlashStorage) -> Self {
        Self {
            flash,
            pos: DATA_START,
        }
    }

    pub fn read_header(&mut self) -> Result<Option<Header>, FlashStorageError> {
        let mut header_bytes = [0u8; HEADER_LEN];
        self.flash.read(NVS_PARTITION_START, &mut header_bytes)?;
        match bincode::decode_from_slice::<(MagicBytes, Header), _>(
            &header_bytes,
            bincode::config::standard(),
        ) {
            Ok(((_, header), _)) => Ok(Some(header)),
            Err(bincode::error::DecodeError::Other("invalid magic bytes")) => Ok(None),
            Err(e) => panic!("nvs: invalid header. {e}"),
        }
    }

    pub fn write_header(&mut self, header: Header) -> Result<(), FlashStorageError> {
        let mut header_bytes = [0u8; HEADER_LEN];
        bincode::encode_into_slice(
            &(MagicBytes, header),
            &mut header_bytes,
            bincode::config::standard(),
        )
        .expect("header shouldn't be too long");
        self.flash.write(NVS_PARTITION_START, &header_bytes)
    }

    pub fn push(&mut self, change: Change) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(change, self, bincode::config::standard())
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Change> + '_ {
        self.pos = DATA_START;
        core::iter::from_fn(move || {
            let pos_before_read = self.pos;
            match bincode::decode_from_reader(&mut *self, bincode::config::standard()) {
                Ok(change) => Some(change),
                Err(_) => {
                    self.pos = pos_before_read;
                    None
                }
            }
        })
    }

    pub fn flash_mut(&mut self) -> &mut FlashStorage {
        &mut self.flash
    }
}

impl Reader for DeviceStorage {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        self.flash
            .read(self.pos, bytes)
            .map_err(|e| DecodeError::OtherString(format!("Flash read error {:?}", e)))?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}

impl Writer for DeviceStorage {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.flash
            .write(self.pos, bytes)
            .map_err(|e| EncodeError::OtherString(format!("Flash write error {:?}", e)))?;
        self.pos += bytes.len() as u32;
        Ok(())
    }
}
