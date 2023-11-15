use schnorr_fun::{
    frost::FrostKey,
    fun::{
        hash::{HashAdd, Tag},
        marker::Normal,
        Point,
    },
};
use sha2::Digest;

use crate::{impl_display_debug_serialize, impl_fromstr_deserialize};

#[derive(Clone, Copy)]
pub struct KeyId(pub [u8; 32]);

impl_display_debug_serialize! {
    fn to_bytes(key_id: &KeyId) -> [u8;32] {
        key_id.0
    }
}

impl_fromstr_deserialize! {
    name => "Frostsnap key id",
    fn from_bytes(bytes: [u8;32]) -> KeyId {
        KeyId(bytes)
    }
}

impl KeyId {
    pub fn from_pubkey(key: Point) -> Self {
        KeyId(
            sha2::Sha256::default()
                .tag(b"frostsnap/keyid")
                .add(key)
                .finalize()
                .into(),
        )
    }
}

pub trait FrostKeyExt {
    fn key_id(&self) -> KeyId;
}

impl FrostKeyExt for FrostKey<Normal> {
    fn key_id(&self) -> KeyId {
        KeyId::from_pubkey(self.public_key())
    }
}
