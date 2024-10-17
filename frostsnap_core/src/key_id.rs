use alloc::string::String;
use schnorr_fun::{
    frost::{PairedSecretShare, SharedKey},
    fun::prelude::*,
};

use crate::{impl_display_debug_serialize, impl_fromstr_deserialize};

#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct KeyId(pub [u8; 33]);

impl_display_debug_serialize! {
    fn to_bytes(key_id: &KeyId) -> [u8;33] {
        key_id.0
    }
}

impl_fromstr_deserialize! {
    name => "Frostsnap key id",
    fn from_bytes(bytes: [u8;33]) -> KeyId {
        KeyId(bytes)
    }
}

impl KeyId {
    pub fn from_root_pubkey(key: Point) -> Self {
        KeyId(key.to_bytes())
    }

    pub fn to_root_pubkey(&self) -> Option<Point> {
        Point::from_bytes(self.0)
    }

    pub fn to_redacted_string(&self) -> String {
        use alloc::string::ToString;
        let full = self.to_string();
        let redacted = format!("{}...{}", &full[..4], &full[full.len() - 4..]);
        redacted
    }
}

pub trait FrostKeyExt {
    fn key_id(&self) -> KeyId;
}

impl FrostKeyExt for SharedKey {
    fn key_id(&self) -> KeyId {
        KeyId::from_root_pubkey(self.public_key())
    }
}

impl FrostKeyExt for PairedSecretShare {
    fn key_id(&self) -> KeyId {
        KeyId::from_root_pubkey(self.public_key())
    }
}

impl FrostKeyExt for Point {
    fn key_id(&self) -> KeyId {
        KeyId::from_root_pubkey(*self)
    }
}

impl PartialEq<Point> for KeyId {
    fn eq(&self, other: &Point) -> bool {
        other.to_bytes() == self.0
    }
}

impl PartialEq<KeyId> for Point {
    fn eq(&self, other: &KeyId) -> bool {
        other.eq(self)
    }
}
