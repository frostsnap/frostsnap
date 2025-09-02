use embedded_storage::nor_flash::NorFlash;
use frostsnap_comms::{HasMagicBytes, MagicBytes, MAGIC_BYTES_LEN};
use frostsnap_core::schnorr_fun::fun::{KeyPair, Scalar};
use frostsnap_embedded::{AbSlot, FlashPartition};

use crate::efuse::EfuseHmacKey;

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct HeaderMagicBytes;
impl HasMagicBytes for HeaderMagicBytes {
    const MAGIC_BYTES: [u8; MAGIC_BYTES_LEN] = *b"fshead0";
    const VERSION_SIGNAL: frostsnap_comms::MagicBytesVersion = 0;
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub struct Header {
    pub magic_bytes: MagicBytes<HeaderMagicBytes>,
    pub body: HeaderBody,
}

impl Header {
    pub fn device_keypair(&self, hmac: &mut EfuseHmacKey) -> KeyPair {
        KeyPair::new(match self.body {
            HeaderBody::V0 { device_id_seed } => {
                let secret_scalar_bytes = hmac
                    .hash("frostsnap-device-keypair", &device_id_seed)
                    .unwrap();
                Scalar::from_slice_mod_order(&secret_scalar_bytes)
                    .expect("just got 32 bytes from fixed entropy hash")
                    .non_zero()
                    .expect("built using random bytes")
            }
        })
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum HeaderBody {
    V0 { device_id_seed: [u8; 32] },
}

impl Header {
    pub fn new(device_id_seed: [u8; 32]) -> Self {
        Header {
            magic_bytes: Default::default(),
            body: HeaderBody::V0 { device_id_seed },
        }
    }

    pub fn init(rng: &mut impl rand_core::RngCore) -> Self {
        let mut device_id_seed = [0u8; 32];
        rng.fill_bytes(&mut device_id_seed);
        Header::new(device_id_seed)
    }
}

pub struct FlashHeader<'a, S> {
    ab: AbSlot<'a, S>,
}

impl<'a, S: NorFlash> FlashHeader<'a, S> {
    pub fn new(mut flash: FlashPartition<'a, S>) -> Self {
        flash.tag = "header";
        let ab = AbSlot::new(flash);
        Self { ab }
    }

    pub fn read_header(&self) -> Option<Header> {
        self.ab.read()
    }

    pub fn write_header(&self, header: &Header) {
        self.ab.write(header)
    }

    pub fn init(&self, rng: &mut impl rand_core::RngCore) -> Header {
        let header = Header::init(rng);
        self.write_header(&header);
        header
    }
}
