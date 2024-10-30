use crate::{
    tweak::{TweakableKey, Xpub},
    KeyId,
};
use alloc::string::String;
use schnorr_fun::fun::prelude::*;

/// A 65-byte encoded point and chaincode. This is exists because it's easier to pass around byte
/// arrays via FFI rather than `Point`.
#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Appkey(pub [u8; 65]);

impl Appkey {
    pub fn derive_from_rootkey(rootkey: Point) -> Self {
        let xpub = Xpub::<Point>::from_rootkey(rootkey).rootkey_to_appkey();
        Self::from_xpub_unchecked(&xpub)
    }

    pub fn to_xpub(&self) -> Xpub<Point> {
        let point = Point::from_slice(&self.0[..33]).expect("invariant");
        let chaincode = self.0[33..].try_into().expect("correct length");
        Xpub {
            key: point,
            chaincode,
        }
    }

    pub fn from_xpub_unchecked<K: TweakableKey>(appkey_xpub: &Xpub<K>) -> Self {
        let mut bytes = [0u8; 65];
        bytes[..33].copy_from_slice(appkey_xpub.key.to_key().to_bytes().as_ref());
        bytes[33..].copy_from_slice(appkey_xpub.chaincode.as_ref());
        Self(bytes)
    }

    pub fn to_redacted_string(&self) -> String {
        use alloc::string::ToString;
        let full = self.to_string();
        let redacted = format!("{}...{}", &full[..4], &full[full.len() - 4..]);
        redacted
    }

    pub fn key_id(&self) -> KeyId {
        KeyId::from_appkey(*self)
    }
}

crate::impl_display_debug_serialize! {
    fn to_bytes(appkey: &Appkey) -> [u8;65] {
        appkey.0
    }
}

crate::impl_fromstr_deserialize! {
    name => "appkey",
    fn from_bytes(bytes: [u8;65]) -> Option<Appkey> {
        let _ = Point::<Normal, Public, NonZero>::from_slice(&bytes[0..33])?;
        Some(Appkey(bytes))
    }
}
