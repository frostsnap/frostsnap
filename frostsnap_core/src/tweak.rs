use bitcoin::{bip32::*, secp256k1, Network};
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

    pub fn derive_xonly_key<K: TweakableKey>(&self, root_key: &K) -> K::XOnly {
        let (app_key, extra) = root_key.app_tweak_and_expand(self.kind());
        match &self {
            AppTweak::Bitcoin(bip32_path) => {
                let mut xpub = crate::tweak::Xpub::new(app_key, extra);
                xpub.derive_bip32(&bip32_path.to_u32_array());
                let derived_key = xpub.into_key();
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
            AppTweak::Nostr => app_key.into_xonly(),
            AppTweak::TestMessage => app_key.into_xonly(),
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
#[derive(Clone)]
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
    fn bitcoin_app_xpub(&self) -> Xpub<Self> {
        let (app_key, chaincode) = self.app_tweak_and_expand(AppTweakKind::Bitcoin);
        Xpub::new(app_key, chaincode)
    }
}

impl TweakableKey for SharedKey<Normal> {
    type XOnly = SharedKey<EvenY>;

    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        self.public_key().to_libsecp_key()
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

    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        self.public_key().to_libsecp_xonly()
    }

    fn into_xonly(self) -> Self::XOnly {
        self.into_xonly()
    }
}

impl TweakableKey for PairedSecretShare {
    type XOnly = PairedSecretShare<EvenY>;

    fn to_libsecp_key(&self) -> secp256k1::PublicKey {
        self.public_key().to_libsecp_key()
    }

    fn to_libsecp_xonly(&self) -> secp256k1::XOnlyPublicKey {
        self.public_key().to_libsecp_xonly()
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
        self.into_xonly()
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
                network: Network::Bitcoin.into(),
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

    pub fn xpub(&self, network: bitcoin::Network) -> bitcoin::bip32::Xpub {
        let mut xpub = self.xpub;
        xpub.network = network.into();
        xpub
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
        let (app_key, chaincode) = frost_key.app_tweak_and_expand(AppTweakKind::Bitcoin);

        let mut app_xpub = Xpub::new(app_key, chaincode);
        let secp = Secp256k1::verification_only();
        let xpub = app_xpub.xpub(bitcoin::Network::Bitcoin);
        let path = [1337u32, 42, 0];
        let child_path = path
            .iter()
            .map(|i| ChildNumber::Normal { index: *i })
            .collect::<Vec<_>>();
        let derived_xpub = xpub.derive_pub(&secp, &child_path).unwrap();
        app_xpub.derive_bip32(&path);

        assert_eq!(app_xpub.xpub(bitcoin::Network::Bitcoin), derived_xpub);
    }
}
