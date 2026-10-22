use bitcoin::{
    bip32::*,
    hashes::{sha512, Hash, HashEngine, Hmac, HmacEngine},
    secp256k1, NetworkKind,
};
use schnorr_fun::{
    frost::{PairedSecretShare, SharedKey},
    fun::{g, marker::*, s, Point, Scalar, G},
};

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub enum AccountKind {
    Segwitv1 = 0,
}

impl AccountKind {
    pub fn path_segments_from_bitcoin_appkey(&self) -> impl Iterator<Item = u32> {
        core::iter::once(*self as u32)
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
    Bitcoin(BitcoinBip32Path),
    Nostr,
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub struct BitcoinBip32Path {
    pub account_keychain: BitcoinAccountKeychain,
    pub index: u32,
}

impl BitcoinBip32Path {
    pub fn external(index: u32) -> Self {
        Self {
            account_keychain: BitcoinAccountKeychain::external(),
            index,
        }
    }

    pub fn internal(index: u32) -> Self {
        Self {
            account_keychain: BitcoinAccountKeychain::internal(),
            index,
        }
    }
}

impl From<BitcoinBip32Path> for DerivationPath {
    fn from(bip32_path: BitcoinBip32Path) -> Self {
        DerivationPath::from_normal_path_segments(
            bip32_path
                .account_keychain
                .account
                .path_segments_from_bitcoin_appkey()
                .chain(core::iter::once(bip32_path.index)),
        )
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub struct BitcoinAccount {
    pub kind: AccountKind,
    pub index: u32,
}

impl BitcoinAccount {
    pub fn path_segments_from_bitcoin_appkey(&self) -> impl Iterator<Item = u32> {
        self.kind
            .path_segments_from_bitcoin_appkey()
            .chain(core::iter::once(self.index))
    }

    /// The key of this account, derived from a master appkey the same way
    /// [`AppTweak::derive_xonly_key`] derives the path below it.
    pub fn derive_account_key<K: TweakableKey>(&self, master_appkey: &Xpub<K>) -> Xpub<K> {
        master_appkey
            .derive_bip32([AppTweakKind::Bitcoin as u32])
            .derive_bip32(self.path_segments_from_bitcoin_appkey())
    }
}

/// How the signing secret of a [`BitcoinBip32Path`] key is built from the secret `x` of its
/// account key: the secret is `gv * (gacc * x + tacc)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweakContext {
    pub gacc: Scalar<Public, Zero>,
    pub tacc: Scalar<Public, Zero>,
    pub gv: Scalar<Public, Zero>,
}

impl BitcoinBip32Path {
    /// The tweak context of this path relative to `account_xpub`, the key of the path's account.
    ///
    /// Non-hardened BIP32 adds its tweak to the secret without scaling it, so `gacc` is always 1.
    /// The BIP341 key-only tweak and the even-Y normalization around it are folded into `tacc`
    /// and `gv` as described on [`TweakContext`].
    pub fn tweak_context(&self, account_xpub: &Xpub<Point>) -> TweakContext {
        let mut xpub = *account_xpub;
        let mut bip32_tacc = Scalar::<Public, Zero>::zero();
        for child in [self.account_keychain.keychain as u32, self.index] {
            let (child_xpub, tweak) = xpub.bip32_child(child);
            xpub = child_xpub;
            bip32_tacc = s!(bip32_tacc + tweak).public();
        }

        // derive_xonly_key first negates the secret to make the child key even Y (gv1), then adds
        // the taproot tweak and negates again if the tweaked key is odd Y (gv2).
        let (internal_even_y, internal_negated) = xpub.key.into_point_with_even_y();
        let taproot_tweak = taproot_key_tweak(&xpub.key);
        let (_, output_negated) = g!(internal_even_y + taproot_tweak * G)
            .normalize()
            .non_zero()
            .expect("taproot tweak is a hash so this is unreachable")
            .into_point_with_even_y();

        let gv1 = sign(internal_negated);
        let gv2 = sign(output_negated);
        TweakContext {
            gacc: Scalar::<Public>::one().mark_zero(),
            tacc: s!(bip32_tacc + gv1 * taproot_tweak).public(),
            gv: s!(gv1 * gv2).public(),
        }
    }
}

fn sign(negated: bool) -> Scalar<Public, Zero> {
    let mut one = Scalar::<Public>::one().mark_zero();
    one.conditional_negate(negated);
    one
}

/// The BIP341 key-only tweak for an internal key, as used by [`AppTweak::derive_xonly_key`].
fn taproot_key_tweak(internal_key: &Point) -> Scalar<Public, Zero> {
    let tweak =
        bitcoin::taproot::TapTweakHash::from_key_and_tweak(internal_key.to_libsecp_xonly(), None)
            .to_scalar();
    Scalar::<Public, _>::from_bytes_mod_order(tweak.to_be_bytes())
}

impl Default for BitcoinAccount {
    fn default() -> Self {
        Self {
            kind: AccountKind::Segwitv1,
            index: 0,
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub struct BitcoinAccountKeychain {
    pub account: BitcoinAccount,
    pub keychain: Keychain,
}

impl BitcoinAccountKeychain {
    pub fn external() -> Self {
        Self {
            account: BitcoinAccount::default(),
            keychain: Keychain::External,
        }
    }

    pub fn internal() -> Self {
        Self {
            account: BitcoinAccount::default(),
            keychain: Keychain::Internal,
        }
    }

    pub fn path_segments_from_bitcoin_appkey(&self) -> impl Iterator<Item = u32> {
        self.account
            .path_segments_from_bitcoin_appkey()
            .chain(core::iter::once(self.keychain as u32))
    }
}

impl BitcoinBip32Path {
    pub fn path_segments_from_bitcoin_appkey(&self) -> impl Iterator<Item = u32> {
        self.account_keychain
            .path_segments_from_bitcoin_appkey()
            .chain(core::iter::once(self.index))
    }

    pub fn from_u32_slice(path: &[u32]) -> Option<Self> {
        if path.len() != 4 {
            return None;
        }

        let account_kind = match path[0] {
            0 => AccountKind::Segwitv1,
            _ => return None,
        };

        let account_index = path[1];
        let account = BitcoinAccount {
            kind: account_kind,
            index: account_index,
        };

        let keychain = match path[2] {
            0 => Keychain::External,
            1 => Keychain::Internal,
            _ => return None,
        };

        let _check_it = ChildNumber::from_normal_idx(path[2]).ok()?;
        let index = path[3];

        Some(BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain { account, keychain },
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

    pub fn derive_xonly_key<K: TweakableKey>(&self, master_appkey: &Xpub<K>) -> K::XOnly {
        let appkey = master_appkey.derive_bip32([self.kind() as u32]);

        match &self {
            AppTweak::Bitcoin(bip32_path) => {
                let concrete_internal_key =
                    appkey.derive_bip32(bip32_path.path_segments_from_bitcoin_appkey());
                let derived_key = concrete_internal_key.into_key();
                let tweak = taproot_key_tweak(&derived_key.to_key());
                derived_key
                    .into_xonly_with_tweak(tweak.non_zero().expect("computationally unreachable"))
            }
            AppTweak::Nostr => appkey.into_key().into_xonly(),
            AppTweak::TestMessage => appkey.into_key().into_xonly(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum AppTweakKind {
    Bitcoin = 0,
    TestMessage = 1,
    Nostr = 2,
}

impl AppTweakKind {
    pub fn derivation_path(&self) -> DerivationPath {
        DerivationPath::master().child(ChildNumber::Normal {
            index: *self as u32,
        })
    }
}

pub trait TweakableKey: Clone + core::fmt::Debug {
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
    pub fn from_rootkey(rootkey: T) -> Self {
        Xpub {
            chaincode: [0u8; 32],
            key: rootkey,
        }
    }

    pub fn rootkey_to_master_appkey(&self) -> Xpub<T> {
        let mut master_appkey = self.clone();
        master_appkey.derive_bip32_in_place([0]);
        master_appkey
    }

    pub fn new(key: T, chaincode: [u8; 32]) -> Self {
        Xpub { chaincode, key }
    }

    /// Does non-hardened derivation in place
    pub fn derive_bip32_in_place(&mut self, segments: impl IntoIterator<Item = u32>) {
        for child in segments.into_iter() {
            *self = self.bip32_child(child).0;
        }
    }

    /// One non-hardened BIP32 step, returning the child and the tweak that was added to the key.
    fn bip32_child(&self, child: u32) -> (Self, Scalar<Public, Zero>) {
        let mut hmac_engine: HmacEngine<sha512::Hash> = HmacEngine::new(&self.chaincode[..]);
        hmac_engine.input(&self.key().to_key().to_bytes());
        hmac_engine.input(&child.to_be_bytes());
        let hmac_result: Hmac<sha512::Hash> = Hmac::from_engine(hmac_engine);

        let tweak =
            Scalar::<Public, _>::from_slice_mod_order(&hmac_result[..32]).expect("32 bytes");
        let mut chaincode = [0u8; 32];
        chaincode.copy_from_slice(&hmac_result[32..]);
        let child_xpub = Xpub {
            key: self.key.clone().tweak(tweak),
            chaincode,
        };
        (child_xpub, tweak)
    }

    pub fn derive_bip32(&self, segments: impl IntoIterator<Item = u32>) -> Xpub<T> {
        let mut ret = self.clone();
        ret.derive_bip32_in_place(segments);
        ret
    }

    pub fn key(&self) -> &T {
        &self.key
    }

    pub fn into_key(self) -> T {
        self.key
    }

    pub fn fingerprint(&self) -> bitcoin::bip32::Fingerprint {
        self.to_bitcoin_xpub_with_lies(NetworkKind::Main)
            .fingerprint()
    }

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

pub trait DerivationPathExt {
    fn from_normal_path_segments(path_segments: impl IntoIterator<Item = u32>) -> Self;
}

impl DerivationPathExt for DerivationPath {
    fn from_normal_path_segments(path_segments: impl IntoIterator<Item = u32>) -> Self {
        DerivationPath::from_iter(path_segments.into_iter().map(|path_segment| {
            ChildNumber::from_normal_idx(path_segment).expect("valid normal derivation index")
        }))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;
    use bitcoin::secp256k1::Secp256k1;
    use schnorr_fun::frost::chilldkg::certpedpop;

    #[test]
    pub fn bip32_derivation_matches_rust_bitcoin() {
        let schnorr = schnorr_fun::new_with_deterministic_nonces::<sha2::Sha256>();
        let cert_scheme = certpedpop::vrf_cert::VrfCertScheme::<sha2::Sha256>::new("chilldkg-vrf");
        let output = certpedpop::simulate_keygen(
            &schnorr,
            &cert_scheme,
            3,
            5,
            5,
            schnorr_fun::frost::Fingerprint {
                tag: "frostsnap-v0",
                bits_per_coeff: 10,
                max_bits_total: 20,
            },
            &mut rand::thread_rng(),
        );

        let frost_key = output.certified_keygen.verified_agg_input().shared_key();
        let root_xpub = Xpub::from_rootkey(frost_key);
        let secp = Secp256k1::verification_only();
        let xpub = bitcoin::bip32::Xpub {
            network: bitcoin::Network::Bitcoin.into(),
            depth: 0,
            parent_fingerprint: Fingerprint::default(),
            child_number: ChildNumber::from_normal_idx(0).unwrap(),
            public_key: root_xpub.key.public_key().into(),
            chain_code: ChainCode::from(root_xpub.chaincode),
        };
        let path = [1337u32, 42, 0];
        let child_path = path
            .iter()
            .map(|i| ChildNumber::Normal { index: *i })
            .collect::<Vec<_>>();
        let derived_xpub = xpub.derive_pub(&secp, &child_path).unwrap();
        let our_derived_xpub = root_xpub.derive_bip32(path);

        assert_eq!(
            our_derived_xpub.chaincode,
            *derived_xpub.chain_code.as_bytes()
        );
        assert_eq!(
            our_derived_xpub.key.public_key(),
            derived_xpub.public_key.into()
        );
    }
}
