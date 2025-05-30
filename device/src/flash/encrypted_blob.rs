use alloc::vec::Vec;
use embedded_storage::nor_flash::NorFlash;
use frostsnap_comms::{HasMagicBytes, MagicBytes, MAGIC_BYTES_LEN};
use frostsnap_core::schnorr_fun::Signature;
use frostsnap_embedded::{AbSlot, FlashPartition};

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct BlobMagicBytes;
impl HasMagicBytes for BlobMagicBytes {
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = *b"fsblob0";
    const VERSION_SIGNAL: frostsnap_comms::MagicBytesVersion = 0;
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct Blob {
    pub magic_bytes: MagicBytes<BlobMagicBytes>,
    pub body: BlobBody,
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum BlobBody {
    V0 {
        encrypted_params: Vec<u8>,
        hmac_key: [u8; 32],
        genuine_key: [u8; 32],
        genuine_certificate: Signature,
    },
}

impl Blob {
    pub fn new(
        encrypted_params: Vec<u8>,
        hmac_key: [u8; 32],
        genuine_key: [u8; 32],
        genuine_certificate: Signature,
    ) -> Self {
        Blob {
            magic_bytes: Default::default(),
            body: BlobBody::V0 {
                encrypted_params,
                hmac_key,
                genuine_key,
                genuine_certificate,
            },
        }
    }
}

pub struct FlashBlob<'a, S> {
    ab: AbSlot<'a, S>,
}

impl<'a, S: NorFlash> FlashBlob<'a, S> {
    pub fn new(mut flash: FlashPartition<'a, S>) -> Self {
        flash.tag = "blob";
        let ab = AbSlot::new(flash);
        Self { ab }
    }

    pub fn read_blob(&self) -> Option<Blob> {
        self.ab.read()
    }

    pub fn write_blob(&self, blob: &Blob) {
        self.ab.write(blob)
    }
}
