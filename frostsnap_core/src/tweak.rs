use bitcoin::{
    bip32::*,
    hashes::{sha512, Hash, HashEngine, Hmac, HmacEngine},
    secp256k1, NetworkKind,
};
use schnorr_fun::{
    frost::{PairedSecretShare, SharedKey},
    fun::{g, marker::*, Point, Scalar, G},
};

/// A BIP32 normal (non-hardened) child index.
///
/// Our own derivation hmacs any `u32` it is handed, but the descriptors the wallet builds spks
/// from can only express normal children — so a hardened index names a path we can derive and
/// never watch. Decoding enforces the range as well as construction, which keeps the guarantee
/// with the type instead of with whoever remembers to check.
///
/// rust-bitcoin has no equivalent: `ChildNumber` is an enum spanning both halves, and its
/// `From<u32>` maps the hardened half to `Hardened` rather than failing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct NormalIndex(u32);

impl NormalIndex {
    pub const ZERO: Self = NormalIndex(0);

    pub fn new(index: u32) -> Option<Self> {
        ChildNumber::from_normal_idx(index).ok()?;
        Some(NormalIndex(index))
    }

    pub fn to_u32(self) -> u32 {
        self.0
    }
}

impl From<NormalIndex> for ChildNumber {
    fn from(index: NormalIndex) -> Self {
        ChildNumber::Normal { index: index.0 }
    }
}

impl From<NormalIndex> for u32 {
    fn from(index: NormalIndex) -> Self {
        index.0
    }
}

impl TryFrom<u32> for NormalIndex {
    type Error = HardenedIndexError;

    fn try_from(index: u32) -> Result<Self, Self::Error> {
        NormalIndex::new(index).ok_or(HardenedIndexError { index })
    }
}

impl core::fmt::Display for NormalIndex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HardenedIndexError {
    pub index: u32,
}

impl core::fmt::Display for HardenedIndexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "bip32 index {} is hardened; only normal children can be derived here",
            self.index
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HardenedIndexError {}

impl bincode::Encode for NormalIndex {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.0.encode(encoder)
    }
}

impl<Context> bincode::Decode<Context> for NormalIndex {
    fn decode<D: bincode::de::Decoder>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let index = u32::decode(decoder)?;
        NormalIndex::new(index).ok_or(bincode::error::DecodeError::Other(
            "bip32 index is hardened",
        ))
    }
}

impl<'de, Context> bincode::BorrowDecode<'de, Context> for NormalIndex {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let index = u32::borrow_decode(decoder)?;
        NormalIndex::new(index).ok_or(bincode::error::DecodeError::Other(
            "bip32 index is hardened",
        ))
    }
}

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
    pub index: NormalIndex,
}

impl BitcoinBip32Path {
    pub fn external(index: NormalIndex) -> Self {
        Self {
            account_keychain: BitcoinAccountKeychain::external(),
            index,
        }
    }

    pub fn internal(index: NormalIndex) -> Self {
        Self {
            account_keychain: BitcoinAccountKeychain::internal(),
            index,
        }
    }

    /// How an output the wallet derives is named to the user: "Receive #3", "Change #2".
    /// Defined here because the device screen and the app both name it, and one output
    /// read two ways is a worse answer than either.
    pub fn label(&self) -> alloc::string::String {
        let keychain = match self.account_keychain.keychain {
            Keychain::External => "Receive",
            Keychain::Internal => "Change",
        };
        alloc::format!("{} #{}", keychain, self.index)
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, bincode::Encode, bincode::Decode, Eq, Hash, PartialOrd, Ord,
)]
pub struct BitcoinAccount {
    pub kind: AccountKind,
    pub index: NormalIndex,
}

impl BitcoinAccount {
    pub fn path_segments_from_bitcoin_appkey(&self) -> impl Iterator<Item = u32> {
        self.kind
            .path_segments_from_bitcoin_appkey()
            .chain(core::iter::once(self.index.to_u32()))
    }
}

impl Default for BitcoinAccount {
    fn default() -> Self {
        Self {
            kind: AccountKind::Segwitv1,
            index: NormalIndex::ZERO,
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
            .chain(core::iter::once(self.index.to_u32()))
    }

    pub fn from_u32_slice(path: &[u32]) -> Option<Self> {
        if path.len() != 4 {
            return None;
        }

        let account_kind = match path[0] {
            0 => AccountKind::Segwitv1,
            _ => return None,
        };

        let keychain = match path[2] {
            0 => Keychain::External,
            1 => Keychain::Internal,
            _ => return None,
        };

        // The kind and keychain segments are pinned to one value each above; these two carry the
        // whole `u32`, so they are where the range can actually be violated.
        let account = BitcoinAccount {
            kind: account_kind,
            index: NormalIndex::new(path[1])?,
        };

        Some(BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain { account, keychain },
            index: NormalIndex::new(path[3])?,
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

    const MAX_NORMAL: u32 = (1 << 31) - 1;

    /// Mirrors the encoding of [`BitcoinBip32Path`] with the indices left as bare `u32`s, so a
    /// test can produce the bytes a pre-`NormalIndex` encoder would have written.
    #[derive(bincode::Encode)]
    struct RawPath {
        account_kind: u32,
        account_index: u32,
        keychain: u32,
        index: u32,
    }

    fn encode(value: &impl bincode::Encode) -> Vec<u8> {
        bincode::encode_to_vec(value, bincode::config::standard()).unwrap()
    }

    #[test]
    fn normal_index_range() {
        assert!(NormalIndex::new(0).is_some());
        assert!(NormalIndex::new(MAX_NORMAL).is_some());
        assert!(NormalIndex::new(MAX_NORMAL + 1).is_none());
        assert!(NormalIndex::new(u32::MAX).is_none());
    }

    /// The type may not change what goes on the wire, only what is accepted off it.
    #[test]
    fn encoding_is_unchanged_by_the_newtype() {
        let path = BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain {
                account: BitcoinAccount {
                    kind: AccountKind::Segwitv1,
                    index: NormalIndex::new(7).unwrap(),
                },
                keychain: Keychain::Internal,
            },
            index: NormalIndex::new(MAX_NORMAL).unwrap(),
        };
        let raw = RawPath {
            account_kind: 0,
            account_index: 7,
            keychain: 1,
            index: MAX_NORMAL,
        };

        assert_eq!(encode(&path), encode(&raw));
    }

    /// A hardened index has to be refused wherever it can arrive, not only at the leaf type, so
    /// this drives it through each wire type that carries a path.
    #[test]
    fn hardened_index_is_refused_by_path_tweak_and_sign_item() {
        let cfg = bincode::config::standard();
        let raw = |account_index, index| RawPath {
            account_kind: 0,
            account_index,
            keychain: 0,
            index,
        };
        // `AppTweak::Bitcoin` is variant 1; a `SignItem` is an empty message then an `AppTweak`.
        let wrap = |bytes: &[u8], prefix: &[u8]| {
            let mut out = prefix.to_vec();
            out.extend_from_slice(bytes);
            out
        };

        // Both segments carrying a full `u32`, in range and just past it.
        let cases = [
            (MAX_NORMAL, MAX_NORMAL, false),
            (MAX_NORMAL + 1, 0, true),
            (0, MAX_NORMAL + 1, true),
        ];

        for (account_index, index, out_of_range) in cases {
            let path = encode(&raw(account_index, index));
            let tweak = wrap(&path, &[1]);
            let sign_item = wrap(&tweak, &[0]);

            // The in-range case decoding cleanly is what proves the rejections below are about
            // the index and not about bytes that were malformed to begin with.
            assert_eq!(
                bincode::decode_from_slice::<BitcoinBip32Path, _>(&path, cfg).is_err(),
                out_of_range,
                "BitcoinBip32Path at {account_index}/{index}"
            );
            assert_eq!(
                bincode::decode_from_slice::<AppTweak, _>(&tweak, cfg).is_err(),
                out_of_range,
                "AppTweak at {account_index}/{index}"
            );
            assert_eq!(
                bincode::decode_from_slice::<crate::SignItem, _>(&sign_item, cfg).is_err(),
                out_of_range,
                "SignItem at {account_index}/{index}"
            );
        }
    }

    /// The untrusted signing boundary is the one that matters most. Splices a hardened value into
    /// the encoding of a real task, giving the bytes an encoder predating `NormalIndex` would have
    /// produced, and covers both segments that carry a full `u32`.
    #[test]
    fn hardened_index_is_refused_inside_a_transaction_sign_task() {
        let master_appkey = crate::MasterAppkey::derive_from_rootkey(g!(2 * G).normalize());
        let cfg = bincode::config::standard();

        let needle = encode(&MAX_NORMAL);
        let hardened = encode(&(MAX_NORMAL + 1));
        assert_eq!(
            needle.len(),
            hardened.len(),
            "the splice must preserve length"
        );

        let sentinel = NormalIndex::new(MAX_NORMAL).unwrap();
        let at_address_index = BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain::external(),
            index: sentinel,
        };
        let at_account_index = BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain {
                account: BitcoinAccount {
                    kind: AccountKind::Segwitv1,
                    index: sentinel,
                },
                keychain: Keychain::External,
            },
            index: NormalIndex::ZERO,
        };

        for bip32_path in [at_address_index, at_account_index] {
            let mut tx_template = crate::bitcoin_transaction::TransactionTemplate::new();
            tx_template.push_imaginary_owned_input(
                crate::bitcoin_transaction::LocalSpk {
                    master_appkey,
                    bip32_path,
                },
                bitcoin::Amount::from_sat(100_000),
            );
            let bytes = encode(&crate::WireSignTask::BitcoinTransaction(tx_template));

            // Without this the splice below would prove nothing: a decode failure could just mean
            // the bytes were never valid.
            assert!(
                bincode::decode_from_slice::<crate::WireSignTask, _>(&bytes, cfg).is_ok(),
                "the in-range task must decode"
            );

            let occurrences = bytes.windows(needle.len()).filter(|w| *w == needle).count();
            assert_eq!(
                occurrences, 1,
                "exactly one segment should carry the sentinel"
            );
            let at = bytes
                .windows(needle.len())
                .position(|w| w == needle)
                .expect("sentinel is present");

            let mut patched = bytes.clone();
            patched[at..at + hardened.len()].copy_from_slice(&hardened);

            assert!(
                bincode::decode_from_slice::<crate::WireSignTask, _>(&patched, cfg).is_err(),
                "a hardened index must not survive decoding inside a sign task"
            );
        }
    }

    #[test]
    fn from_u32_slice_bounds_both_unpinned_segments() {
        assert!(BitcoinBip32Path::from_u32_slice(&[0, MAX_NORMAL, 1, MAX_NORMAL]).is_some());
        assert!(BitcoinBip32Path::from_u32_slice(&[0, MAX_NORMAL + 1, 0, 0]).is_none());
        assert!(BitcoinBip32Path::from_u32_slice(&[0, 0, 0, MAX_NORMAL + 1]).is_none());
        assert!(BitcoinBip32Path::from_u32_slice(&[0, 0, 1, u32::MAX]).is_none());
    }

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
