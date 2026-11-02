//! The seal and the opening proofs (spec v2 §7, as spec v3 PK.5 adapts them to C′).
//!
//! The device commits to each of the card's 311 committed bits (the 55 bits of the hidden
//! random values, then the 256 scalar bits) with a Pedersen commitment C_v = x_v·G + r_v·H. The
//! blindings derive from the share, so the seal is the same in every session, and r_300 is
//! solved so that Σ_t 2^t·C_t = S_i holds exactly: the phone checks that the committed scalar
//! is the share. The seal hash h_C also covers hP, the hash of the phone half P (words 33-64,
//! which the device sends in the clear), so every later P must hash to the hP inside the pinned
//! h_C (binding (1), PK.6). Every C_v gets a bit proof.
//!
//! All 32 device words are computed, so every word the phone round opens takes the same
//! opening: the device reveals its word y and commits to the 7-bit carry of each output bit's
//! parity sum over the committed bits, so the phone can check Σ_{v∈T_jβ} C_v − y′_β·G =
//! Σ_l 2^{l+1}·Q_{β,l}, where y′ is y with the public part (the index and P, which the phone
//! folds itself) taken out, and proves all 77 carry bits under open_dom, which also covers hP.
//! The phone half has no openings.
//!
//! The device computes the bit proofs only after the device half is written, quizzed and
//! confirmed on the phone (`DeviceHalfWritten`, PK.7), and the openings only in the phone
//! round. Those gates are in frostsnap_core's card state machine, not here.
//!
//! The bit proofs are stock sigma_fun `Or<DL, DL>` proofs, so the phone verifies them with
//! sigma_fun itself. The device proves with [`CardBitProver`], which does the same steps in the
//! same order for both bits and gives the stock proof byte for byte. [`Ops`] counts the group
//! operations and SHA-256 blocks each call costs, so device timings can be checked against unit
//! costs.

use super::{
    codec::{self, wipe, EncodeError},
    jobs::{OpenStart, SealJob},
    layout::{
        self, committed_of_scalar_bit, Bits, BoxSets, Meta, MetaError, CARRIES_PER_BOX, CARRY_BITS,
        COMMITTED, FMT_C, FREE, HALF, V_LINK, WORD_BITS,
    },
};
use alloc::{boxed::Box, vec::Vec};
use rand_chacha::ChaCha20Rng;
use rand_core::{CryptoRng, RngCore};
use schnorr_fun::{
    frost::{SecretShare, ShareImage, SharedKey},
    fun::{
        digest::Update,
        prelude::*,
        subtle::{Choice, ConditionallySelectable, ConstantTimeEq},
        Tag,
    },
};
use sha2::{Digest, Sha256};
use sigma_fun::{
    generic_array::GenericArray, secp256k1::DL, typenum::U16, CompactProof, Either, FiatShamir,
    HashTranscript, Or, ProverTranscript, Sigma, Transcript, Writable,
};

// DRAFT: frozen before the first release (spec PK-O3).
/// H is `hash_to_curve_sswu` (RFC 9380) of this message under [`H_DST`], so nobody knows log_G H.
pub const H_MESSAGE: &[u8] = b"frostsnap card generator H";
pub const H_DST: &[u8] = b"frostsnap/card/commit-v1/H";
// The commit-v1 tags: they fix the seal, so they never change once a card ships.
pub const CTX_TAG: &[u8] = b"frostsnap/card/commit-v1/ctx";
pub const BLIND_KEY_TAG: &[u8] = b"frostsnap/card/commit-v1/blind-key";
pub const BLIND_TAG: &[u8] = b"frostsnap/card/commit-v1/blind";
pub const COIN_TAG: &[u8] = b"frostsnap/card/commit-v1/box-coin";
pub const COIN_COMMIT_TAG: &[u8] = b"frostsnap/card/commit-v1/box-coin-commit";
// The C′ (packed-c1) seal names: h_C covers hP, so it's a new hash.
pub const SEAL_TAG: &[u8] = b"frostsnap/card/c1/seal";
pub const PHONE_HALF_TAG: &[u8] = b"frostsnap/card/c1/phone-half";
// The proof-v3 names: any change to the statement, encoding or draw order bumps them. v3 is
// C′: 311 commitments, open_dom covers hP, and every D word takes a carry opening.
pub const BIT_NAME: &str = "frostsnap/card/proof-v3/bit";
pub const OPEN_TAG: &[u8] = b"frostsnap/card/proof-v3/open";
pub const N_TAG_TAG: &[u8] = b"frostsnap/card/proof-v3/n-tag";
pub const CARRY_BLIND_TAG: &[u8] = b"frostsnap/card/proof-v3/carry-blind";
pub const BOXES_TAG: &[u8] = b"frostsnap/card/proof-v3/boxes";

/// The wire versions: format_id 3 is C′, the packed layout, and proof_version 3 is its proofs
/// (311 commitments, 7 carry bits, hP in h_C and open_dom).
pub use super::layout::FORMAT_ID;
pub const PROOF_VERSION: u8 = 3;
/// Boxes the phone round opens: the ordered set D.
pub const CHECK_BOXES: usize = 5;
/// The most C's in one frame.
pub const POINTS_PER_FRAME: usize = 32;
/// The most commit proofs in one frame.
pub const PROOFS_PER_FRAME: usize = 16;
/// An opening's proofs go in two frames: the first 39, then the other 38.
pub const OPENING_SPLIT: usize = 39;
const _: () = assert!(OPENING_SPLIT == CARRIES_PER_BOX.div_ceil(2));
/// The most runs (openings computed) per box.
pub const MAX_RUNS: u8 = 4;

/// The role of a commit proof, for a C_v.
pub const ROLE_COMMIT: u8 = 0x00;
/// The role of a carry proof, for a Q_{β,l}.
pub const ROLE_CARRY: u8 = 0x01;

/// A compressed point on the wire.
pub const POINT_LEN: usize = 33;
/// A bit proof on the wire: e (16) ‖ z_l (32) ‖ e_l (16) ‖ z_r (32).
pub const PROOF_LEN: usize = 96;
/// Every C_v and its proof: 40,119 B.
pub const COMMITMENT_BYTES: usize = COMMITTED * (POINT_LEN + PROOF_LEN);
/// One opening: j, y, then every Q and its proof: 9,936 B.
pub const OPENING_BYTES: usize = 1 + 2 + CARRIES_PER_BOX * (POINT_LEN + PROOF_LEN);
const _: () = assert!(COMMITMENT_BYTES == 40_119 && OPENING_BYTES == 9_936);

/// Work done, counted where this module does it, so device timings can be compared with unit
/// costs. The codec's work (encoding the card) isn't counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ops {
    /// k·P, normalisation included.
    pub mul: u32,
    /// a·P + b·Q. Stock verification does two per bit proof.
    pub lincomb: u32,
    /// Point additions and doublings.
    pub add: u32,
    /// Normalisations of a sum, a field inversion each.
    pub normalize: u32,
    /// Points read from 33 bytes, a square root each.
    pub decompress: u32,
    /// SHA-256 compressions.
    pub sha_blocks: u32,
}

impl core::ops::Sub for Ops {
    type Output = Ops;

    fn sub(self, before: Ops) -> Ops {
        Ops {
            mul: self.mul - before.mul,
            lincomb: self.lincomb - before.lincomb,
            add: self.add - before.add,
            normalize: self.normalize - before.normalize,
            decompress: self.decompress - before.decompress,
            sha_blocks: self.sha_blocks - before.sha_blocks,
        }
    }
}

/// What each counted operation costs, in microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitCosts {
    pub mul: u32,
    pub lincomb: u32,
    pub add: u32,
    pub normalize: u32,
    pub decompress: u32,
    pub sha_block: u32,
}

/// The spec's ESP32-C3 unit costs (§7.5.1). It doesn't cost additions, normalisations or
/// parsing points, so a bench should measure those.
pub const SPEC_COSTS: UnitCosts = UnitCosts {
    mul: 33_200,
    lincomb: 47_500,
    add: 0,
    normalize: 0,
    decompress: 0,
    sha_block: 58,
};

impl Ops {
    /// What these operations take at these unit costs, in microseconds.
    pub fn cost_us(&self, costs: &UnitCosts) -> u64 {
        [
            (self.mul, costs.mul),
            (self.lincomb, costs.lincomb),
            (self.add, costs.add),
            (self.normalize, costs.normalize),
            (self.decompress, costs.decompress),
            (self.sha_blocks, costs.sha_block),
        ]
        .iter()
        .map(|&(count, unit)| count as u64 * unit as u64)
        .sum()
    }
}

/// SHA-256 compressions for a message of `len` bytes.
const fn sha_blocks(len: usize) -> u32 {
    (len + 9).div_ceil(64) as u32
}

/// TH(tag, m) = SHA256(SHA256(tag) ‖ SHA256(tag) ‖ m), counting the blocks it costs.
pub(crate) struct Tagged {
    hash: Sha256,
    len: usize,
}

impl Tagged {
    pub(crate) fn new(tag: &[u8]) -> Self {
        Self {
            hash: Sha256::default().tag(tag),
            len: 0,
        }
    }

    pub(crate) fn add(mut self, bytes: &[u8]) -> Self {
        Digest::update(&mut self.hash, bytes);
        self.len += bytes.len();
        self
    }

    pub(crate) fn finish(self, ops: &mut Ops) -> [u8; 32] {
        // One block for the tag (every tag here is under 56 bytes), then the prefix and m.
        ops.sha_blocks += 1 + sha_blocks(64 + self.len);
        self.hash.finalize().into()
    }
}

/// h2s: a hash as a scalar, reduced mod n.
fn h2s(hash: [u8; 32]) -> Scalar<Secret, Zero> {
    Scalar::from_bytes_mod_order(hash)
}

fn bit_choice(bit: bool) -> Choice {
    Choice::from(bit as u8)
}

/// `a` when `choice` is 0 and `b` when it's 1, as a + choice·(b − a), without branching.
fn select_scalar(
    a: &Scalar<Public, Zero>,
    b: &Scalar<Public, Zero>,
    choice: Choice,
) -> Scalar<Public, Zero> {
    let c = Scalar::<Secret, Zero>::from(choice.unwrap_u8());
    s!(a + c * (b - a)).public()
}

/// A point from its 33 bytes, refusing the identity and G (pitfall 13).
pub fn point_from_bytes(bytes: &[u8; POINT_LEN], ops: &mut Ops) -> Option<Point> {
    ops.decompress += 1;
    Point::from_bytes(*bytes).filter(|point| point != G)
}

/// What a bit proof is about, hashed ahead of its statement (strong Fiat–Shamir).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitCtx {
    /// [`ROLE_COMMIT`] or [`ROLE_CARRY`].
    pub role: u8,
    /// h_C for a commit proof, open_dom for a carry proof.
    pub dom: [u8; 32],
    /// v for a commit proof, 7β + l for a carry proof.
    pub index: u16,
}

impl BitCtx {
    pub fn commit(h_c: [u8; 32], v: usize) -> Self {
        Self {
            role: ROLE_COMMIT,
            dom: h_c,
            index: v as u16,
        }
    }

    /// Carry `k` = 7β + l.
    pub fn carry(open_dom: [u8; 32], k: usize) -> Self {
        Self {
            role: ROLE_CARRY,
            dom: open_dom,
            index: k as u16,
        }
    }
}

/// A sigma protocol with a [`BitCtx`] hashed ahead of its statement. Everything else is the
/// inner protocol's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WithCtx<S>(pub S);

impl<S: Sigma> Sigma for WithCtx<S> {
    type Witness = S::Witness;
    type Statement = (BitCtx, S::Statement);
    type AnnounceSecret = S::AnnounceSecret;
    type Announcement = S::Announcement;
    type Response = S::Response;
    type ChallengeLength = S::ChallengeLength;

    fn announce(
        &self,
        statement: &Self::Statement,
        announce_secret: &Self::AnnounceSecret,
    ) -> Self::Announcement {
        self.0.announce(&statement.1, announce_secret)
    }

    fn gen_announce_secret<Rng: CryptoRng + RngCore>(
        &self,
        witness: &Self::Witness,
        rng: &mut Rng,
    ) -> Self::AnnounceSecret {
        self.0.gen_announce_secret(witness, rng)
    }

    fn sample_response<Rng: CryptoRng + RngCore>(&self, rng: &mut Rng) -> Self::Response {
        self.0.sample_response(rng)
    }

    fn respond(
        &self,
        witness: &Self::Witness,
        statement: &Self::Statement,
        announce_secret: Self::AnnounceSecret,
        announce: &Self::Announcement,
        challenge: &GenericArray<u8, Self::ChallengeLength>,
    ) -> Self::Response {
        self.0
            .respond(witness, &statement.1, announce_secret, announce, challenge)
    }

    fn implied_announcement(
        &self,
        statement: &Self::Statement,
        challenge: &GenericArray<u8, Self::ChallengeLength>,
        response: &Self::Response,
    ) -> Option<Self::Announcement> {
        self.0
            .implied_announcement(&statement.1, challenge, response)
    }

    fn hash_statement<H: Update>(&self, hash: &mut H, statement: &Self::Statement) {
        let (ctx, inner) = statement;
        hash.update(&[ctx.role]);
        hash.update(&ctx.dom);
        hash.update(&ctx.index.to_be_bytes());
        self.0.hash_statement(hash, inner)
    }

    fn hash_announcement<H: Update>(&self, hash: &mut H, announcement: &Self::Announcement) {
        self.0.hash_announcement(hash, announcement)
    }

    fn hash_witness<H: Update>(&self, hash: &mut H, witness: &Self::Witness) {
        self.0.hash_witness(hash, witness)
    }
}

impl<S: Writable> Writable for WithCtx<S> {
    fn write_to<W: core::fmt::Write>(&self, w: &mut W) -> core::fmt::Result {
        self.0.write_to(w)
    }
}

/// One committed bit: X = r·H (bit 0, `Either::Left(r)`) or X − G = r·H (bit 1, `Right(r)`).
pub type CardBit = WithCtx<Or<DL<U16>, DL<U16>>>;
pub type CardTranscript = HashTranscript<Sha256, ChaCha20Rng>;
pub type CardFs = FiatShamir<CardBit, CardTranscript>;
/// (ctx, ((H, X), (H, X − G))).
pub type Statement = <CardBit as Sigma>::Statement;
pub type BitProof = CompactProof<<CardBit as Sigma>::Response, U16>;

// The transcript's 64-byte prefix is hashed once, in `Setup::new`. After it the 167 statement
// bytes fill 2 blocks with 39 left over, and the nonce seed (64 more bytes) and the challenge
// (66 more) take 2 blocks each.
const STATEMENT_BLOCKS: u32 = 2;
const SEED_BLOCKS: u32 = 2;
const CHALLENGE_BLOCKS: u32 = 2;

/// The wire format: e (16) ‖ z_l (32) ‖ e_l (16) ‖ z_r (32), where ((z_l, e_l), z_r) is the
/// `Or` response.
pub fn encode_proof(proof: &BitProof) -> [u8; PROOF_LEN] {
    let ((z_l, e_l), z_r) = &proof.response;
    let mut bytes = [0u8; PROOF_LEN];
    bytes[..16].copy_from_slice(&proof.challenge);
    bytes[16..48].copy_from_slice(&z_l.to_bytes());
    bytes[48..64].copy_from_slice(e_l);
    bytes[64..].copy_from_slice(&z_r.to_bytes());
    bytes
}

/// Reads a proof, refusing a response of n or more.
pub fn decode_proof(bytes: &[u8; PROOF_LEN]) -> Option<BitProof> {
    let scalar = |at: usize| Scalar::<Public, Zero>::from_slice(&bytes[at..at + 32]);
    Some(CompactProof {
        challenge: GenericArray::clone_from_slice(&bytes[..16]),
        response: (
            (scalar(16)?, GenericArray::clone_from_slice(&bytes[48..64])),
            scalar(64)?,
        ),
    })
}

/// A 16-byte challenge as a scalar, read as sigma_fun's DL reads it.
fn challenge_scalar(challenge: &[u8; 16]) -> Scalar<Public, Zero> {
    let mut bytes = [0u8; 32];
    bytes[16..].copy_from_slice(challenge);
    Scalar::from_bytes_mod_order(bytes)
}

/// Why the device can't seal or open its card. Each one stops the card job ("Device error").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardError {
    Meta(MetaError),
    Encode(EncodeError),
    /// A derived value is zero (probability about 2^-256).
    ZeroValue,
    /// Two computations of a secret value differ, or a check before release failed: a fault.
    SelfCheck,
    /// There's no opening of word `j + 1`: only the device half's words 1-32 have one.
    NoSuchBox,
    /// There's no committed bit `v`.
    NoSuchBit,
}

impl From<MetaError> for CardError {
    fn from(error: MetaError) -> Self {
        CardError::Meta(error)
    }
}

impl From<EncodeError> for CardError {
    fn from(error: EncodeError) -> Self {
        match error {
            // The two encodes disagreeing is a fault, like any other self-check.
            EncodeError::SelfCheck => CardError::SelfCheck,
            error => CardError::Encode(error),
        }
    }
}

/// Why the phone rejects a seal or an opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyError {
    /// The wrong number of points or proofs.
    Shape,
    /// A point isn't on the curve, or is the identity or G.
    BadPoint,
    /// Σ_t 2^t·C_t isn't S_i: the committed scalar isn't this share.
    Link,
    /// h_C over the phone's own ctx and the hash of the phone half the device sent isn't the
    /// seal's.
    SealHash,
    /// A phone half that doesn't hash to the hP inside the pinned h_C: the device served
    /// another P than the one the seal was pinned with (binding (1), PK.6).
    PhoneHalf,
    /// The bit proof of C_v failed.
    CommitProof(usize),
    /// Word `j + 1` has no opening: only the device half's words 1-32 have one.
    NoSuchBox,
    /// The opening is for another box.
    WrongBox,
    /// A word, of an opening or of the phone half, is 2048 or more.
    WordRange,
    /// Output bit β's parity equation fails.
    Parity(usize),
    /// The bit proof of carry 7β + l failed.
    CarryProof(usize),
    /// The revealed coin isn't the one the seal committed to.
    CoinCommit,
    /// The word isn't in D.
    NotInD,
    /// A frame is out of range, the wrong size, out of order or overlaps one already taken.
    Frame,
}

/// The public record a card's seal is bound to. The device builds it from its own share (in
/// [`CardSecrets::new`]) and the phone from its own wallet record; neither takes it from the
/// other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// From the wallet's `AccessStructureRef`.
    pub key_id: [u8; 32],
    pub access_structure_id: [u8; 32],
    /// i_sh and S_i.
    pub image: ShareImage,
    /// The threshold and index, as the card writes them.
    pub meta: Meta,
}

impl Context {
    pub fn new(
        key_id: [u8; 32],
        access_structure_id: [u8; 32],
        image: ShareImage,
        threshold: usize,
    ) -> Result<Self, CardError> {
        if image.image.is_zero() {
            return Err(CardError::ZeroValue);
        }
        Ok(Self {
            key_id,
            access_structure_id,
            image,
            meta: Meta::new(image.index, threshold)?,
        })
    }

    /// ctx = TH(ctx, u8 len(FMT_C) ‖ FMT_C ‖ key_id ‖ access_structure_id ‖ i_sh as a scalar ‖
    /// ser(S_i) ‖ u8 t ‖ u8 i).
    pub fn hash(&self, ops: &mut Ops) -> [u8; 32] {
        Tagged::new(CTX_TAG)
            .add(&[FMT_C.len() as u8])
            .add(FMT_C)
            .add(&self.key_id)
            .add(&self.access_structure_id)
            .add(&self.image.index.to_bytes())
            .add(&self.image.image.to_bytes())
            .add(&[self.meta.threshold, self.meta.index])
            .finish(ops)
    }
}

/// hP = TH("frostsnap/card/c1/phone-half", L ‖ BE16(y_33) ‖ … ‖ BE16(y_64)), the hash of the
/// phone half that h_C and every open_dom cover. It hashes the words as given; callers that
/// take P from the device refuse a word of 2048 or more first ([`check_phone_words`]).
pub fn phone_half_hash(phone_half: &[u16; HALF], ops: &mut Ops) -> [u8; 32] {
    phone_half
        .iter()
        .fold(
            Tagged::new(PHONE_HALF_TAG).add(&layout::layout_hash()),
            |hash, y| hash.add(&y.to_be_bytes()),
        )
        .finish(ops)
}

/// Refuses a phone half with a word of 2048 or more: P values are 11 bits.
pub fn check_phone_words(phone_half: &[u16; HALF]) -> Result<(), VerifyError> {
    if phone_half.iter().any(|&y| y >= 1 << WORD_BITS) {
        return Err(VerifyError::WordRange);
    }
    Ok(())
}

/// r_v = h2s(TH(blind, K_card ‖ BE16(v))), for every v but [`V_LINK`].
fn derived_blinding(key: &[u8; 32], v: usize, ops: &mut Ops) -> Result<Scalar, CardError> {
    h2s(Tagged::new(BLIND_TAG)
        .add(key)
        .add(&(v as u16).to_be_bytes())
        .finish(ops))
    .non_zero()
    .ok_or(CardError::ZeroValue)
}

/// What the device derives from its share for the seal and its openings. It's secret, so it
/// has no `Debug` and is wiped when dropped.
pub struct CardSecrets {
    /// x_v.
    bits: Bits,
    /// The card's index, its public bits in h0.
    index: u8,
    /// The phone half P, the public values the phone folds into every word.
    phone_half: [u16; HALF],
    /// hP, the hash of P that h_C and every open_dom cover.
    phone_hash: [u8; 32],
    pub(super) ctx: [u8; 32],
    pub(super) image: Point<Normal, Public, Zero>,
    /// K_card: every blinding derives from it.
    key: [u8; 32],
    /// r_300, solved so that Σ_t 2^t·r_t = 0 and the link holds exactly.
    link: Scalar,
    pub(super) coin: [u8; 32],
}

impl CardSecrets {
    /// Encodes the share's card twice and checks the two agree (the masks and the threshold
    /// word can't be checked any other way), reads its bits out and checks they rebuild the
    /// card's 64 words (the link doesn't cover the free bits or P), then derives hP, ctx,
    /// K_card, r_300 and the box coin.
    ///
    /// ctx takes S_i and M from the share itself. `key_id` and `access_structure_id` must come
    /// from the device's own record of the share, its `AccessStructureRef`: a request only
    /// selects the share, and its values are never used (spec §7.2.4).
    pub fn new(
        share: &SecretShare,
        shared_key: &SharedKey<Normal, impl ZeroChoice>,
        key_id: [u8; 32],
        access_structure_id: [u8; 32],
        ops: &mut Ops,
    ) -> Result<Self, CardError> {
        Self::new_with_card(share, shared_key, key_id, access_structure_id, ops)
            .map(|(secrets, _)| secrets)
    }

    /// [`new`](Self::new), also giving the card it encoded, so a device that shows the card
    /// doesn't encode it again.
    pub fn new_with_card(
        share: &SecretShare,
        shared_key: &SharedKey<Normal, impl ZeroChoice>,
        key_id: [u8; 32],
        access_structure_id: [u8; 32],
        ops: &mut Ops,
    ) -> Result<(Self, codec::Card), CardError> {
        let card = codec::encode_checked(share, shared_key)?;
        let secrets = Self::from_card(share, shared_key, &card, key_id, access_structure_id, ops)?;
        Ok((secrets, card))
    }

    /// [`new`](Self::new) for a card the caller has encoded already, twice and compared (as
    /// the device's sliced encode does), so the share isn't encoded again here. It still checks
    /// the bits, the index and P rebuild the card's words, comparing in constant time, and
    /// computes hP twice and compares.
    pub fn from_card(
        share: &SecretShare,
        shared_key: &SharedKey<Normal, impl ZeroChoice>,
        card: &codec::Card,
        key_id: [u8; 32],
        access_structure_id: [u8; 32],
        ops: &mut Ops,
    ) -> Result<Self, CardError> {
        let bits = layout::committed_bits(&card.values);
        let index = layout::index_of(&card.values);
        let phone_half = card.phone_half();
        let mut words = layout::words_from_bits(&bits, index, &phone_half);
        let rebuilt: bool = words[..].ct_eq(&card.words[..]).into();
        wipe(&mut words, [0; layout::WORDS]);
        if !rebuilt {
            return Err(CardError::SelfCheck);
        }
        let context = Context::new(
            key_id,
            access_structure_id,
            share.share_image(),
            shared_key.threshold(),
        )?;

        if index != context.meta.index {
            return Err(CardError::SelfCheck);
        }
        // hP goes into h_C, which the phone checks every later P against, so a fault here
        // would fail an honest card: computed twice and compared.
        let phone_hash = phone_half_hash(&phone_half, ops);
        if phone_half_hash(&phone_half, ops) != phone_hash {
            return Err(CardError::SelfCheck);
        }
        let ctx = context.hash(ops);
        let key = Tagged::new(BLIND_KEY_TAG)
            .add(&share.share.to_bytes())
            .add(&ctx)
            .finish(ops);
        // Σ_{t≥1} 2^{t−1}·r_t by Horner, from scalar bit 255 down; r_300 is −2 times it.
        let mut sum = Scalar::<Secret, Zero>::zero();
        for t in (1..256).rev() {
            let r = derived_blinding(&key, committed_of_scalar_bit(t), ops)?;
            sum = s!(sum + sum + r);
        }
        let double = s!(sum + sum);
        let link = s!(-double).non_zero().ok_or(CardError::ZeroValue)?;
        let coin = Tagged::new(COIN_TAG).add(&key).finish(ops);

        Ok(Self {
            bits,
            index,
            phone_half,
            phone_hash,
            ctx,
            image: context.image.image,
            key,
            link,
            coin,
        })
    }

    /// S_i, the image of the share these were derived from.
    pub fn image(&self) -> Point<Normal, Public, Zero> {
        self.image
    }

    /// x_v.
    pub fn bit(&self, v: usize) -> Choice {
        bit_choice(self.bits.get(v))
    }

    /// r_v. All but r_300 are derived on demand, so the device holds none of them.
    pub fn blinding(&self, v: usize, ops: &mut Ops) -> Result<Scalar, CardError> {
        if v == V_LINK {
            Ok(self.link)
        } else {
            derived_blinding(&self.key, v, ops)
        }
    }

    /// The box coin, which the device reveals only after the device round passed.
    pub fn coin(&self) -> [u8; 32] {
        self.coin
    }

    /// The card's index.
    pub fn index(&self) -> u8 {
        self.index
    }

    /// The card's phone half P.
    pub fn phone_half(&self) -> &[u16; HALF] {
        &self.phone_half
    }

    /// hP, the hash of the phone half that the seal and every opening cover.
    pub fn phone_hash(&self) -> &[u8; 32] {
        &self.phone_hash
    }
}

impl Drop for CardSecrets {
    fn drop(&mut self) {
        wipe(&mut self.bits, Bits::default());
        wipe(&mut self.index, 0);
        wipe(&mut self.phone_half, [0; HALF]);
        wipe(&mut self.phone_hash, [0; 32]);
        wipe(&mut self.key, [0; 32]);
        wipe(&mut self.link, Scalar::one());
        wipe(&mut self.coin, [0; 32]);
    }
}

/// The device's commitment to its card, which the phone pins before the first word is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seal {
    /// ser(C_v) for v = 0..311.
    pub commitments: Vec<[u8; POINT_LEN]>,
    /// TH(box-coin-commit, coin).
    pub coin_commit: [u8; 32],
    /// h_C = TH(seal, ctx ‖ coin_commit ‖ hP ‖ ser(C_0) ‖ … ‖ ser(C_310)).
    pub h_c: [u8; 32],
}

/// A pinned seal whose 311 commit proofs have all verified. Openings are checked only against
/// one, so the type system keeps a merely pinned seal out of the phone round.
#[derive(Clone, Copy, Debug)]
pub struct Proven<'a> {
    seal: &'a Seal,
    ctx: [u8; 32],
    index: u8,
}

impl<'a> Proven<'a> {
    /// For a seal whose 311 commit proofs have all verified under a context with this ctx and
    /// index.
    pub(super) fn new(seal: &'a Seal, ctx: [u8; 32], index: u8) -> Self {
        Self { seal, ctx, index }
    }

    pub fn seal(&self) -> &'a Seal {
        self.seal
    }

    /// Whether this phone half is the one the seal was pinned with: P has no word of 2048 or
    /// more, and h_C over the pinned ctx and hP(P) is the seal's. Gives hP.
    pub fn check_phone_half(
        &self,
        phone_half: &[u16; HALF],
        ops: &mut Ops,
    ) -> Result<[u8; 32], VerifyError> {
        phone_half_matches(self.seal, &self.ctx, phone_half, ops)
    }
}

/// hP(P), if h_C over `ctx`, the seal's coin_commit and C's and hP(P) is the seal's, else
/// [`VerifyError::PhoneHalf`]. A word of 2048 or more is [`VerifyError::WordRange`].
pub(super) fn phone_half_matches(
    seal: &Seal,
    ctx: &[u8; 32],
    phone_half: &[u16; HALF],
    ops: &mut Ops,
) -> Result<[u8; 32], VerifyError> {
    check_phone_words(phone_half)?;
    let phone_hash = phone_half_hash(phone_half, ops);
    if seal_hash(ctx, &seal.coin_commit, &phone_hash, &seal.commitments, ops) != seal.h_c {
        return Err(VerifyError::PhoneHalf);
    }
    Ok(phone_hash)
}

/// coin_commit = TH(box-coin-commit, coin).
pub fn coin_commitment(coin: &[u8; 32], ops: &mut Ops) -> [u8; 32] {
    Tagged::new(COIN_COMMIT_TAG).add(coin).finish(ops)
}

/// Whether a revealed coin is the one the seal committed to (B5), compared in constant time.
pub fn coin_matches(coin_commit: &[u8; 32], coin: &[u8; 32], ops: &mut Ops) -> bool {
    coin_commitment(coin, ops).ct_eq(coin_commit).into()
}

/// D, the words the phone round opens (B5), all in words 1-32: for k = 0, 1, …, the bytes b of
/// TH(boxes, h_C ‖ N ‖ coin ‖ u8 k) in order, each as word j = b & 31, skipping repeats. The
/// first 5 distinct, in order. Every input is public by the time D is drawn.
pub fn derive_boxes(
    h_c: &[u8; 32],
    n: &[u8; 32],
    coin: &[u8; 32],
    ops: &mut Ops,
) -> [u8; CHECK_BOXES] {
    let mut boxes = [0u8; CHECK_BOXES];
    let mut found = 0;
    for k in 0..=u8::MAX {
        let block = Tagged::new(BOXES_TAG)
            .add(h_c)
            .add(n)
            .add(coin)
            .add(&[k])
            .finish(ops);
        for b in block {
            let j = b & (HALF as u8 - 1);
            if !boxes[..found].contains(&j) {
                boxes[found] = j;
                found += 1;
                if found == CHECK_BOXES {
                    return boxes;
                }
            }
        }
    }
    // Fewer than 5 distinct values in 8,192 hash bytes is a hash failure.
    unreachable!("256 blocks always hold 5 distinct boxes")
}

/// h_C = TH(seal, ctx ‖ coin_commit ‖ hP ‖ ser(C_0) ‖ … ‖ ser(C_310)).
pub fn seal_hash(
    ctx: &[u8; 32],
    coin_commit: &[u8; 32],
    phone_hash: &[u8; 32],
    commitments: &[[u8; POINT_LEN]],
    ops: &mut Ops,
) -> [u8; 32] {
    commitments
        .iter()
        .fold(
            Tagged::new(SEAL_TAG)
                .add(ctx)
                .add(coin_commit)
                .add(phone_hash),
            |hash, c| hash.add(c),
        )
        .finish(ops)
}

/// Σ_t 2^t·C_t over the scalar bits, by Horner from bit 255 down, or `None` if a point doesn't
/// parse. The device's self-check and the phone's check both use it.
fn link(commitments: &[[u8; POINT_LEN]], ops: &mut Ops) -> Option<Point<NonNormal, Public, Zero>> {
    link_steps(Point::zero(), commitments, (0..256).rev(), ops)
}

/// The link's Horner steps for scalar bits `ts`, taken from the top down, starting from
/// `sum`, so the device can compute it in chunks.
pub(super) fn link_steps(
    mut sum: Point<NonNormal, Public, Zero>,
    commitments: &[[u8; POINT_LEN]],
    ts: impl Iterator<Item = usize>,
    ops: &mut Ops,
) -> Option<Point<NonNormal, Public, Zero>> {
    for t in ts {
        let c = point_from_bytes(&commitments[committed_of_scalar_bit(t)], ops)?;
        sum = g!(sum + sum + c);
        ops.add += 2;
    }
    Some(sum)
}

/// n_tag = TH(n-tag, N), which the device persists before it reveals the coin.
pub fn n_tag(n: &[u8; 32], ops: &mut Ops) -> [u8; 32] {
    Tagged::new(N_TAG_TAG).add(n).finish(ops)
}

/// open_dom = TH(open, h_C ‖ hP ‖ N ‖ u8 j ‖ BE16(y) ‖ ser(Q_{0,0}) ‖ … ‖ ser(Q_{10,6})).
pub fn open_domain(
    h_c: &[u8; 32],
    phone_hash: &[u8; 32],
    n: &[u8; 32],
    j: u8,
    y: u16,
    carries: &[[u8; POINT_LEN]],
    ops: &mut Ops,
) -> [u8; 32] {
    carries
        .iter()
        .fold(
            Tagged::new(OPEN_TAG)
                .add(h_c)
                .add(phone_hash)
                .add(n)
                .add(&[j])
                .add(&y.to_be_bytes()),
            |hash, q| hash.add(q),
        )
        .finish(ops)
}

/// H and the bit-proof transcript, computed once and shared by every proof and check.
#[derive(Clone, Debug)]
pub struct Setup {
    /// The NUMS generator H.
    pub h: Point,
    fs: CardFs,
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}

impl Setup {
    pub fn new() -> Self {
        Self {
            h: Point::hash_to_curve_sswu::<Sha256>(H_MESSAGE, H_DST).normalize(),
            fs: CardFs::new(
                CardBit::default(),
                CardTranscript::default(),
                Some(BIT_NAME),
            ),
        }
    }

    /// The stock prover and verifier. The phone verifies with it; the device proves with
    /// [`CardBitProver`].
    pub fn fs(&self) -> &CardFs {
        &self.fs
    }

    /// The statement for a committed point X, or `None` when X = G.
    pub fn statement(&self, x: Point, ctx: BitCtx, ops: &mut Ops) -> Option<Statement> {
        ops.add += 1;
        ops.normalize += 1;
        let x_minus_g = g!(x - G).normalize().non_zero()?;
        Some((ctx, ((self.h, x), (self.h, x_minus_g))))
    }

    fn verify_statement(&self, statement: &Statement, proof: &BitProof, ops: &mut Ops) -> bool {
        ops.lincomb += 2;
        ops.sha_blocks += STATEMENT_BLOCKS + CHALLENGE_BLOCKS;
        self.fs.verify(statement, proof)
    }

    /// Stock verification of a bit proof for X under `ctx`. It refuses X = G; a parsed point is
    /// never the identity.
    #[must_use]
    pub fn verify_bit(
        &self,
        x: &Point,
        ctx: BitCtx,
        proof: &[u8; PROOF_LEN],
        ops: &mut Ops,
    ) -> bool {
        match (self.statement(*x, ctx, ops), decode_proof(proof)) {
            (Some(statement), Some(proof)) => self.verify_statement(&statement, &proof, ops),
            _ => false,
        }
    }

    /// A bit proof from the fixed-order prover, verified before it's returned.
    pub fn prove_bit<R: RngCore + CryptoRng>(
        &self,
        x: Point,
        ctx: BitCtx,
        bit: Choice,
        witness: Scalar,
        rng: Option<&mut R>,
        ops: &mut Ops,
    ) -> Result<[u8; PROOF_LEN], CardError> {
        CardBitProver::new(self, x, ctx, bit, witness, rng, ops)?
            .respond(ops)?
            .release(ops)
    }

    /// r·H, then G added in constant time when `bit` is 1.
    pub(super) fn pedersen(
        &self,
        bit: Choice,
        r: &Scalar,
        ops: &mut Ops,
    ) -> Result<Point, CardError> {
        let h = self.h;
        let blind = g!(r * h).mark_zero();
        let with_g = g!(blind + G);
        ops.mul += 1;
        ops.add += 1;
        ConditionallySelectable::conditional_select(&blind, &with_g, bit)
            .normalize()
            .non_zero()
            .ok_or(CardError::ZeroValue)
    }

    /// C_v = r_v·H + x_v·G.
    pub fn commit(
        &self,
        secrets: &CardSecrets,
        v: usize,
        ops: &mut Ops,
    ) -> Result<Point, CardError> {
        let r = secrets.blinding(v, ops)?;
        self.pedersen(secrets.bit(v), &r, ops)
    }

    /// The seal (spec §7.2.5, PK.5). It never uses the RNG, so every session gets the same one.
    /// Before returning it checks the link against the device's own S_i, computes the free-bit
    /// commitments a second time (the link doesn't cover them), and computes coin_commit and
    /// h_C (over hP) twice. It runs every step of a [`SealJob`].
    pub fn seal(&self, secrets: &CardSecrets, ops: &mut Ops) -> Result<Seal, CardError> {
        let mut job = SealJob::new();
        loop {
            if let Some(seal) = job.step(self, secrets, ops)? {
                return Ok(seal);
            }
        }
    }

    /// The prover for C_v's bit proof (role 0, dom h_C, index v), after its first slice.
    pub fn commit_prover<R: RngCore + CryptoRng>(
        &self,
        secrets: &CardSecrets,
        seal: &Seal,
        v: usize,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<CardBitProver<'_>, CardError> {
        if v >= COMMITTED {
            return Err(CardError::NoSuchBit);
        }
        let c = seal.commitments.get(v).ok_or(CardError::SelfCheck)?;
        let x = point_from_bytes(c, ops).ok_or(CardError::SelfCheck)?;
        let r = secrets.blinding(v, ops)?;
        CardBitProver::new(
            self,
            x,
            BitCtx::commit(seal.h_c, v),
            secrets.bit(v),
            r,
            Some(rng),
            ops,
        )
    }

    /// C_v's bit proof, verified before it's returned.
    pub fn prove_commit_bit<R: RngCore + CryptoRng>(
        &self,
        secrets: &CardSecrets,
        seal: &Seal,
        v: usize,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<[u8; PROOF_LEN], CardError> {
        self.commit_prover(secrets, seal, v, rng, ops)?
            .respond(ops)?
            .release(ops)
    }

    /// All 311 commit proofs, in v order. A test and tooling convenience: the firmware proves one
    /// frame at a time through [`commit_prover`](Self::commit_prover).
    pub fn prove_commit_bits<R: RngCore + CryptoRng>(
        &self,
        secrets: &CardSecrets,
        seal: &Seal,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<Vec<[u8; PROOF_LEN]>, CardError> {
        (0..COMMITTED)
            .map(|v| self.prove_commit_bit(secrets, seal, v, rng, ops))
            .collect()
    }

    /// Opens device word `j + 1` (j < 32) for the phone's N as run `run`: every step of
    /// [`OpenStart`] and [`OpenJob`], with each proof verified before release. A test and tooling
    /// convenience: the firmware runs the job a step per poll.
    #[allow(clippy::too_many_arguments)]
    pub fn open<R: RngCore + CryptoRng>(
        &self,
        secrets: &CardSecrets,
        seal: &Seal,
        n: &[u8; 32],
        j: usize,
        run: u8,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<Opening, CardError> {
        let mut job = OpenJob::new(self, secrets, seal, n, j, run, rng, ops)?.check_witness(ops)?;
        for _ in 0..CARRIES_PER_BOX {
            job.prove_carry(rng, ops)?;
        }
        job.finish()
    }

    /// The phone's check of a seal's points (spec §7.3.1 (2), PK.5): P has no word of 2048 or
    /// more, every C_v parses and isn't the identity or G, Σ_t 2^t·C_t = S_i, and h_C over the
    /// phone's own ctx and the hP of the phone half the device sent is the seal's. Gives the
    /// ctx it was checked under. A seal that passes is pinned ([`Setup::pin`]), not proven: only
    /// the bit proofs rule out integer non-bits.
    pub(super) fn check_seal_points(
        &self,
        context: &Context,
        seal: &Seal,
        phone_half: &[u16; HALF],
        ops: &mut Ops,
    ) -> Result<[u8; 32], VerifyError> {
        if seal.commitments.len() != COMMITTED {
            return Err(VerifyError::Shape);
        }
        check_phone_words(phone_half)?;
        // The link parses the scalar bits' points.
        for c in &seal.commitments[..FREE] {
            point_from_bytes(c, ops).ok_or(VerifyError::BadPoint)?;
        }
        if link(&seal.commitments, ops).ok_or(VerifyError::BadPoint)? != context.image.image {
            return Err(VerifyError::Link);
        }
        let ctx = context.hash(ops);
        let phone_hash = phone_half_hash(phone_half, ops);
        if seal_hash(&ctx, &seal.coin_commit, &phone_hash, &seal.commitments, ops) != seal.h_c {
            return Err(VerifyError::SealHash);
        }
        Ok(ctx)
    }

    /// Stock verification of C_v's bit proof against a seal. False for a v it has no C for.
    pub(super) fn commit_proof_holds(
        &self,
        seal: &Seal,
        v: usize,
        proof: &[u8; PROOF_LEN],
        ops: &mut Ops,
    ) -> bool {
        match seal
            .commitments
            .get(v)
            .and_then(|c| point_from_bytes(c, ops))
        {
            Some(x) => self.verify_bit(&x, BitCtx::commit(seal.h_c, v), proof, ops),
            None => false,
        }
    }

    /// The phone's whole check of a seal: its points, then all 311 bit proofs. A test and tooling
    /// convenience: the phone pins a seal ([`Setup::pin`]) and verifies its proofs a frame at a
    /// time ([`Setup::verify_commit_proofs`]).
    pub fn verify_seal<'s>(
        &self,
        context: &Context,
        seal: &'s Seal,
        phone_half: &[u16; HALF],
        proofs: &[[u8; PROOF_LEN]],
        ops: &mut Ops,
    ) -> Result<Proven<'s>, VerifyError> {
        let ctx = self.check_seal_points(context, seal, phone_half, ops)?;
        if proofs.len() != COMMITTED {
            return Err(VerifyError::Shape);
        }
        for (v, proof) in proofs.iter().enumerate() {
            if !self.commit_proof_holds(seal, v, proof, ops) {
                return Err(VerifyError::CommitProof(v));
            }
        }
        Ok(Proven::new(seal, ctx, context.meta.index))
    }

    /// The phone's check of an opening of device word `j + 1` (spec L1.5.3.2, PK.5), against
    /// the phone half P the phone holds:
    /// - j is a device word (j < 32), and the opening is for it;
    /// - y < 2048, and the opening has 77 Q's and 77 proofs;
    /// - P is the one the seal was pinned with: no word of 2048 or more, and h_C over the
    ///   proven seal's ctx and hP(P) is the seal's (binding (1), PK.6);
    /// - for every output bit β, D_β = Σ_{v∈T_jβ} C_v − y′_β·G equals Σ_l 2^{l+1}·Q_{β,l},
    ///   with y′ = y XOR the public word from the proven context's index and P;
    /// - every carry proof verifies under open_dom = TH(open, h_C ‖ hP ‖ N ‖ j ‖ y ‖ Q's).
    ///
    /// Every device word is computed, so every one is opened this way; phone words have no
    /// openings.
    pub fn verify_opening(
        &self,
        seal: &Proven<'_>,
        phone_half: &[u16; HALF],
        n: &[u8; 32],
        j: usize,
        opening: &Opening,
        ops: &mut Ops,
    ) -> Result<(), VerifyError> {
        if j >= HALF {
            return Err(VerifyError::NoSuchBox);
        }
        if opening.j as usize != j {
            return Err(VerifyError::WrongBox);
        }
        if opening.y >= 1 << WORD_BITS {
            return Err(VerifyError::WordRange);
        }
        if opening.carries.len() != CARRIES_PER_BOX || opening.proofs.len() != CARRIES_PER_BOX {
            return Err(VerifyError::Shape);
        }
        let phone_hash = seal.check_phone_half(phone_half, ops)?;
        let sets = BoxSets::new(j);
        let public = sets.public_word(seal.index, phone_half);
        let seal = seal.seal;

        // Σ_{v∈T_jβ} C_v for every β, in one pass over the C's.
        let mut sums = [Point::<NonNormal, Public, Zero>::zero(); WORD_BITS];
        for (v, c) in seal.commitments.iter().enumerate() {
            let c = point_from_bytes(c, ops).ok_or(VerifyError::BadPoint)?;
            for (sum, set) in sums.iter_mut().zip(&sets.committed) {
                if set.get(v) {
                    let total = *sum;
                    *sum = g!(total + c);
                    ops.add += 1;
                }
            }
        }

        let mut carries = Vec::with_capacity(CARRIES_PER_BOX);
        for q in &opening.carries {
            carries.push(point_from_bytes(q, ops).ok_or(VerifyError::BadPoint)?);
        }
        // The group equations first: they're cheap next to the proofs.
        for (beta, (sum, qs)) in sums.into_iter().zip(carries.chunks(CARRY_BITS)).enumerate() {
            let y_beta = ((opening.y ^ public) >> beta) & 1;
            let expected = if y_beta == 1 {
                ops.add += 1;
                g!(sum - G)
            } else {
                sum
            };
            if carry_sum(qs, ops) != expected {
                return Err(VerifyError::Parity(beta));
            }
        }
        let dom = open_domain(
            &seal.h_c,
            &phone_hash,
            n,
            opening.j,
            opening.y,
            &opening.carries,
            ops,
        );
        for (k, (q, proof)) in carries.iter().zip(&opening.proofs).enumerate() {
            if !self.verify_bit(q, BitCtx::carry(dom, k), proof, ops) {
                return Err(VerifyError::CarryProof(k));
            }
        }
        Ok(())
    }
}

/// Σ_l 2^{l+1}·Q_l, by Horner from l = 6 down.
pub(super) fn carry_sum(carries: &[Point], ops: &mut Ops) -> Point<NonNormal, Public, Zero> {
    let mut sum = Point::<NonNormal, Public, Zero>::zero();
    for q in carries.iter().rev() {
        sum = g!(sum + sum + q);
    }
    ops.add += 2 * carries.len() as u32 + 1;
    g!(sum + sum)
}

/// The fixed-order device prover for one bit proof (spec §7.4.2).
///
/// Stock `Or::announce` and `respond` branch on the bit, so the order of their expensive steps
/// shows it. This draws what stock draws, in the same order, then always computes k·H before
/// the simulated branch and places every value with constant-time selects, so both bits do the
/// same work in the same order and the proof is stock's byte for byte. It runs in three slices:
/// [`new`](Self::new) (k·H), [`respond`](Self::respond) (the simulated branch and the challenge)
/// and [`Unverified::release`] (stock verification). The bit, the witness and the nonce are
/// wiped when it's dropped.
pub struct CardBitProver<'a> {
    setup: &'a Setup,
    statement: Statement,
    transcript: CardTranscript,
    bit: Choice,
    witness: Scalar,
    fake_challenge: [u8; 16],
    fake_response: Scalar<Public, Zero>,
    nonce: Scalar,
    real: Point,
}

impl<'a> CardBitProver<'a> {
    /// Seeds the nonce RNG as `FiatShamir::prove` does, hedged with 32 bytes from `rng`, draws
    /// the simulated challenge, the simulated response and the nonce, then computes k·H.
    pub fn new<R: RngCore + CryptoRng>(
        setup: &'a Setup,
        x: Point,
        ctx: BitCtx,
        bit: Choice,
        witness: Scalar,
        rng: Option<&mut R>,
        ops: &mut Ops,
    ) -> Result<Self, CardError> {
        let statement = setup.statement(x, ctx, ops).ok_or(CardError::SelfCheck)?;
        let mut transcript = setup.fs.transcript.clone();
        transcript.add_statement(&setup.fs.sigma, &statement);
        // Stock hashes the witness's scalar whichever side it's on, so `Left` seeds the same
        // RNG for both bits.
        let mut nonce_rng = transcript.gen_rng(&setup.fs.sigma, &Either::Left(witness), rng);
        ops.sha_blocks += STATEMENT_BLOCKS + SEED_BLOCKS;
        // `Or::gen_announce_secret`'s draws, in its order, for both bits.
        let mut fake_challenge = [0u8; 16];
        nonce_rng.fill_bytes(&mut fake_challenge);
        let fake_response = Scalar::random(&mut nonce_rng).public().mark_zero();
        let nonce = Scalar::random(&mut nonce_rng);
        let h = setup.h;
        let real = g!(nonce * h).normalize();
        ops.mul += 1;
        Ok(Self {
            setup,
            statement,
            transcript,
            bit,
            witness,
            fake_challenge,
            fake_response,
            nonce,
            real,
        })
    }

    /// The simulated branch's announcement, the challenge and both responses.
    pub fn respond(self, ops: &mut Ops) -> Result<Unverified<'a>, CardError> {
        let (setup, statement, bit) = (self.setup, self.statement, self.bit);
        let (fake_challenge, fake_response, real) =
            (self.fake_challenge, self.fake_response, self.real);
        let (_, ((h, x), (_, x_minus_g))) = statement;
        // The simulated branch is the right one (X − G) for bit 0 and the left one (X) for bit 1.
        let target = Point::conditional_select(&x_minus_g, &x, bit);
        let e_fake = challenge_scalar(&fake_challenge);
        let fake = g!(fake_response * h - e_fake * target)
            .normalize()
            .non_zero()
            .ok_or(CardError::ZeroValue)?;
        ops.lincomb += 1;
        let announcement = (
            Point::conditional_select(&real, &fake, bit),
            Point::conditional_select(&fake, &real, bit),
        );
        let challenge = self
            .transcript
            .clone()
            .get_challenge(&setup.fs.sigma, &announcement);
        ops.sha_blocks += CHALLENGE_BLOCKS;

        let real_challenge: [u8; 16] = core::array::from_fn(|i| challenge[i] ^ fake_challenge[i]);
        let e_real = challenge_scalar(&real_challenge);
        let (nonce, witness) = (&self.nonce, &self.witness);
        let real_response = s!(nonce + e_real * witness).public();
        let left_challenge: [u8; 16] = core::array::from_fn(|i| {
            u8::conditional_select(&real_challenge[i], &fake_challenge[i], bit)
        });
        let response = (
            (
                select_scalar(&real_response, &fake_response, bit),
                GenericArray::from(left_challenge),
            ),
            select_scalar(&fake_response, &real_response, bit),
        );
        Ok(Unverified {
            setup,
            statement,
            proof: CompactProof {
                challenge,
                response,
            },
        })
    }
}

impl Drop for CardBitProver<'_> {
    fn drop(&mut self) {
        wipe(&mut self.bit, Choice::from(0));
        wipe(&mut self.witness, Scalar::one());
        wipe(&mut self.nonce, Scalar::one());
    }
}

/// A finished bit proof that hasn't been checked. Its bytes come only from
/// [`release`](Self::release), which verifies it first.
pub struct Unverified<'a> {
    setup: &'a Setup,
    statement: Statement,
    proof: BitProof,
}

impl Unverified<'_> {
    /// Verify before release: stock verification against the same X the phone holds. A proof
    /// that fails is never returned.
    pub fn release(self, ops: &mut Ops) -> Result<[u8; PROOF_LEN], CardError> {
        if self
            .setup
            .verify_statement(&self.statement, &self.proof, ops)
        {
            Ok(encode_proof(&self.proof))
        } else {
            Err(CardError::SelfCheck)
        }
    }
}

/// An opened device word: the word, the carry commitments Q_{β,l} at 7β + l, and their bit
/// proofs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opening {
    pub j: u8,
    pub y: u16,
    pub carries: Vec<[u8; POINT_LEN]>,
    pub proofs: Vec<[u8; PROOF_LEN]>,
}

/// The secret side of an opening, wiped when dropped.
pub(super) struct Carries {
    y: u16,
    /// W_β − y′_β = 2q_β, where W_β = Σ_{v∈T_jβ} x_v.
    even: [u32; WORD_BITS],
    /// ρ_β = Σ_{v∈T_jβ} r_v.
    rho: [Scalar<Secret, Zero>; WORD_BITS],
    /// σ_{β,l} at 7β + l.
    sigma: Vec<Scalar>,
    /// The fresh bytes that hedge the σ's, until they're derived.
    fresh: [u8; 32],
}

impl Carries {
    /// q_{β,l} for carry `k` = 7β + l.
    pub(super) fn bit(&self, k: usize) -> Choice {
        bit_choice((self.even[k / CARRY_BITS] >> (1 + k % CARRY_BITS)) & 1 == 1)
    }

    /// σ_{β,l} for carry `k` = 7β + l, once they're derived.
    pub(super) fn sigma(&self, k: usize) -> Option<&Scalar> {
        self.sigma.get(k)
    }

    pub(super) fn word(&self) -> u16 {
        self.y
    }
}

impl Drop for Carries {
    fn drop(&mut self) {
        wipe(&mut self.y, 0);
        wipe(&mut self.even, [0; WORD_BITS]);
        wipe(&mut self.rho, [Scalar::zero(); WORD_BITS]);
        for sigma in &mut self.sigma {
            wipe(sigma, Scalar::one());
        }
        wipe(&mut self.fresh, [0; 32]);
    }
}

/// An opening of one device word under way (spec §7.3.2), in the steps a device would slice it
/// into. It holds y, the 77 Q's and their witnesses. Only [`into_checked`](Self::into_checked),
/// after [`check_next`](Self::check_next) has checked every β, leads on to the carry proofs,
/// so no Q is proven before the witness self-check.
pub struct OpenJob<'a> {
    setup: &'a Setup,
    j: usize,
    carries: Carries,
    points: Vec<[u8; POINT_LEN]>,
    /// Σ_l 2^{l+1}·Q_{β,l}, for the witness self-check.
    sums: Box<[Point<NonNormal, Public, Zero>; WORD_BITS]>,
    dom: [u8; 32],
    /// The β's whose witness check has passed, in order.
    checked: usize,
}

impl<'a> OpenJob<'a> {
    /// y, W_β, q_β and ρ_β, each computed twice and compared; the σ's; the 77 Q's; open_dom.
    /// `rng` gives the 32 fresh bytes that hedge the σ's. It runs every step of an
    /// [`OpenStart`].
    #[allow(clippy::too_many_arguments)]
    pub fn new<R: RngCore + CryptoRng>(
        setup: &'a Setup,
        secrets: &CardSecrets,
        seal: &Seal,
        n: &[u8; 32],
        j: usize,
        run: u8,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<Self, CardError> {
        let mut start = OpenStart::new(setup, secrets, &seal.h_c, n, j, run, rng, ops)?;
        while !start.step(secrets, ops)? {}
        start.finish(ops)
    }

    /// The job once every Q is in and open_dom is hashed over them.
    pub(super) fn from_parts(
        setup: &'a Setup,
        j: usize,
        carries: Carries,
        points: Vec<[u8; POINT_LEN]>,
        sums: Box<[Point<NonNormal, Public, Zero>; WORD_BITS]>,
        dom: [u8; 32],
    ) -> Self {
        Self {
            setup,
            j,
            carries,
            points,
            sums,
            dom,
            checked: 0,
        }
    }

    /// The word being opened.
    pub fn word(&self) -> u16 {
        self.carries.y
    }

    /// The witness self-check before any Q leaves: Σ_l 2^{l+1}·Q_{β,l} = (W_β − y′_β)·G + ρ_β·H
    /// for every β. Given the self-checked seal it's the phone's parity equation. It runs
    /// [`check_next`](Self::check_next) for every β.
    pub fn check_witness(mut self, ops: &mut Ops) -> Result<CheckedOpenJob<'a>, CardError> {
        while !self.check_next(ops)? {}
        self.into_checked()
    }

    /// The witness check of the next β, one lincomb. True once all 11 have passed.
    pub fn check_next(&mut self, ops: &mut Ops) -> Result<bool, CardError> {
        let beta = self.checked;
        if beta < WORD_BITS {
            let h = self.setup.h;
            let even = Scalar::<Secret, Zero>::from(self.carries.even[beta]);
            let rho = &self.carries.rho[beta];
            ops.lincomb += 1;
            if self.sums[beta] != g!(even * G + rho * h) {
                return Err(CardError::SelfCheck);
            }
            self.checked += 1;
        }
        Ok(self.checked == WORD_BITS)
    }

    /// The job that proves the carries, once every β's witness check has passed.
    pub fn into_checked(self) -> Result<CheckedOpenJob<'a>, CardError> {
        if self.checked != WORD_BITS {
            return Err(CardError::SelfCheck);
        }
        let Self {
            setup,
            j,
            carries,
            points,
            dom,
            ..
        } = self;
        Ok(CheckedOpenJob {
            setup,
            j,
            carries,
            points,
            dom,
            proofs: Vec::with_capacity(CARRIES_PER_BOX),
        })
    }
}

/// An opening whose witness self-check passed. It proves the carries in order, k = 7β + l, and
/// keeps a proof only once it has verified as the next carry's, so the opening it finishes
/// holds this job's Q's and their own proofs.
pub struct CheckedOpenJob<'a> {
    setup: &'a Setup,
    j: usize,
    carries: Carries,
    points: Vec<[u8; POINT_LEN]>,
    dom: [u8; 32],
    proofs: Vec<[u8; PROOF_LEN]>,
}

impl<'a> CheckedOpenJob<'a> {
    /// The word being opened.
    pub fn word(&self) -> u16 {
        self.carries.y
    }

    /// The carry the next proof is for, or `None` once all 77 are in.
    pub fn next_carry(&self) -> Option<usize> {
        (self.proofs.len() < CARRIES_PER_BOX).then_some(self.proofs.len())
    }

    /// The prover for the next carry (role 1, dom open_dom, index k), after its first slice.
    pub fn carry_prover<R: RngCore + CryptoRng>(
        &self,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<CardBitProver<'a>, CardError> {
        let k = self.next_carry().ok_or(CardError::SelfCheck)?;
        let x = point_from_bytes(&self.points[k], ops).ok_or(CardError::SelfCheck)?;
        CardBitProver::new(
            self.setup,
            x,
            BitCtx::carry(self.dom, k),
            self.carries.bit(k),
            self.carries.sigma[k],
            Some(rng),
            ops,
        )
    }

    /// Releases the next carry's proof (stock verification) and keeps it. A proof of any other
    /// statement is refused.
    pub fn add_proof(&mut self, proof: Unverified<'a>, ops: &mut Ops) -> Result<(), CardError> {
        let k = self.next_carry().ok_or(CardError::SelfCheck)?;
        let (ctx, ((_, x), _)) = proof.statement;
        if ctx != BitCtx::carry(self.dom, k) || x.to_bytes() != self.points[k] {
            return Err(CardError::SelfCheck);
        }
        self.proofs.push(proof.release(ops)?);
        Ok(())
    }

    /// The next carry's proof, verified before it's kept.
    pub fn prove_carry<R: RngCore + CryptoRng>(
        &mut self,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<(), CardError> {
        let proof = self.carry_prover(rng, ops)?.respond(ops)?;
        self.add_proof(proof, ops)
    }

    /// The opening, once every carry's proof is in.
    pub fn finish(self) -> Result<Opening, CardError> {
        if self.next_carry().is_some() {
            return Err(CardError::SelfCheck);
        }
        Ok(Opening {
            j: self.j as u8,
            y: self.carries.y,
            carries: self.points,
            proofs: self.proofs,
        })
    }
}

/// The word of device word `j + 1` from the secrets: the committed bits' part XOR the public
/// part.
fn device_word(secrets: &CardSecrets, sets: &BoxSets) -> u16 {
    sets.word(&secrets.bits) ^ sets.public_word(secrets.index, &secrets.phone_half)
}

/// The first half of an opening's secret side for device word `j + 1` (spec §7.3.2): y,
/// computed twice and compared, and ρ_β. [`derive_carries`] finishes it.
pub(super) fn start_carries(
    secrets: &CardSecrets,
    sets: &BoxSets,
    fresh: [u8; 32],
    ops: &mut Ops,
) -> Result<Carries, CardError> {
    let y = device_word(secrets, sets);
    if device_word(secrets, sets) != y {
        return Err(CardError::SelfCheck);
    }
    let rho = blinding_sums(secrets, sets, ops)?;
    Ok(Carries {
        y,
        even: [0; WORD_BITS],
        rho,
        sigma: Vec::with_capacity(CARRIES_PER_BOX),
        fresh,
    })
}

/// ρ_β computed again and compared with [`start_carries`]'. On the 64-word card it's a device
/// slice of its own: it derives every blinding.
pub(super) fn recheck_rho(
    carries: &Carries,
    secrets: &CardSecrets,
    sets: &BoxSets,
    ops: &mut Ops,
) -> Result<(), CardError> {
    if blinding_sums(secrets, sets, ops)? != carries.rho {
        return Err(CardError::SelfCheck);
    }
    Ok(())
}

/// The second half, after [`recheck_rho`]: W_β − y′_β and the σ's. Every value derived from
/// the secret bits is computed twice and compared.
pub(super) fn derive_carries(
    carries: &mut Carries,
    secrets: &CardSecrets,
    sets: &BoxSets,
    n: &[u8; 32],
    j: usize,
    run: u8,
    ops: &mut Ops,
) -> Result<(), CardError> {
    if !carries.sigma.is_empty() {
        return Err(CardError::SelfCheck);
    }
    let y = carries.y;
    let public = sets.public_word(secrets.index, &secrets.phone_half);
    let n_tag = n_tag(n, ops);
    let half = Scalar::<Public, Zero>::from(2u32)
        .non_zero()
        .expect("2 isn't 0")
        .invert();

    for beta in 0..WORD_BITS {
        let ones = sets.committed[beta].count_common(&secrets.bits);
        if sets.committed[beta].count_common(&secrets.bits) != ones {
            return Err(CardError::SelfCheck);
        }
        // y′_β is the parity of the committed bits alone, so W_β − y′_β is even.
        let y_beta = (((y ^ public) >> beta) & 1) as u32;
        carries.even[beta] = ones.wrapping_sub(y_beta);

        // σ_{β,1..6} are bound to the box and the run and hedged with fresh bytes, so they
        // never repeat across boxes or runs even with a dead RNG.
        let mut upper = Vec::with_capacity(CARRY_BITS - 1);
        let mut weighted = Scalar::<Secret, Zero>::zero();
        for l in 1..CARRY_BITS {
            let s = h2s(Tagged::new(CARRY_BLIND_TAG)
                .add(&secrets.key)
                .add(&n_tag)
                .add(&[run, j as u8, beta as u8, l as u8])
                .add(&carries.fresh)
                .finish(ops))
            .non_zero()
            .ok_or(CardError::ZeroValue)?;
            let weight = Scalar::<Public, Zero>::from(2u32 << l);
            weighted = s!(weighted + weight * s);
            upper.push(s);
        }
        // σ_{β,0} makes Σ_l 2^{l+1}·σ_{β,l} = ρ_β, so the parity equation holds exactly.
        let rho_beta = carries.rho[beta];
        carries.sigma.push(
            s!(half * (rho_beta - weighted))
                .non_zero()
                .ok_or(CardError::ZeroValue)?,
        );
        carries.sigma.extend(upper);
    }
    wipe(&mut carries.fresh, [0; 32]);
    Ok(())
}

/// ρ_β = Σ_{v∈T_jβ} r_v for every β, deriving each r_v once.
fn blinding_sums(
    secrets: &CardSecrets,
    sets: &BoxSets,
    ops: &mut Ops,
) -> Result<[Scalar<Secret, Zero>; WORD_BITS], CardError> {
    let mut sums = [Scalar::<Secret, Zero>::zero(); WORD_BITS];
    for v in 0..COMMITTED {
        let r = secrets.blinding(v, ops)?;
        for (sum, set) in sums.iter_mut().zip(&sets.committed) {
            // The sets are public, so this branch shows nothing.
            if set.get(v) {
                let total = *sum;
                *sum = s!(total + r);
            }
        }
    }
    Ok(sums)
}

#[cfg(test)]
mod test {
    extern crate std;

    use super::*;
    use crate::{verifiable_card::phone, Fingerprint, ShareBackup};
    use alloc::string::{String, ToString};
    use rand::Rng;
    use rand_chacha::rand_core::SeedableRng;
    use std::{println, time::Instant};

    /// §7.9's key_id and access_structure_id.
    const KEY_ID: [u8; 32] = [0x11; 32];
    const ACCESS_STRUCTURE_ID: [u8; 32] = [0x22; 32];

    fn wallet(rng: &mut impl RngCore) -> (SharedKey, Vec<SecretShare>) {
        let (backups, shared_key) =
            ShareBackup::generate_shares(Scalar::random(rng), 2, 3, Fingerprint::NONE, rng);
        let shares = backups
            .into_iter()
            .map(|backup| backup.extract_secret(&shared_key).unwrap())
            .collect();
        (shared_key, shares)
    }

    /// A share's context, its secrets and its seal.
    fn sealed(
        setup: &Setup,
        shared_key: &SharedKey,
        share: &SecretShare,
    ) -> (Context, CardSecrets, Seal) {
        let mut ops = Ops::default();
        let context = Context::new(
            KEY_ID,
            ACCESS_STRUCTURE_ID,
            share.share_image(),
            shared_key.threshold(),
        )
        .unwrap();
        let secrets =
            CardSecrets::new(share, shared_key, KEY_ID, ACCESS_STRUCTURE_ID, &mut ops).unwrap();
        let seal = setup.seal(&secrets, &mut ops).unwrap();
        (context, secrets, seal)
    }

    /// The seal proven as the phone proves it: every commit proof, then the phone's check.
    fn proven<'a>(
        setup: &Setup,
        context: &Context,
        secrets: &CardSecrets,
        seal: &'a Seal,
    ) -> Proven<'a> {
        let mut ops = Ops::default();
        let proofs = setup
            .prove_commit_bits(secrets, seal, &mut rand::thread_rng(), &mut ops)
            .unwrap();
        setup
            .verify_seal(context, seal, secrets.phone_half(), &proofs, &mut ops)
            .unwrap()
    }

    /// An RNG that only ever gives `self.0`: the spec's aux = 0x01 × 32 case.
    struct ConstRng(u8);

    impl RngCore for ConstRng {
        fn next_u32(&mut self) -> u32 {
            u32::from_ne_bytes([self.0; 4])
        }

        fn next_u64(&mut self) -> u64 {
            u64::from_ne_bytes([self.0; 8])
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(self.0)
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    impl CryptoRng for ConstRng {}

    #[test]
    fn h_is_pinned_and_not_g() {
        let setup = Setup::new();
        assert_ne!(setup.h, *G);
        assert_ne!(setup.h, g!(-G).normalize());
        // Pinned from this implementation, so an accidental change shows. It isn't an
        // independent vector.
        assert_eq!(
            setup.h.to_string(),
            "023a7c4ae8c8288f82d7b9813dcd67b887d011caf06a2b6b92316b90c9cb505bc3"
        );
    }

    #[test]
    fn fixed_order_prover_matches_stock() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        for bit in [false, true] {
            for case in 0..10 {
                let r = Scalar::random(&mut rng);
                let x = setup
                    .pedersen(bit_choice(bit), &r, &mut Ops::default())
                    .unwrap();
                let ctx = BitCtx {
                    role: rng.gen_range(0..2),
                    dom: rng.gen(),
                    index: rng.gen(),
                };
                let statement = setup.statement(x, ctx, &mut Ops::default()).unwrap();
                let witness = if bit {
                    Either::Right(r)
                } else {
                    Either::Left(r)
                };
                let mut ops = Ops::default();
                let (stock, ours) = match case {
                    // The known-answer cases: no RNG, and aux = 0x01 × 32.
                    0 => (
                        setup
                            .fs
                            .prove(&witness, &statement, None::<&mut ChaCha20Rng>),
                        setup.prove_bit(
                            x,
                            ctx,
                            bit_choice(bit),
                            r,
                            None::<&mut ChaCha20Rng>,
                            &mut ops,
                        ),
                    ),
                    1 => (
                        setup.fs.prove(&witness, &statement, Some(&mut ConstRng(1))),
                        setup.prove_bit(
                            x,
                            ctx,
                            bit_choice(bit),
                            r,
                            Some(&mut ConstRng(1)),
                            &mut ops,
                        ),
                    ),
                    _ => {
                        let seed: [u8; 32] = rng.gen();
                        (
                            setup.fs.prove(
                                &witness,
                                &statement,
                                Some(&mut ChaCha20Rng::from_seed(seed)),
                            ),
                            setup.prove_bit(
                                x,
                                ctx,
                                bit_choice(bit),
                                r,
                                Some(&mut ChaCha20Rng::from_seed(seed)),
                                &mut ops,
                            ),
                        )
                    }
                };
                let ours = ours.unwrap();
                assert_eq!(encode_proof(&stock), ours, "bit {bit}, case {case}");
                assert_eq!(decode_proof(&ours), Some(stock));
                assert!(setup.verify_bit(&x, ctx, &ours, &mut Ops::default()));
            }
        }
    }

    #[test]
    fn both_bits_take_the_same_steps() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let ctx = BitCtx::commit(rng.gen(), 9);
        let steps = |bit: bool, rng: &mut rand::rngs::ThreadRng| {
            let r = Scalar::random(&mut *rng);
            let x = setup
                .pedersen(bit_choice(bit), &r, &mut Ops::default())
                .unwrap();
            let mut ops = [Ops::default(); 3];
            let prover =
                CardBitProver::new(&setup, x, ctx, bit_choice(bit), r, Some(rng), &mut ops[0])
                    .unwrap();
            let unverified = prover.respond(&mut ops[1]).unwrap();
            unverified.release(&mut ops[2]).unwrap();
            ops
        };
        let zero = steps(false, &mut rng);
        assert_eq!(zero, steps(true, &mut rng));
        // k·H; the simulated branch; then stock verification's two lincombs.
        assert_eq!(
            zero.map(|ops| (ops.mul, ops.lincomb)),
            [(1, 0), (0, 1), (0, 2)]
        );
    }

    #[test]
    fn a_false_witness_never_leaves_the_prover() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let r = Scalar::random(&mut rng);
        let ctx = BitCtx::commit(rng.gen(), 0);
        let mut ops = Ops::default();
        let one = setup.pedersen(bit_choice(true), &r, &mut ops).unwrap();
        // The wrong bit, or the wrong blinding: self-verification stops both.
        assert_eq!(
            setup.prove_bit(one, ctx, bit_choice(false), r, Some(&mut rng), &mut ops),
            Err(CardError::SelfCheck)
        );
        let other = Scalar::random(&mut rng);
        assert_eq!(
            setup.prove_bit(one, ctx, bit_choice(true), other, Some(&mut rng), &mut ops),
            Err(CardError::SelfCheck)
        );
        // X = G has no statement.
        assert_eq!(
            setup.prove_bit(
                G.normalize(),
                ctx,
                bit_choice(true),
                r,
                Some(&mut rng),
                &mut ops
            ),
            Err(CardError::SelfCheck)
        );
    }

    #[test]
    fn proofs_round_trip_and_refuse_big_scalars() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let r = Scalar::random(&mut rng);
        let ctx = BitCtx::carry(rng.gen(), 76);
        let mut ops = Ops::default();
        let x = setup.pedersen(bit_choice(false), &r, &mut ops).unwrap();
        let proof = setup
            .prove_bit(x, ctx, bit_choice(false), r, Some(&mut rng), &mut ops)
            .unwrap();
        assert_eq!(encode_proof(&decode_proof(&proof).unwrap()), proof);
        for at in [16, 64] {
            let mut big = proof;
            big[at..at + 32].fill(0xff);
            assert_eq!(decode_proof(&big), None);
            assert!(!setup.verify_bit(&x, ctx, &big, &mut ops));
        }
    }

    #[test]
    fn the_link_blinding_cancels() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let (key, shares) = wallet(&mut rng);
        let (_, secrets, _) = sealed(&setup, &key, &shares[0]);
        let mut ops = Ops::default();
        let mut sum = Scalar::<Secret, Zero>::zero();
        for t in (0..256).rev() {
            let r = secrets
                .blinding(committed_of_scalar_bit(t), &mut ops)
                .unwrap();
            sum = s!(sum + sum + r);
        }
        assert!(sum.is_zero());
    }

    #[test]
    fn honest_seal_and_openings_verify_for_every_box() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let (key, shares) = wallet(&mut rng);
        let share = &shares[1];
        let (context, secrets, seal) = sealed(&setup, &key, share);
        let mut ops = Ops::default();
        let proofs = setup
            .prove_commit_bits(&secrets, &seal, &mut rng, &mut ops)
            .unwrap();
        assert_eq!(
            seal.commitments.len() * POINT_LEN + proofs.len() * PROOF_LEN,
            COMMITMENT_BYTES
        );
        let proven = setup
            .verify_seal(&context, &seal, secrets.phone_half(), &proofs, &mut ops)
            .unwrap();

        let card = codec::encode(share, &key).unwrap();
        let phone_half = card.phone_half();
        assert_eq!(*secrets.phone_half(), phone_half);
        assert_eq!(
            *secrets.phone_hash(),
            phone_half_hash(&phone_half, &mut ops)
        );
        let words = card.words;
        let n: [u8; 32] = rng.gen();
        // Every device word is computed and takes the same carry opening; the phone half has
        // none.
        let mut last = None;
        for (j, &word) in words.iter().enumerate().take(HALF) {
            let opening = setup
                .open(&secrets, &seal, &n, j, 0, &mut rng, &mut ops)
                .unwrap();
            assert_eq!(opening.y, word, "box {j}");
            assert_eq!(
                3 + opening.carries.len() * POINT_LEN + opening.proofs.len() * PROOF_LEN,
                OPENING_BYTES
            );
            assert_eq!(
                setup.verify_opening(&proven, &phone_half, &n, j, &opening, &mut ops),
                Ok(()),
                "box {j}"
            );
            last = Some(opening);
        }
        let opening = last.unwrap();
        for j in [HALF, layout::WORDS - 1, layout::WORDS, usize::MAX] {
            assert_eq!(
                setup.open(&secrets, &seal, &n, j, 0, &mut rng, &mut ops),
                Err(CardError::NoSuchBox)
            );
            assert_eq!(
                setup.verify_opening(&proven, &phone_half, &n, j, &opening, &mut ops),
                Err(VerifyError::NoSuchBox)
            );
        }
    }

    #[test]
    fn the_seal_is_deterministic() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let (key, shares) = wallet(&mut rng);
        let share = &shares[2];
        // Two sessions, or two devices holding the same share.
        let (context, secrets, seal) = sealed(&setup, &key, share);
        let (_, again, resealed) = sealed(&setup, &key, share);
        assert_eq!(seal, resealed);
        assert_eq!(secrets.coin(), again.coin());
        let mut ops = Ops::default();
        assert_eq!(coin_commitment(&secrets.coin(), &mut ops), seal.coin_commit);
        let mut pinned = setup
            .pin(&context, seal.clone(), secrets.phone_half(), &mut ops)
            .unwrap();

        // Hedged proofs differ between sessions, but each verifies against the same pin.
        let first = setup
            .prove_commit_bit(&secrets, &seal, 300, &mut rng, &mut ops)
            .unwrap();
        let second = setup
            .prove_commit_bit(&again, &resealed, 300, &mut rng, &mut ops)
            .unwrap();
        assert_ne!(first, second);
        assert!(setup.verify_commit_bit(&mut pinned, 300, &first, &mut ops));
        assert!(setup.verify_commit_bit(&mut pinned, 300, &second, &mut ops));
        assert_eq!(pinned.verified().iter().collect::<Vec<_>>(), [300]);
        assert!(pinned.proven().is_none());

        // Two runs of one box give the same word under different carry commitments.
        let n: [u8; 32] = rng.gen();
        let run0 = setup
            .open(&secrets, &seal, &n, 4, 0, &mut rng, &mut ops)
            .unwrap();
        let run1 = setup
            .open(&secrets, &seal, &n, 4, 1, &mut rng, &mut ops)
            .unwrap();
        assert_eq!(run0.y, run1.y);
        assert!(run0.carries.iter().zip(&run1.carries).all(|(a, b)| a != b));
    }

    /// Both halves of an opening's secret side.
    fn carries(
        secrets: &CardSecrets,
        sets: &BoxSets,
        n: &[u8; 32],
        j: usize,
        run: u8,
        fresh: &[u8; 32],
        ops: &mut Ops,
    ) -> Result<Carries, CardError> {
        let mut carries = start_carries(secrets, sets, *fresh, ops)?;
        derive_carries(&mut carries, secrets, sets, n, j, run, ops)?;
        Ok(carries)
    }

    /// An opening of box `j` claiming its word with bit β flipped. The liar moves Q_{β,0} by
    /// ±½·G so that the parity equation still holds, then proves every carry it can. Q_{β,0} is
    /// no longer a bit commitment, so its proof can only be a forgery.
    fn lying_opening(
        setup: &Setup,
        secrets: &CardSecrets,
        seal: &Seal,
        n: &[u8; 32],
        j: usize,
        beta: usize,
        rng: &mut impl Rng,
    ) -> Opening {
        let mut ops = Ops::default();
        let sets = BoxSets::new(j);
        let honest = carries(secrets, &sets, n, j, 0, &rng.gen(), &mut ops).unwrap();
        let y = honest.y ^ (1 << beta);
        // y′_β flips with bit β of y, so D_β moves by (y′_β − y″_β)·G.
        let public = sets.public_word(secrets.index, &secrets.phone_half);
        let y_beta = ((honest.y ^ public) >> beta) & 1;
        let mut half = Scalar::<Public, Zero>::from(2u32)
            .non_zero()
            .unwrap()
            .invert()
            .mark_zero();
        half.conditional_negate(y_beta == 0);
        let mut points: Vec<Point> = (0..CARRIES_PER_BOX)
            .map(|k| {
                setup
                    .pedersen(honest.bit(k), &honest.sigma[k], &mut ops)
                    .unwrap()
            })
            .collect();
        let lie = CARRY_BITS * beta;
        let moved = points[lie];
        points[lie] = g!(moved + half * G).normalize().non_zero().unwrap();
        let carries: Vec<[u8; 33]> = points.iter().map(|q| q.to_bytes()).collect();
        let dom = open_domain(
            &seal.h_c,
            &secrets.phone_hash,
            n,
            j as u8,
            y,
            &carries,
            &mut ops,
        );
        let proofs = (0..CARRIES_PER_BOX)
            .map(|k| {
                let ctx = BitCtx::carry(dom, k);
                if k == lie {
                    // The fixed-order prover refuses; stock will prove anything.
                    for bit in [false, true] {
                        assert_eq!(
                            setup.prove_bit(
                                points[k],
                                ctx,
                                bit_choice(bit),
                                honest.sigma[k],
                                Some(&mut ConstRng(3)),
                                &mut ops
                            ),
                            Err(CardError::SelfCheck)
                        );
                    }
                    let statement = setup.statement(points[k], ctx, &mut ops).unwrap();
                    let witness = Either::Left(honest.sigma[k]);
                    encode_proof(&setup.fs.prove(&witness, &statement, Some(&mut ConstRng(3))))
                } else {
                    setup
                        .prove_bit(
                            points[k],
                            ctx,
                            honest.bit(k),
                            honest.sigma[k],
                            Some(&mut ConstRng(3)),
                            &mut ops,
                        )
                        .unwrap()
                }
            })
            .collect();
        Opening {
            j: j as u8,
            y,
            carries,
            proofs,
        }
    }

    #[test]
    fn one_bit_lies_are_rejected() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let (key, shares) = wallet(&mut rng);
        let (context, secrets, seal) = sealed(&setup, &key, &shares[0]);
        let proven = proven(&setup, &context, &secrets, &seal);
        let n: [u8; 32] = rng.gen();
        let j = 7;
        let mut ops = Ops::default();
        let opening = setup
            .open(&secrets, &seal, &n, j, 0, &mut rng, &mut ops)
            .unwrap();
        for beta in 0..WORD_BITS {
            // The honest carries under a word with one bit flipped.
            let flipped = Opening {
                y: opening.y ^ (1 << beta),
                ..opening.clone()
            };
            assert_eq!(
                setup.verify_opening(&proven, secrets.phone_half(), &n, j, &flipped, &mut ops),
                Err(VerifyError::Parity(beta))
            );
        }
        // Carries made to fit the lie: the group equations hold and a bit proof fails.
        for beta in [0, 5, 10] {
            let lie = lying_opening(&setup, &secrets, &seal, &n, j, beta, &mut rng);
            assert_eq!(
                setup.verify_opening(&proven, secrets.phone_half(), &n, j, &lie, &mut ops),
                Err(VerifyError::CarryProof(CARRY_BITS * beta))
            );
        }
        // Tampered proofs and carries.
        for at in [0, 20, 50, 70] {
            let mut tampered = opening.clone();
            tampered.proofs[33][at] ^= 1;
            assert_eq!(
                setup.verify_opening(&proven, secrets.phone_half(), &n, j, &tampered, &mut ops),
                Err(VerifyError::CarryProof(33))
            );
        }
        let mut swapped = opening.clone();
        swapped.carries.swap(3, 4);
        assert_eq!(
            setup.verify_opening(&proven, secrets.phone_half(), &n, j, &swapped, &mut ops),
            Err(VerifyError::Parity(0))
        );
        let mut generator = opening.clone();
        generator.carries[3] = G.normalize().to_bytes();
        assert_eq!(
            setup.verify_opening(&proven, secrets.phone_half(), &n, j, &generator, &mut ops),
            Err(VerifyError::BadPoint)
        );
        let out_of_range = Opening {
            y: 2048,
            ..opening.clone()
        };
        assert_eq!(
            setup.verify_opening(
                &proven,
                secrets.phone_half(),
                &n,
                j,
                &out_of_range,
                &mut ops
            ),
            Err(VerifyError::WordRange)
        );
    }

    #[test]
    fn non_bit_commitments_are_rejected() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let h = setup.h;
        let two = Scalar::<Public, Zero>::from(2u32);
        let mut ops = Ops::default();

        // C = 2·G + r·H: no witness gives a proof that verifies.
        let r = Scalar::random(&mut rng);
        let c = g!(r * h + two * G).normalize().non_zero().unwrap();
        let ctx = BitCtx::commit(rng.gen(), 42);
        let statement = setup.statement(c, ctx, &mut ops).unwrap();
        for witness in [Either::Left(r), Either::Right(r)] {
            let proof = setup.fs.prove(&witness, &statement, Some(&mut rng));
            assert!(!setup.verify_bit(&c, ctx, &encode_proof(&proof), &mut ops));
        }
        for bit in [false, true] {
            assert_eq!(
                setup.prove_bit(c, ctx, bit_choice(bit), r, Some(&mut rng), &mut ops),
                Err(CardError::SelfCheck)
            );
        }

        // An integer non-bit card: where the scalar's bits (t + 1, t) are (1, 0), commit
        // (0, 2) instead. The committed integer is the same, so the link and h_C pass, and only
        // C_t's bit proof can catch it.
        let (key, shares) = wallet(&mut rng);
        let (context, secrets, seal) = sealed(&setup, &key, &shares[0]);
        let bit = |t: usize| secrets.bits.get(committed_of_scalar_bit(t));
        let t = (0..255).find(|&t| bit(t + 1) && !bit(t)).unwrap();
        let (low, high) = (committed_of_scalar_bit(t), committed_of_scalar_bit(t + 1));
        let r_low = secrets.blinding(low, &mut ops).unwrap();
        let r_high = secrets.blinding(high, &mut ops).unwrap();
        let mut lie = seal.clone();
        lie.commitments[high] = g!(r_high * h).normalize().to_bytes();
        lie.commitments[low] = g!(r_low * h + two * G).normalize().to_bytes();
        lie.h_c = seal_hash(
            &secrets.ctx,
            &lie.coin_commit,
            &secrets.phone_hash,
            &lie.commitments,
            &mut ops,
        );
        let mut pinned = setup
            .pin(&context, lie.clone(), secrets.phone_half(), &mut ops)
            .unwrap();
        let x_low = point_from_bytes(&lie.commitments[low], &mut ops).unwrap();
        let ctx = BitCtx::commit(lie.h_c, low);
        let statement = setup.statement(x_low, ctx, &mut ops).unwrap();
        for witness in [Either::Left(r_low), Either::Right(r_low)] {
            let proof = setup.fs.prove(&witness, &statement, Some(&mut rng));
            assert!(!setup.verify_commit_bit(&mut pinned, low, &encode_proof(&proof), &mut ops));
        }
        // The other changed bit really is a 0, so its proof is fine.
        let x_high = point_from_bytes(&lie.commitments[high], &mut ops).unwrap();
        let proof = setup
            .prove_bit(
                x_high,
                BitCtx::commit(lie.h_c, high),
                bit_choice(false),
                r_high,
                Some(&mut rng),
                &mut ops,
            )
            .unwrap();
        assert!(setup.verify_commit_bit(&mut pinned, high, &proof, &mut ops));
        // Without C_low's proof the seal never becomes proven.
        assert!(!pinned.verified().get(low) && pinned.proven().is_none());
    }

    #[test]
    fn wrong_share_metadata_or_box_is_rejected() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let (key, shares) = wallet(&mut rng);
        let (context, secrets, seal) = sealed(&setup, &key, &shares[0]);
        let mut ops = Ops::default();
        // The device's ctx, from its own share, is the one the phone builds.
        assert_eq!(secrets.ctx, context.hash(&mut ops));

        // Another share's S_i: the link fails.
        let other = Context {
            image: shares[1].share_image(),
            ..context
        };
        let phone_half = secrets.phone_half();
        assert_eq!(
            setup.pin(&other, seal.clone(), phone_half, &mut ops).err(),
            Some(VerifyError::Link)
        );
        // Another threshold or index, or another wallet: ctx and so h_C differ.
        let threshold = Context {
            meta: Meta {
                threshold: 3,
                ..context.meta
            },
            ..context
        };
        let index = Context {
            meta: Meta {
                index: context.meta.index + 1,
                ..context.meta
            },
            ..context
        };
        let wallet_ids = Context {
            key_id: [0x33; 32],
            ..context
        };
        for wrong in [threshold, index, wallet_ids] {
            assert_eq!(
                setup.pin(&wrong, seal.clone(), phone_half, &mut ops).err(),
                Some(VerifyError::SealHash)
            );
        }
        // A changed C: a scalar bit breaks the link; a free bit only h_C.
        for (v, error) in [(200, VerifyError::Link), (3, VerifyError::SealHash)] {
            let mut tampered = seal.clone();
            tampered.commitments[v] = tampered.commitments[v + 1];
            assert_eq!(
                setup
                    .pin(&context, tampered.clone(), phone_half, &mut ops)
                    .err(),
                Some(error)
            );
        }

        // A commit proof under another h_C, index or role.
        let proof = setup
            .prove_commit_bit(&secrets, &seal, 5, &mut rng, &mut ops)
            .unwrap();
        let mut pinned = setup
            .pin(&context, seal.clone(), phone_half, &mut ops)
            .unwrap();
        assert!(setup.verify_commit_bit(&mut pinned, 5, &proof, &mut ops));
        let x = point_from_bytes(&seal.commitments[5], &mut ops).unwrap();
        for ctx in [
            BitCtx::commit([0; 32], 5),
            BitCtx::commit(seal.h_c, 6),
            BitCtx::carry(seal.h_c, 5),
        ] {
            assert!(!setup.verify_bit(&x, ctx, &proof, &mut ops), "{ctx:?}");
        }
        assert!(!setup.verify_commit_bit(&mut pinned, 6, &proof, &mut ops));

        // Openings: another box asked for, relabelled, or another N.
        let proven = proven(&setup, &context, &secrets, &seal);
        let n: [u8; 32] = rng.gen();
        let j = 12;
        let opening = setup
            .open(&secrets, &seal, &n, j, 0, &mut rng, &mut ops)
            .unwrap();
        assert_eq!(
            setup.verify_opening(&proven, secrets.phone_half(), &n, j, &opening, &mut ops),
            Ok(())
        );
        assert_eq!(
            setup.verify_opening(&proven, secrets.phone_half(), &n, j + 1, &opening, &mut ops),
            Err(VerifyError::WrongBox)
        );
        let relabelled = Opening {
            j: j as u8 + 1,
            ..opening.clone()
        };
        assert!(matches!(
            setup.verify_opening(
                &proven,
                secrets.phone_half(),
                &n,
                j + 1,
                &relabelled,
                &mut ops
            ),
            Err(VerifyError::Parity(_))
        ));
        let other_n: [u8; 32] = rng.gen();
        assert_eq!(
            setup.verify_opening(
                &proven,
                secrets.phone_half(),
                &other_n,
                j,
                &opening,
                &mut ops
            ),
            Err(VerifyError::CarryProof(0))
        );
        // The phone's own index differs. Its h_C would refuse that index, so the proven seal is
        // made by hand: the parity equations themselves use the phone's index.
        let flipped = Proven::new(&seal, secrets.ctx, context.meta.index ^ 1);
        assert!(matches!(
            setup.verify_opening(&flipped, phone_half, &n, j, &opening, &mut ops),
            Err(VerifyError::Parity(_))
        ));
        // Another P, or a P word past 11 bits: refused against h_C before any parity is
        // looked at. A seal pinned under another ctx refuses even the right P.
        let mut other_half = *phone_half;
        other_half[7] ^= 1;
        assert_eq!(
            setup.verify_opening(&proven, &other_half, &n, j, &opening, &mut ops),
            Err(VerifyError::PhoneHalf)
        );
        let mut wide = *phone_half;
        wide[0] |= 1 << WORD_BITS;
        assert_eq!(
            setup.verify_opening(&proven, &wide, &n, j, &opening, &mut ops),
            Err(VerifyError::WordRange)
        );
        let other_ctx = Proven::new(&seal, [0x44; 32], context.meta.index);
        assert_eq!(
            setup.verify_opening(&other_ctx, phone_half, &n, j, &opening, &mut ops),
            Err(VerifyError::PhoneHalf)
        );
        // And the pin itself: another P gives another hP, so another h_C.
        assert_eq!(
            setup
                .pin(&context, seal.clone(), &other_half, &mut ops)
                .err(),
            Some(VerifyError::SealHash)
        );
        assert_eq!(
            setup.pin(&context, seal.clone(), &wide, &mut ops).err(),
            Some(VerifyError::WordRange)
        );
        // No phone word, and nothing past the card, has an opening.
        for j in [HALF, layout::WORDS] {
            assert_eq!(
                setup.open(&secrets, &seal, &n, j, 0, &mut rng, &mut ops),
                Err(CardError::NoSuchBox)
            );
            assert_eq!(
                setup.verify_opening(&proven, phone_half, &n, j, &opening, &mut ops),
                Err(VerifyError::NoSuchBox)
            );
        }
    }

    /// Binding (1) (PK.6): hP is in h_C and in open_dom, so a P the device serves after the pin
    /// must be the pinned one, and an opening made under another P fails.
    #[test]
    fn phone_half_is_bound() {
        let setup = Setup::new();
        let mut rng = rand::thread_rng();
        let (key, shares) = wallet(&mut rng);
        let (context, secrets, seal) = sealed(&setup, &key, &shares[2]);
        let mut ops = Ops::default();
        let phone_half = *secrets.phone_half();
        let mut swapped = phone_half;
        swapped[31] ^= 0x400;
        let swapped_hash = phone_half_hash(&swapped, &mut ops);
        assert_ne!(swapped_hash, *secrets.phone_hash());

        // Pinning with P′: h_C over hP(P′) isn't the seal's.
        assert_eq!(
            setup.pin(&context, seal.clone(), &swapped, &mut ops).err(),
            Some(VerifyError::SealHash)
        );
        // Pinned with P, a re-pull serving P′ is refused, and P itself passes, giving hP.
        let mut pinned = setup
            .pin(&context, seal.clone(), &phone_half, &mut ops)
            .unwrap();
        assert_eq!(
            pinned.check_phone_half(&swapped, &mut ops),
            Err(VerifyError::PhoneHalf)
        );
        assert_eq!(
            pinned.check_phone_half(&phone_half, &mut ops),
            Ok(*secrets.phone_hash())
        );
        let mut wide = phone_half;
        wide[5] = 2048;
        assert_eq!(
            pinned.check_phone_half(&wide, &mut ops),
            Err(VerifyError::WordRange)
        );
        // The phone's own record of the pin checks P the same way.
        let recorded = phone::PinnedSeal::from_record(seal.clone(), &context, Bits::default());
        assert_eq!(recorded, pinned);
        assert_eq!(
            recorded.check_phone_half(&swapped, &mut ops),
            Err(VerifyError::PhoneHalf)
        );

        let proofs = setup
            .prove_commit_bits(&secrets, &seal, &mut rng, &mut ops)
            .unwrap();
        for (from, frame) in proofs.chunks(PROOFS_PER_FRAME).enumerate() {
            setup
                .verify_commit_proofs(
                    &mut pinned,
                    (from * PROOFS_PER_FRAME) as u16,
                    frame,
                    &mut ops,
                )
                .unwrap();
        }
        let proven = pinned.proven().unwrap();
        assert_eq!(
            proven.check_phone_half(&swapped, &mut ops),
            Err(VerifyError::PhoneHalf)
        );
        let n: [u8; 32] = rng.gen();
        let d = derive_boxes(&seal.h_c, &n, &secrets.coin(), &mut ops);
        let opening = setup
            .open(&secrets, &seal, &n, d[0] as usize, 0, &mut rng, &mut ops)
            .unwrap();
        assert_eq!(
            setup.check_opening(&proven, &phone_half, &n, &d, d[0], &opening, &mut ops),
            Ok(opening.y)
        );
        // The phone checking an honest opening against P′ refuses P′, not the opening.
        assert_eq!(
            setup.check_opening(&proven, &swapped, &n, &d, d[0], &opening, &mut ops),
            Err(VerifyError::PhoneHalf)
        );

        // A carry proof under another hP: open_dom differs, so it fails.
        let honest_dom = open_domain(
            &seal.h_c,
            secrets.phone_hash(),
            &n,
            opening.j,
            opening.y,
            &opening.carries,
            &mut ops,
        );
        let swapped_dom = open_domain(
            &seal.h_c,
            &swapped_hash,
            &n,
            opening.j,
            opening.y,
            &opening.carries,
            &mut ops,
        );
        assert_ne!(honest_dom, swapped_dom);
        let q = point_from_bytes(&opening.carries[0], &mut ops).unwrap();
        assert!(setup.verify_bit(
            &q,
            BitCtx::carry(honest_dom, 0),
            &opening.proofs[0],
            &mut ops
        ));
        assert!(!setup.verify_bit(
            &q,
            BitCtx::carry(swapped_dom, 0),
            &opening.proofs[0],
            &mut ops
        ));
    }

    /// PK.11, owner B6: D is drawn evenly from all 32 device words, a 6-word device-half lie is
    /// caught at the design rate 1 − C(26,5)/C(32,5) wherever it sits, and every device word has
    /// dense committed parity sets (|T| ≥ 128). That every word D can name opens and verifies is
    /// `honest_seal_and_openings_verify_for_every_box`.
    #[test]
    fn every_d_word_is_a_carry_opening() {
        let mut rng = ChaCha20Rng::from_seed([11; 32]);
        let mut ops = Ops::default();
        let draws = 10_000;
        let mut hits = [0u32; HALF];
        // 6-word lies (0-based j): the old random words 5, 11, 17, 23, 29 plus word 1, a run at
        // each end, and a spread.
        let lies: [[u8; 6]; 4] = [
            [0, 5, 11, 17, 23, 29],
            [0, 1, 2, 3, 4, 5],
            [26, 27, 28, 29, 30, 31],
            [2, 8, 13, 19, 24, 30],
        ];
        let mut caught = [0u32; 4];
        for _ in 0..draws {
            let d = derive_boxes(&rng.gen(), &rng.gen(), &rng.gen(), &mut ops);
            for &j in &d {
                hits[j as usize] += 1;
            }
            for (lie, count) in lies.iter().zip(&mut caught) {
                if d.iter().any(|j| lie.contains(j)) {
                    *count += 1;
                }
            }
        }
        // Caught unless all 5 boxes miss the 6 lied words: 1 − C(26,5)/C(32,5) = 0.6734,
        // 6,734 of 10,000 draws, σ ≈ 47; allow about 4σ.
        let design = draws as u64 * (201_376 - 65_780) / 201_376;
        for (lie, &count) in lies.iter().zip(&caught) {
            assert!(
                (count as u64).abs_diff(design) < 200,
                "lie {lie:?}: caught {count} of {draws}"
            );
        }
        // Each word is in D with probability 5/32: 1,562.5 of 10,000, σ ≈ 36.
        for (j, &count) in hits.iter().enumerate() {
            assert!((1_350..=1_780).contains(&count), "word {}: {count}", j + 1);
        }
        assert_eq!(hits.iter().sum::<u32>(), draws * CHECK_BOXES as u32);
        // Every device word has a carry opening's sets, fed by the committed bits.
        for j in 0..HALF {
            let sets = BoxSets::new(j);
            for (beta, set) in sets.committed.iter().enumerate() {
                assert!(
                    set.len() >= 128,
                    "word {}, bit {beta}: {}",
                    j + 1,
                    set.len()
                );
                assert!(set.len() <= layout::MAX_SET);
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| alloc::format!("{b:02x}")).collect()
    }

    fn sha_of<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
        hex(&parts
            .into_iter()
            .fold(Sha256::new(), |hash, part| hash.chain_update(part))
            .finalize())
    }

    /// §7.9's share: s = h2s(SHA256("frostsnap card tv1 s")), i = 3, t = 2, and
    /// F = [a0·G, a1·G] with a0 = h2s(SHA256("frostsnap card tv1 a0")) and F(3) = s·G.
    fn spec_share() -> (SharedKey, SecretShare) {
        let s = h2s(Sha256::digest(b"frostsnap card tv1 s").into());
        let a0 = h2s(Sha256::digest(b"frostsnap card tv1 a0").into());
        let index = Scalar::<Public, Zero>::from(3u32).non_zero().unwrap();
        let i_inv = index.invert();
        let a1 = s!((s - a0) * i_inv);
        let key = SharedKey::from_poly(vec![g!(a0 * G).normalize(), g!(a1 * G).normalize()]);
        (key.non_zero().unwrap(), SecretShare { index, share: s })
    }

    /// §7.9's known answers on its fixed inputs (T4, T5, T6), as (name, value) pairs.
    fn spec_answers() -> Vec<(String, String)> {
        let setup = Setup::new();
        let (key, share) = spec_share();
        assert_eq!(key.share_image(share.index), share.share_image());
        let (context, secrets, seal) = sealed(&setup, &key, &share);
        let mut ops = Ops::default();
        let mut answers = Vec::new();
        let mut add = |name: &str, value: String| answers.push((name.to_string(), value));

        // T5: the seal.
        add("ctx", hex(&context.hash(&mut ops)));
        add("K_card", hex(&secrets.key));
        for v in [0, FREE, V_LINK] {
            let r = secrets.blinding(v, &mut ops).unwrap();
            add(&alloc::format!("r_{v}"), hex(&r.to_bytes()));
        }
        for v in [0, V_LINK, COMMITTED - 1] {
            add(&alloc::format!("C_{v}"), hex(&seal.commitments[v]));
        }
        add("coin", hex(&secrets.coin()));
        add("coin_commit", hex(&seal.coin_commit));
        add("hP", hex(secrets.phone_hash()));
        add("h_C", hex(&seal.h_c));

        // T4: commit proofs with no RNG, and with aux = 0x01 × 32. The nonce seed hashes the
        // statement and the witness, so a change to how a proof is built shows here first.
        let kat_vs = [0, FREE - 1, FREE, V_LINK, COMMITTED - 1];
        let unhedged: Vec<[u8; PROOF_LEN]> = kat_vs
            .iter()
            .map(|&v| {
                let x = point_from_bytes(&seal.commitments[v], &mut ops).unwrap();
                let r = secrets.blinding(v, &mut ops).unwrap();
                setup
                    .prove_bit(
                        x,
                        BitCtx::commit(seal.h_c, v),
                        secrets.bit(v),
                        r,
                        None::<&mut ChaCha20Rng>,
                        &mut ops,
                    )
                    .unwrap()
            })
            .collect();
        add("proof v=0, no rng", hex(&unhedged[0]));
        add(
            "proofs v=0,54,55,300,310, no rng: sha256",
            sha_of(unhedged.iter().map(|p| &p[..])),
        );
        let hedged: Vec<[u8; PROOF_LEN]> = kat_vs
            .iter()
            .map(|&v| {
                setup
                    .prove_commit_bit(&secrets, &seal, v, &mut ConstRng(1), &mut ops)
                    .unwrap()
            })
            .collect();
        add(
            "proofs v=0,54,55,300,310, aux 0x01: sha256",
            sha_of(hedged.iter().map(|p| &p[..])),
        );

        // T6: words 1, 13 and 32, run 0, each under a fresh ChaCha20Rng([0; 32]).
        let n = [0x09; 32];
        for j in [0, 12, 31] {
            let sets = BoxSets::new(j);
            let job = OpenJob::new(
                &setup,
                &secrets,
                &seal,
                &n,
                j,
                0,
                &mut ChaCha20Rng::from_seed([0; 32]),
                &mut ops,
            )
            .unwrap();
            // W_β; y′_β is its parity, and q_β half the rest.
            let w: Vec<u32> = (0..WORD_BITS)
                .map(|beta| sets.committed[beta].count_common(&secrets.bits))
                .collect();
            for (beta, &w) in w.iter().enumerate() {
                assert_eq!(job.carries.even[beta], w & !1, "box {j}, β = {beta}");
            }
            let opening = setup
                .open(
                    &secrets,
                    &seal,
                    &n,
                    j,
                    0,
                    &mut ChaCha20Rng::from_seed([0; 32]),
                    &mut ops,
                )
                .unwrap();
            assert_eq!(opening.carries, job.points);
            let dom = open_domain(
                &seal.h_c,
                secrets.phone_hash(),
                &n,
                j as u8,
                opening.y,
                &opening.carries,
                &mut ops,
            );
            assert_eq!(dom, job.dom);
            let name = |what: &str| alloc::format!("box {} {what}", j + 1);
            add(&name("y"), opening.y.to_string());
            let w: Vec<String> = w.iter().map(|w| w.to_string()).collect();
            add(&name("W"), w.join(","));
            add(
                &name("Q's: sha256"),
                sha_of(opening.carries.iter().map(|q| &q[..])),
            );
            add(&name("open_dom"), hex(&dom));
            add(
                &name("proofs: sha256"),
                sha_of(opening.proofs.iter().map(|p| &p[..])),
            );
        }
        answers
    }

    /// Pinned from this implementation on C′ (stage S2). `verifiable_card_review.rs` re-derives
    /// the seal, hP and the openings from the spec text (PK.2, PK.5) and checks this
    /// implementation against them, but none of these is an independent vector until a second
    /// implementation (T13) reproduces them.
    const KNOWN_ANSWERS: &[(&str, &str)] = &[
    ("ctx", "1d12d5dec1074a66562c2289985d2d8e8a30f1713ea1a8146c0954bad03a01a7"),
    ("K_card", "e6d6a1f02f4b772cbc5a27816e578719cfd3c5efdc8a23299e14d1af526b7f9a"),
    ("r_0", "5ae2c79d95ff6d8c99d998a49e380a630606bde0d942c7cae0f89967efbe17b0"),
    ("r_55", "320a6d99c365138edb54809db8395ec1ff3579fc14adad95554127ceacb48e61"),
    ("r_300", "86787bdb3f79d7ef146701d3d34291750c0c955fecc3c1ad4a403ed4fd625e90"),
    ("C_0", "02d27f3847d373473baf939c00abc6aa772a15e314caf615d7d3bccbfb332f2e8a"),
    ("C_300", "027a5b4cc79e29c65efb23b30bcc19ed79cf0f9eebdd2e0ba9f8e74d8b2c319afc"),
    ("C_310", "03e728670e442921bb488fc0e31939cbbad4f19548a3a056fe071ba7ad81d68b2a"),
    ("coin", "d45a1528a7a3d5397ec8d17d6ec42d2d32ec20e0f7d63e5513fc687db6d5b7aa"),
    ("coin_commit", "a5e0c6fb163ca56c4952a71d7f6f1f03043cb46277aed0c3c734e7f58c6f4c7d"),
    ("hP", "728611f962053c95a2c66e9fb95eae8f23b8ca6a070c566e1bf9ddb7bf60488e"),
    ("h_C", "2d70e7b74ec88a36e40578fc31ca9a36598f20694cd88bab426d67eb0dd92fb5"),
    ("proof v=0, no rng", "683904e004c3d3e2d15ecc7b0cad9d2ef2022c74a5c0a2163686cb9abce66f98d472925421e2dd77beefc8843324801d42c67da0570c1c359641719677971dffd60beee5282d172dc4f2c406e75f78cfab73131409013d4315d982f931415d30"),
    ("proofs v=0,54,55,300,310, no rng: sha256", "22f1852f596a1725f927e9d52821f3878d5717bff1c4bea66576f5104837f125"),
    ("proofs v=0,54,55,300,310, aux 0x01: sha256", "1ab8ffbb37990af6c0a0ca5dd56656fb9b8a6f89692eb8bd111b5796893196c2"),
    ("box 1 y", "996"),
    ("box 1 W", "77,72,62,61,60,68,73,72,73,61,77"),
    ("box 1 Q's: sha256", "6cc228307be3d8ef8a07b99448bf2818d0f85df712a9bb5973eecde6fa82dc23"),
    ("box 1 open_dom", "abe5afda54092581ca573c85304476a32f5e34bd61413da314013ec1a7f31d03"),
    ("box 1 proofs: sha256", "64365f7f1c9b89f4131719e1c0994a60f68dff1a44cbef9e7285fd63f72349fc"),
    ("box 13 y", "1720"),
    ("box 13 W", "71,68,75,81,79,72,75,70,78,74,58"),
    ("box 13 Q's: sha256", "ff960bdb412ae0487c54207dd72b6df328277117719f39c3ca8bc0e79d8ef84f"),
    ("box 13 open_dom", "989c16d08bc426b3eba74985b90a24a8b7e41eb047e3fdaee2f057095e2bdab8"),
    ("box 13 proofs: sha256", "25ad13cf1a8702f6adbb4b7149b5909dbc0aa16a5f45d2cf83bfb9f145f5e616"),
    ("box 32 y", "1291"),
    ("box 32 W", "78,59,73,69,71,64,66,69,70,73,68"),
    ("box 32 Q's: sha256", "79fd231cb6f41f4f07873d5ee06f019e964262ef0aafafc8c8ba1d1aab996f0f"),
    ("box 32 open_dom", "8fcd6be424d39e0d6b91dde96be4aef0e4c4a39777b44d3df45d8b6d6fd84c4b"),
    ("box 32 proofs: sha256", "c07f2ed158b990bcab5da64876b76efbde2a2bf2230a5c5c7a949497b2ed4468"),
    ];

    #[test]
    fn known_answers_on_the_spec_inputs() {
        let answers = spec_answers();
        let table: String = answers
            .iter()
            .map(|(name, value)| alloc::format!("    (\"{name}\", \"{value}\"),\n"))
            .collect();
        assert_eq!(answers.len(), KNOWN_ANSWERS.len(), "\n{table}");
        for ((name, value), &(pinned_name, pinned)) in answers.iter().zip(KNOWN_ANSWERS) {
            assert_eq!(
                (name.as_str(), value.as_str()),
                (pinned_name, pinned),
                "\n{table}"
            );
        }
    }

    /// Runs `f` with fresh counters, returning its output, what it counted and how long it
    /// took in milliseconds.
    fn phase<T>(f: impl FnOnce(&mut Ops) -> T) -> (T, Ops, f64) {
        let mut ops = Ops::default();
        let start = Instant::now();
        let out = f(&mut ops);
        (out, ops, start.elapsed().as_secs_f64() * 1e3)
    }

    /// The operation counts of each phase, on the host. The device bench times the same calls,
    /// so these counts are the basis for comparing its timings with the spec's unit costs. Run
    /// with `--nocapture` to see the table.
    #[test]
    fn op_counts_and_host_timings() {
        let setup = Setup::new();
        let mut rng = ChaCha20Rng::from_seed([7; 32]);
        let (key, shares) = wallet(&mut rng);
        let share = &shares[0];
        let context = Context::new(KEY_ID, ACCESS_STRUCTURE_ID, share.share_image(), 2).unwrap();
        let mut rows = Vec::new();

        let (_, ops, ms) = phase(|_| codec::encode(share, &key).unwrap());
        rows.push(("encode (codec work isn't counted)", ops, ms, None));
        let ((secrets, seal), seal_ops, ms) = phase(|ops| {
            let secrets = CardSecrets::new(share, &key, KEY_ID, ACCESS_STRUCTURE_ID, ops).unwrap();
            let seal = setup.seal(&secrets, ops).unwrap();
            (secrets, seal)
        });
        rows.push(("seal", seal_ops, ms, None));

        let mut each = Vec::new();
        let (proofs, proof_ops, ms) = phase(|ops| {
            (0..COMMITTED)
                .map(|v| {
                    let start = Instant::now();
                    let proof = setup
                        .prove_commit_bit(&secrets, &seal, v, &mut rng, ops)
                        .unwrap();
                    each.push(start.elapsed().as_secs_f64() * 1e3);
                    proof
                })
                .collect::<Vec<_>>()
        });
        rows.push(("311 commit proofs, self-verified", proof_ops, ms, None));
        let (proven, verify_seal_ops, ms) = phase(|ops| {
            setup
                .verify_seal(&context, &seal, secrets.phone_half(), &proofs, ops)
                .unwrap()
        });
        rows.push(("verify seal (phone)", verify_seal_ops, ms, None));

        let n = [0x09; 32];
        let mut open_ops = Vec::new();
        let mut verify_ops = Vec::new();
        // Words 1, 13, 32, 8 and 21.
        for j in [0, 12, 31, 7, 20] {
            let (opening, ops, ms) = phase(|ops| {
                setup
                    .open(&secrets, &seal, &n, j, 0, &mut rng, ops)
                    .unwrap()
            });
            rows.push(("open word", ops, ms, None));
            open_ops.push(ops);
            let (_, ops, ms) = phase(|ops| {
                setup
                    .verify_opening(&proven, secrets.phone_half(), &n, j, &opening, ops)
                    .unwrap()
            });
            rows.push(("verify opening (phone)", ops, ms, None));
            verify_ops.push(ops);
        }

        println!(
            "{:<34} {:>5} {:>7} {:>6} {:>4} {:>6} {:>6} {:>9} {:>11} {:>8}",
            "phase",
            "mul",
            "lincomb",
            "add",
            "norm",
            "decomp",
            "sha",
            "host ms",
            "C3 est ms",
            "spec ms"
        );
        for (name, ops, ms, spec) in &rows {
            println!(
                "{:<34} {:>5} {:>7} {:>6} {:>4} {:>6} {:>6} {:>9.2} {:>11.0} {:>8}",
                name,
                ops.mul,
                ops.lincomb,
                ops.add,
                ops.normalize,
                ops.decompress,
                ops.sha_blocks,
                ms,
                ops.cost_us(&SPEC_COSTS) as f64 / 1e3,
                spec.map(|s: f64| alloc::format!("{s:.0}"))
                    .unwrap_or_default(),
            );
        }
        let mean = each.iter().sum::<f64>() / each.len() as f64;
        let (min, max) = each
            .iter()
            .fold((f64::MAX, 0f64), |(lo, hi), &t| (lo.min(t), hi.max(t)));
        println!(
            "one commit proof on the host: min {min:.2} ms, mean {mean:.2} ms, max {max:.2} ms"
        );
        println!("commitment {COMMITMENT_BYTES} B, one opening {OPENING_BYTES} B");

        // The model each phase's count follows (PK.10).
        let heavy = |ops: &Ops| (ops.mul, ops.lincomb, ops.decompress);
        // 311 C's, the 55 free ones again, and the link's points.
        assert_eq!(heavy(&seal_ops), (311 + 55, 0, 256));
        // Per proof: k·H, the simulated branch, stock verification's two lincombs, and X.
        assert_eq!(heavy(&proof_ops), (311, 3 * 311, 311));
        // The free C's parsed, the link's, then each C again for its proof; two lincombs per
        // proof.
        assert_eq!(heavy(&verify_seal_ops), (0, 2 * 311, 55 + 256 + 311));
        for ops in &open_ops {
            // 77 Q's and 77 proofs; 11 witness checks; the Q's read back to prove them.
            assert_eq!(heavy(ops), (77 + 77, 11 + 3 * 77, 77));
        }
        for ops in &verify_ops {
            // The 311 C's for the parity sums, the 77 Q's; two lincombs per carry proof.
            assert_eq!(heavy(ops), (0, 2 * 77, 311 + 77));
        }
        // At the spec's unit costs (§7.5.1) and the counts above.
        let est = |ops: &Ops| ops.cost_us(&SPEC_COSTS) as f64 / 1e3;
        println!(
            "C3 estimates: seal {:.1} s, proofs {:.1} s, one opening {:.1} s",
            est(&seal_ops) / 1e3,
            est(&proof_ops) / 1e3,
            est(&open_ops[0]) / 1e3
        );
    }
}
