use bitcoin::{
    bip32::*,
    hashes::{sha512, Hash, HashEngine, Hmac, HmacEngine},
    secp256k1,
};
use schnorr_fun::{
    frost::{PairedSecretShare, SharedKey},
    fun::{g, marker::*, Point, Scalar, G},
};

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub enum Account {
    Segwitv1 = 0,
}

impl Account {
    pub fn derivation_path(&self) -> DerivationPath {
        DerivationPath::master().child(ChildNumber::Normal {
            index: *self as u32,
        })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub enum Keychain {
    External = 0,
    Internal = 1,
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, PartialOrd, Ord)]
pub enum AppTweak {
    TestMessage,
    Bitcoin(AppBip32Path),
    Nostr,
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub struct AppBip32Path {
    pub account_keychain: AppAccountKeychain,
    pub index: u32,
}

impl AppBip32Path {
    pub fn external(index: u32) -> Self {
        Self {
            account_keychain: AppAccountKeychain::external(),
            index,
        }
    }

    pub fn internal(index: u32) -> Self {
        Self {
            account_keychain: AppAccountKeychain::internal(),
            index,
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub struct AppAccountKeychain {
    pub account: Account,
    pub keychain: Keychain,
}

impl AppAccountKeychain {
    pub fn external() -> Self {
        Self {
            account: Account::Segwitv1,
            keychain: Keychain::External,
        }
    }

    pub fn internal() -> Self {
        Self {
            account: Account::Segwitv1,
            keychain: Keychain::Internal,
        }
    }
}

impl AppBip32Path {
    pub fn to_u32_array(&self) -> [u32; 3] {
        [
            self.account_keychain.account as u32,
            self.account_keychain.keychain as u32,
            self.index,
        ]
    }

    pub fn from_u32_slice(path: &[u32]) -> Option<Self> {
        if path.len() != 3 {
            return None;
        }

        let account = match path[0] {
            0 => Account::Segwitv1,
            _ => return None,
        };

        let keychain = match path[1] {
            0 => Keychain::External,
            1 => Keychain::Internal,
            _ => return None,
        };

        let _check_it = ChildNumber::from_normal_idx(path[2]).ok()?;
        let index = path[2];

        Some(AppBip32Path {
            account_keychain: AppAccountKeychain { account, keychain },
            index,
        })
    }
}

impl AppTweak {
    pub fn kind(&self) -> AppTweakKind {
        match self {
            AppTweak::Bitcoin { .. } => AppTweakKind::Bitcoin,
            AppTweak::Nostr => AppTweakKind::Nostr,
            AppTweak::TestMessage => AppTweakKind::TestMessage,
        }
    }

    pub fn derive_xonly_key<K: TweakableKey>(&self, approot: &Xpub<K>) -> K::XOnly {
        let mut xpub_for_app = approot.clone();
        xpub_for_app.derive_bip32(&[self.kind() as u32]);

        match &self {
            AppTweak::Bitcoin(bip32_path) => {
                xpub_for_app.derive_bip32(&bip32_path.to_u32_array());
                let derived_key = xpub_for_app.into_key();
                let tweak = bitcoin::taproot::TapTweakHash::from_key_and_tweak(
                    derived_key.to_libsecp_xonly(),
                    None,
                )
                .to_scalar();
                derived_key.into_xonly_with_tweak(
                    Scalar::<Public, _>::from_bytes_mod_order(tweak.to_be_bytes())
                        .non_zero()
                        .expect("computationally unreachable"),
                )
            }
            AppTweak::Nostr => xpub_for_app.into_key().into_xonly(),
            AppTweak::TestMessage => xpub_for_app.into_key().into_xonly(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppTweakKind {
    Bitcoin = 0,
    TestMessage = 1,
    Nostr = 2,
}

pub trait TweakableKey: Clone {
    type XOnly;
    fn to_key(&self) -> Point;
    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        self.to_key().into()
    }
    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        self.to_key().to_libsecp_xonly()
    }
    fn tweak(self, tweak: Scalar<Public, Zero>) -> Self;
    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly;
    fn into_xonly(self) -> Self::XOnly;
}

impl TweakableKey for SharedKey<Normal> {
    type XOnly = SharedKey<EvenY>;

    fn to_key(&self) -> Point {
        self.public_key()
    }

    fn tweak(self, tweak: Scalar<Public, Zero>) -> Self {
        self.homomorphic_add(tweak)
            .non_zero()
            .expect("computationally unreachable")
    }

    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly {
        self.into_xonly()
            .homomorphic_add(tweak)
            .non_zero()
            .expect("computationally unreachable")
            .into_xonly()
    }

    fn into_xonly(self) -> Self::XOnly {
        SharedKey::into_xonly(self)
    }
}

impl TweakableKey for PairedSecretShare {
    type XOnly = PairedSecretShare<EvenY>;

    fn to_key(&self) -> Point {
        self.public_key().to_key()
    }

    fn tweak(self, tweak: Scalar<Public, Zero>) -> Self {
        self.homomorphic_add(tweak)
            .non_zero()
            .expect("computationally unreachable")
    }

    fn into_xonly_with_tweak(self, tweak: Scalar<Public>) -> Self::XOnly {
        self.into_xonly()
            .homomorphic_add(tweak)
            .non_zero()
            .expect("computationally unreachable")
            .into_xonly()
    }

    fn into_xonly(self) -> Self::XOnly {
        PairedSecretShare::into_xonly(self)
    }
}

impl TweakableKey for Point {
    type XOnly = Point<EvenY>;

    fn to_key(&self) -> Point {
        *self
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
    pub fn from_root_key(root_key: T) -> Self {
        Xpub {
            chaincode: [0u8; 32],
            key: root_key,
        }
    }

    pub fn root_key_to_appkey(&self) -> Xpub<T> {
        let mut appkey = self.clone();
        appkey.derive_bip32(&[0]);
        appkey
    }

    pub fn new(key: T, chaincode: [u8; 32]) -> Self {
        Xpub { chaincode, key }
    }

    /// Does non-hardended derivation
    pub fn derive_bip32(&mut self, segments: &[u32]) {
        for child in segments {
            let mut hmac_engine: HmacEngine<sha512::Hash> = HmacEngine::new(&self.chaincode[..]);
            hmac_engine.input(&self.key().to_key().to_bytes());
            hmac_engine.input(&child.to_be_bytes());
            let hmac_result: Hmac<sha512::Hash> = Hmac::from_engine(hmac_engine);

            self.key = self.key.clone().tweak(
                Scalar::<Public, _>::from_slice_mod_order(&hmac_result[..32]).expect("32 bytes"),
            );
            self.chaincode.copy_from_slice(&hmac_result[32..]);
        }
    }

    pub fn key(&self) -> &T {
        &self.key
    }

    pub fn into_key(self) -> T {
        self.key
    }
}

/// Xpub to do bip32 deriviation without all the nonsense.
#[derive(
    Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, bincode::Encode, bincode::Decode, Debug,
)]
pub struct Xpub<T> {
    pub key: T,
    pub chaincode: [u8; 32],
}

impl Xpub<SharedKey> {
    pub fn public_key(&self) -> Xpub<Point> {
        Xpub {
            key: self.key.public_key(),
            chaincode: self.chaincode,
        }
    }
}

impl<T: TweakableKey> Xpub<T> {
    /// Create a rust bitcoin xpub lying about the fields we don't care about
    pub fn to_bitcoin_xpub_with_lies(
        &self,
        network_kind: bitcoin::NetworkKind,
    ) -> bitcoin::bip32::Xpub {
        bitcoin::bip32::Xpub {
            network: network_kind,
            // note below this is a lie and shouldn't matter VVV
            depth: 0,
            parent_fingerprint: Fingerprint::default(),
            child_number: ChildNumber::from_normal_idx(0).unwrap(),
            // ^^^ above is a lie and shouldn't matter
            public_key: self.key.to_libsecp_key(),
            chain_code: ChainCode::from(self.chaincode),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;
    use bitcoin::secp256k1::Secp256k1;
    use schnorr_fun::frost::chilldkg::encpedpop;

    #[test]
    pub fn bip32_derivation_matches_rust_bitcoin() {
        let (frost_key, _) = encpedpop::simulate_keygen(
            &schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>(),
            3,
            5,
            5,
            &mut rand::thread_rng(),
        );

        let mut app_xpub = Xpub::from_root_key(frost_key);
        let secp = Secp256k1::verification_only();
        let xpub = bitcoin::bip32::Xpub {
            network: bitcoin::Network::Bitcoin.into(),
            depth: 0,
            parent_fingerprint: Fingerprint::default(),
            child_number: ChildNumber::from_normal_idx(0).unwrap(),
            public_key: app_xpub.key.public_key().into(),
            chain_code: ChainCode::from(app_xpub.chaincode),
        };
        let path = [1337u32, 42, 0];
        let child_path = path
            .iter()
            .map(|i| ChildNumber::Normal { index: *i })
            .collect::<Vec<_>>();
        let derived_xpub = xpub.derive_pub(&secp, &child_path).unwrap();
        app_xpub.derive_bip32(&path);

        assert_eq!(app_xpub.chaincode, *derived_xpub.chain_code.as_bytes());
        assert_eq!(app_xpub.key.public_key(), derived_xpub.public_key.into());
    }
}
