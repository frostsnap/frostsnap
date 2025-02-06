extern crate alloc;
use core::cell::RefCell;

use alloc::{rc::Rc, string::String, vec::Vec};
use bincode::{
    de::read::Reader,
    enc::write::Writer,
    error::{DecodeError, EncodeError},
};
use embedded_storage::nor_flash::{self, NorFlash};
use esp_storage::FlashStorageError;
use frostsnap_comms::BINCODE_CONFIG;
use frostsnap_core::{device, schnorr_fun::fun::Scalar};

use crate::flash_nonce_slot::FLASH_NONCE_SLOTS_SIZE;

const NVS_PARTITION_START: u32 = 0x3D0000;
const NVS_PARTITION_SIZE: u32 = 0x30000;
pub const APP_STORAGE_END: u32 = NVS_PARTITION_START + NVS_PARTITION_SIZE - FLASH_NONCE_SLOTS_SIZE;
const HEADER_LEN: usize = 256;
const DATA_START: u32 = NVS_PARTITION_START + HEADER_LEN as u32;
const MAGIC_BYTES_LEN: usize = 8;
const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = *b"fsheader";
const WORD_SIZE: usize = core::mem::size_of::<u32>();

pub struct DeviceStorage<S> {
    flash: Rc<RefCell<S>>,
    word_pos: usize,
    write_buffer: Vec<u8>,
    read_buffer: Vec<u8>,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum Change {
    Core(device::Mutation),
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

impl<S: NorFlash<Error = FlashStorageError>> DeviceStorage<S> {
    pub fn new(flash: Rc<RefCell<S>>) -> Self {
        assert_eq!(WORD_SIZE, S::WRITE_SIZE);
        Self {
            flash,
            word_pos: DATA_START as usize / WORD_SIZE,
            // This is just the starting capacity
            write_buffer: Vec::with_capacity(1024),
            read_buffer: Vec::with_capacity(1024),
        }
    }

    pub fn read_header(&mut self) -> Result<Option<Header>, FlashStorageError> {
        let mut header_bytes = [0u8; HEADER_LEN];
        self.flash
            .borrow_mut()
            .read(NVS_PARTITION_START, &mut header_bytes)?;
        match bincode::decode_from_slice::<(MagicBytes, Header), _>(&header_bytes, BINCODE_CONFIG) {
            Ok(((_, header), _)) => Ok(Some(header)),
            Err(bincode::error::DecodeError::Other("invalid magic bytes")) => Ok(None),
            Err(e) => panic!("nvs: invalid header. {e}"),
        }
    }

    pub fn write_header(&mut self, header: Header) -> Result<(), FlashStorageError> {
        let mut header_bytes = [0u8; HEADER_LEN];
        bincode::encode_into_slice(&(MagicBytes, header), &mut header_bytes, BINCODE_CONFIG)
            .expect("header shouldn't be too long");
        self.flash
            .borrow_mut()
            .write(NVS_PARTITION_START, &header_bytes)
    }

    /// The layout of the entries are word aligned and length prefixed. So the first entry has a
    /// four byte little endian entry length (in words), and then the main body of the item bincode
    /// encoded. Bincode doesn't care if it doesn't use all the bytes so there's no need to know
    /// exactly where it ends.
    pub fn append(
        &mut self,
        changes: impl IntoIterator<Item = Change>,
    ) -> Result<(), FlashStorageError> {
        for change in changes.into_iter() {
            self.write_buffer.clear();
            // placeholder for the length
            self.write_buffer
                .extend(core::iter::repeat_n(0x00, WORD_SIZE));
            bincode::encode_into_writer(change, BufWriter(&mut self.write_buffer), BINCODE_CONFIG)
                .expect("can bincode encode type to buffer");

            let padding_needed = (WORD_SIZE - (self.write_buffer.len() % WORD_SIZE)) % WORD_SIZE;

            self.write_buffer
                .extend(core::iter::repeat_n(0xff, padding_needed));
            assert_eq!(self.write_buffer.len() % WORD_SIZE, 0);

            let word_length: u32 = (self.write_buffer.len() / WORD_SIZE)
                .try_into()
                .expect("flash item too large");

            self.write_buffer[0..WORD_SIZE].copy_from_slice(
                (word_length - 1/* don't include the length word */)
                    .to_le_bytes()
                    .as_slice(),
            );
            let byte_pos = (self.word_pos * WORD_SIZE) as u32;
            let end_pos = byte_pos + self.write_buffer.len() as u32;
            if end_pos > APP_STORAGE_END {
                return Err(FlashStorageError::OutOfBounds);
            }
            nor_flash::NorFlash::write(
                &mut *self.flash.borrow_mut(),
                byte_pos,
                &self.write_buffer[..],
            )?;
            self.word_pos += word_length as usize;
            self.write_buffer.clear();
        }

        Ok(())
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Change> + '_ {
        core::iter::from_fn(move || {
            let mut length_buf = [0u8; WORD_SIZE];
            let byte_pos = self.word_pos * WORD_SIZE;
            if let Err(e) = self
                .flash
                .borrow_mut()
                .read(byte_pos as u32, &mut length_buf[..])
            {
                panic!(
                    "failed to read {} bytes at {}: {e:?}",
                    length_buf.len(),
                    byte_pos
                );
            }
            if length_buf == [0xff; WORD_SIZE] {
                return None;
            }
            let word_length = u32::from_le_bytes(length_buf);
            self.read_buffer.clear();
            self.read_buffer.extend(core::iter::repeat_n(
                0xff,
                (word_length as usize) * WORD_SIZE,
            ));
            let body_byte_pos = byte_pos + WORD_SIZE;
            self.flash
                .borrow_mut()
                .read(body_byte_pos as u32, &mut self.read_buffer)
                .expect("unabled to read entry from flash");
            self.word_pos += word_length as usize + 1;
            match bincode::decode_from_slice(&self.read_buffer[..], bincode::config::standard()) {
                Ok((item, _)) => Some(item),
                Err(e) => panic!(
                    "failed to decode flash entry of length {}: {}",
                    word_length as usize * WORD_SIZE,
                    e
                ),
            }
        })
    }

    pub fn erase(&mut self) -> Result<(), FlashStorageError> {
        nor_flash::NorFlash::erase(
            &mut *self.flash.borrow_mut(),
            NVS_PARTITION_START,
            NVS_PARTITION_START + NVS_PARTITION_SIZE,
        )
    }
}

struct BufWriter<'a>(&'a mut Vec<u8>);

impl Writer for BufWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.0.extend(bytes);
        Ok(())
    }
}
