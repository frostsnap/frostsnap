use bitcoin::{bip32::*, secp256k1, Network};
use schnorr_fun::{
    frost::FrostKey,
    fun::{g, marker::*, Point, Scalar, G},
};

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub enum AppTweak {
    TestMessage,
    Bitcoin { bip32_path: alloc::vec::Vec<u32> },
    Nostr,
}

impl AppTweak {
    pub fn kind(&self) -> AppTweakKind {
        match self {
            AppTweak::Bitcoin { .. } => AppTweakKind::Bitcoin,
            AppTweak::Nostr => AppTweakKind::Nostr,
            AppTweak::TestMessage => AppTweakKind::TestMessage,
        }
    }
}

impl AppTweakKind {
    pub fn app_string(&self) -> &'static str {
        match self {
            AppTweakKind::Bitcoin { .. } => "frostsnap/bitcoin",
            AppTweakKind::TestMessage => "frostsnap/test-message",
            AppTweakKind::Nostr => "frostsnap/nostr",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppTweakKind {
    Bitcoin,
    Nostr,
    TestMessage,
}

/// Encapsulates bip32 derivations on a key
pub struct Xpub<T> {
    key: T,
    xpub: bitcoin::bip32::Xpub,
}

pub trait TweakableKey: Clone {
    type XOnly;
    fn to_libsecp_key(&self) -> secp256k1::PublicKey;
    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey;
    fn tweak(self, tweak: Scalar<Public, Zero>) -> Self;
    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly;
    fn into_xonly(self) -> Self::XOnly;
    fn app_tweak_and_expand(&self, app: AppTweakKind) -> (Self, [u8; 32]) {
        use bitcoin::hashes::*;
        let mut hmac_engine = HmacEngine::<sha512::Hash>::new(app.app_string().as_bytes());
        hmac_engine.input(self.to_libsecp_key().serialize().as_ref());
        let result = Hmac::from_engine(hmac_engine);
        let app_key = self
            .clone()
            .tweak(Scalar::from_slice_mod_order(&result[0..32]).expect("is 32 bytes long"));
        let mut extra = [0u8; 32];
        extra.copy_from_slice(&result[32..64]);

        (app_key, extra)
    }
}

impl TweakableKey for FrostKey<Normal> {
    type XOnly = FrostKey<EvenY>;

    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        self.public_key().to_libsecp_key()
    }

    fn tweak(self, tweak: Scalar<Public, Zero>) -> Self {
        FrostKey::<Normal>::tweak(self, tweak).expect("computationally unreachable")
    }

    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly {
        self.into_xonly_key()
            .tweak(tweak)
            .expect("if tweak is a hash this should be unreachable")
    }

    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        self.public_key().to_libsecp_xonly()
    }

    fn into_xonly(self) -> Self::XOnly {
        self.into_xonly_key()
    }
}

impl TweakableKey for Point {
    type XOnly = Point<EvenY>;

    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        secp256k1::PublicKey::from_slice(self.to_bytes().as_ref()).unwrap()
    }

    fn tweak(self, tweak: Scalar<Public, Zero>) -> Self {
        g!(self + tweak * G)
            .normalize()
            .non_zero()
            .expect("if tweak is a hash this should be unreachable")
    }

    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly {
        let (even_y, _) = self.into_point_with_even_y();
        let (tweaked_even_y, _) = g!(even_y + tweak * G)
            .normalize()
            .non_zero()
            .expect("if tweak is a hash this should be unreachable")
            .into_point_with_even_y();
        tweaked_even_y
    }

    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        secp256k1::XOnlyPublicKey::from_slice(self.to_xonly_bytes().as_ref()).unwrap()
    }

    fn into_xonly(self) -> Self::XOnly {
        let (even_y, _) = self.into_point_with_even_y();
        even_y
    }
}

impl<T: TweakableKey> Xpub<T> {
    pub fn new(key: T, chaincode: [u8; 32]) -> Self {
        Xpub {
            xpub: bitcoin::bip32::Xpub {
                network: Network::Bitcoin,
                depth: 0,
                child_number: ChildNumber::from(0u32),
                parent_fingerprint: Fingerprint::default(),
                public_key: key.to_libsecp_key(),
                chain_code: ChainCode::from(chaincode),
            },
            key,
        }
    }

    /// Does non-hardended derivation
    pub fn derive_bip32(&mut self, segments: &[u32]) {
        for child in segments {
            let child_number = ChildNumber::Normal { index: *child };
            let (tweak, chain_code) = self
                .xpub
                .ckd_pub_tweak(child_number)
                .expect("can only fail if you do non-hardended derivation");
            self.key = self
                .key
                .clone()
                .tweak(Scalar::<Public, _>::from_slice_mod_order(&tweak[..]).expect("32 bytes"));
            self.xpub = bitcoin::bip32::Xpub {
                network: self.xpub.network,
                depth: self.xpub.depth + 1,
                parent_fingerprint: self.xpub.fingerprint(),
                child_number,
                public_key: self.key.to_libsecp_key(),
                chain_code,
            }
        }
    }

    pub fn key(&self) -> &T {
        &self.key
    }

    pub fn into_key(self) -> T {
        self.key
    }

    pub fn xpub(&self) -> &bitcoin::bip32::Xpub {
        &self.xpub
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;
    use bitcoin::secp256k1::Secp256k1;
    use schnorr_fun::frost;

    #[test]
    pub fn bip32_derivation_matches_rust_bitcoin() {
        let frost = frost::new_with_deterministic_nonces::<sha2::Sha256>();
        let (frost_key, _) = frost.simulate_keygen(3, 5, &mut rand::thread_rng());
        let (app_key, chaincode) = frost_key.app_tweak_and_expand(AppTweakKind::Bitcoin);

        let mut app_xpub = Xpub::new(app_key, chaincode);
        let secp = Secp256k1::verification_only();
        let xpub = app_xpub.xpub();
        let path = [1337u32, 42, 0];
        let child_path = path
            .iter()
            .map(|i| ChildNumber::Normal { index: *i })
            .collect::<Vec<_>>();
        let derived_xpub = xpub.derive_pub(&secp, &child_path).unwrap();
        app_xpub.derive_bip32(&path);

        assert_eq!(app_xpub.xpub(), &derived_xpub);
    }
}
