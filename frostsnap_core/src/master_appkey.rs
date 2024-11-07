use crate::{
    tweak::{AppTweakKind, TweakableKey, Xpub},
    KeyId,
};
use alloc::string::String;
use bitcoin::key::Secp256k1;
use schnorr_fun::fun::prelude::*;

/// A 65-byte encoded point and chaincode. This is a byte array because it's easier to pass around byte
/// arrays via FFI rather than `Point`.
#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct MasterAppkey(pub [u8; 65]);

impl MasterAppkey {
    pub fn derive_from_rootkey(rootkey: Point) -> Self {
        let xpub = Xpub::<Point>::from_rootkey(rootkey).rootkey_to_master_appkey();
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

    pub fn from_xpub_unchecked<K: TweakableKey>(master_appkey_xpub: &Xpub<K>) -> Self {
        let mut bytes = [0u8; 65];
        bytes[..33].copy_from_slice(master_appkey_xpub.key.to_key().to_bytes().as_ref());
        bytes[33..].copy_from_slice(master_appkey_xpub.chaincode.as_ref());
        Self(bytes)
    }

    pub fn to_redacted_string(&self) -> String {
        use alloc::string::ToString;
        let full = self.to_string();
        let redacted = format!("{}...{}", &full[..4], &full[full.len() - 4..]);
        redacted
    }

    pub fn key_id(&self) -> KeyId {
        KeyId::from_master_appkey(*self)
    }

    pub fn derive_appkey<C: bitcoin::secp256k1::Context + bitcoin::secp256k1::Verification>(
        &self,
        secp: &Secp256k1<C>,
        appkey_kind: AppTweakKind,
        network: bitcoin::NetworkKind,
    ) -> bitcoin::bip32::Xpub {
        let app_key_xpub = self.to_xpub().to_bitcoin_xpub_with_lies(network);
        app_key_xpub
            .derive_pub(secp, &appkey_kind.derivation_path())
            .expect("no hardened derivation")
    }
}

crate::impl_display_debug_serialize! {
    fn to_bytes(master_appkey: &MasterAppkey) -> [u8;65] {
        master_appkey.0
    }
}

crate::impl_fromstr_deserialize! {
    name => "master_appkey",
    fn from_bytes(bytes: [u8;65]) -> Option<MasterAppkey> {
        let _ = Point::<Normal, Public, NonZero>::from_slice(&bytes[0..33])?;
        Some(MasterAppkey(bytes))
    }
}
