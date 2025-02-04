use embedded_storage::nor_flash::NorFlash;
use frostsnap_comms::{HasMagicBytes, MagicBytes, MAGIC_BYTES_LEN};
use frostsnap_core::schnorr_fun::fun::{KeyPair, Scalar};
use frostsnap_embedded::{AbSlot, FlashPartition};

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
    pub fn device_keypair(&self) -> KeyPair {
        KeyPair::new(match self.body {
            HeaderBody::V0 { secret_key } => secret_key,
        })
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
pub enum HeaderBody {
    V0 { secret_key: Scalar },
}

impl Header {
    pub fn new(secret_key: Scalar) -> Self {
        Header {
            magic_bytes: Default::default(),
            body: HeaderBody::V0 { secret_key },
        }
    }

    pub fn init(rng: &mut impl rand_core::RngCore) -> Self {
        Header::new(Scalar::random(rng))
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

    pub fn read_or_init(&self, rng: &mut impl rand_core::RngCore) -> Header {
        match self.read_header() {
            Some(header) => header,
            None => self.init(rng),
        }
    }
}
