//! Encoding and decoding C′ (packed) cards.
//!
//! Encoding packs u8(i) ‖ ser32(s) into the hidden values h0..h23, scrambles the threshold into
//! the hidden threshold word with μ, a hash of the layout hash L and s, and derives the 36 masks
//! from L and the share (the last 12 also from the wallet polynomial F), with the smallest
//! counter that makes the 64 words distinct. Every mask hash names L, so one share under two
//! layouts never reuses a mask.
//!
//! Decoding is unique decoding (`2e + f <= 3`), then the hidden values (the index, in the clear
//! and proof-bound, and s), then the target check s·G = F(i) (or S_i), then the threshold word
//! and the mask checks: the 24 self-checking masks, which any restore can recompute from the
//! card alone, and with F the 12 wallet-bound masks and the canonical check. The codec reports
//! the mask checks and never refuses on them itself: the device's restore refuses a card whose
//! masks fail (owner B7), and an offline repair tool with all 64 words may ignore them. The
//! threshold is only ever a warning. Past unique decoding,
//! [`repair_search`] tries sets of wrong words, a syndrome prefilter drops almost all of them
//! cheaply by the corrected index, and only s·G in the target accepts a candidate.
//!
//! The spec forbids flash tables for secret values (§2.6), so words, masks and hidden values
//! meet the field through [`TableFree`]: shift-and-reduce multiplication, with no table reads
//! and no branches on the values. Values that depend only on which points are used, such as the
//! Lagrange rows, are public and use [`gf`]'s tables. The `_with` functions take [`Tables`]
//! instead, so a bench can time both.

use super::layout::{
    self, Meta, MetaError, WordMask, COEFFS, FIRST_SHARE_SLOT, FORMAT_ID, HALF, HIDDEN, MASKS,
    MASK_SLOTS, PARITY, RANDOM_HIDDEN, SELF_MASKS, SELF_MASK_SLOTS, THRESHOLD_SLOT, WALLET_MASKS,
    WALLET_MASK_SLOTS, WORDS, WORD_BITS,
};
use crate::reveal_card::{card::Basis, gf};
use alloc::{boxed::Box, vec::Vec};
use core::{
    marker::PhantomData,
    ops::{ControlFlow, RangeInclusive},
};
use schnorr_fun::{
    frost::{SecretShare, ShareImage, ShareIndex, SharedKey},
    fun::{poly, prelude::*, Tag},
};
use sha2::{Digest, Sha256};

// DRAFT: frozen before the first release (spec PK-O3).
/// Tag of the threshold scramble μ.
pub const THR_TAG: &[u8] = b"frostsnap/card/c1/thr";
/// Tag of the self-checking mask hash.
pub const MASK_TAG: &[u8] = b"frostsnap/card/c1/mask";
/// Tag of the wallet-bound mask hash.
pub const MASK_WALLET_TAG: &[u8] = b"frostsnap/card/c1/mask-wallet";
/// Tag of the words hash the device persists with its seal.
pub const WORDS_TAG: &[u8] = b"frostsnap/card/c1/words";
/// x^11 + x^2 + 1, as [`gf`] builds its tables from.
const FIELD_POLY: u32 = 0x805;
const WORD_MASK: u16 = (1 << WORD_BITS) - 1;
/// Mask counters every encode tries, so that picking one takes the same time for every share.
/// 64 words are distinct with probability about 0.37, so a share needs a later counter with
/// probability about 0.63^50 ≈ 2^-33.
pub const FIXED_COUNTERS: u8 = 50;
/// Mask counters one [`EncodeJob::step`] tries.
pub const COUNTERS_PER_STEP: u8 = 10;
/// The most wrong words a repair tries past unique decoding. With the syndrome prefilter and a
/// target that names the index (a share record, or the wallet's index set), two cost about
/// 0.5 s on the ESP32-C3 in the worst case; three would cost about 9 s.
pub const MAX_REPAIR_WRONG: usize = 2;

/// A wallet's root polynomial F, as [`SharedKey::point_polynomial`] gives it.
pub type Poly = [Point<Normal, Public, Zero>];

/// How card words and values are multiplied in GF(2^11).
pub trait Field {
    fn mul(a: u16, b: u16) -> u16;

    /// Adds `y·ℓ_i` to `out`, where `ℓ_i` is the basis polynomial that is 1 at point `i`.
    /// `column` is scratch space as long as `out`.
    fn add_scaled(basis: &mut Basis, i: usize, y: u16, out: &mut [u16], column: &mut [u16]);

    /// The polynomial with these coefficients (lowest first) at `x`, as
    /// [`card::eval`](crate::reveal_card::card::eval) computes it.
    fn eval(coeffs: &[u16], x: u16) -> u16 {
        coeffs.iter().rev().fold(0, |acc, &c| Self::mul(acc, x) ^ c)
    }
}

/// [`gf`]'s EXP and LOG tables in flash. They branch on zero and read flash at addresses that
/// depend on the values, so the spec allows them only for public values. Kept for the bench.
pub enum Tables {}

impl Field for Tables {
    fn mul(a: u16, b: u16) -> u16 {
        gf::mul(a, b)
    }

    fn add_scaled(basis: &mut Basis, i: usize, y: u16, out: &mut [u16], _: &mut [u16]) {
        basis.add_scaled(i, y, out)
    }
}

/// Shift-and-reduce with masks: no table reads and no branches on either value. No secret is
/// ever inverted: the Lagrange rows and bases are built from the public points with [`gf`]'s
/// tables, and only the words and values are multiplied in here. Both operands must be below
/// 2048.
pub enum TableFree {}

impl Field for TableFree {
    fn mul(a: u16, b: u16) -> u16 {
        let (a, b) = (a as u32, b as u32);
        let mut product = 0;
        for i in 0..WORD_BITS {
            product ^= (a << i) & ((b >> i) & 1).wrapping_neg();
        }
        for bit in (WORD_BITS..2 * WORD_BITS - 1).rev() {
            product ^= (FIELD_POLY << (bit - WORD_BITS)) & ((product >> bit) & 1).wrapping_neg();
        }
        product as u16
    }

    fn add_scaled(basis: &mut Basis, i: usize, y: u16, out: &mut [u16], column: &mut [u16]) {
        // ℓ_i depends only on the points, so the tables may build it.
        column.fill(0);
        basis.add_scaled(i, 1, column);
        for (o, &c) in out.iter_mut().zip(column.iter()) {
            *o ^= Self::mul(y, c);
        }
    }
}

fn tagged(tag: &[u8]) -> Sha256 {
    Sha256::default().tag(tag)
}

/// `N` values of 11 bits each from the front of `bytes`, most significant first.
fn eleven_bit_values<const N: usize>(bytes: &[u8]) -> [u16; N] {
    let bit = |n: usize| (bytes[n / 8] >> (7 - n % 8)) as u16 & 1;
    core::array::from_fn(|k| {
        (0..WORD_BITS).fold(0, |value, g| (value << 1) | bit(k * WORD_BITS + g))
    })
}

/// ser(F): each point's 33 bytes, in order.
fn hash_poly(hash: Sha256, poly: &Poly) -> Sha256 {
    poly.iter()
        .fold(hash, |hash, point| hash.chain_update(point.to_bytes()))
}

/// μ, the 11 bits XORed into the threshold word: the first 11 bits of
/// TH("frostsnap/card/c1/thr", L ‖ ser32(s)). It depends on L and s alone, so a decoder can
/// unscramble the threshold as soon as it has s.
pub fn threshold_mask(scalar: Scalar<Secret, Zero>) -> u16 {
    threshold_mask_with(&layout::layout_hash(), scalar)
}

fn threshold_mask_with(layout_hash: &[u8; 32], scalar: Scalar<Secret, Zero>) -> u16 {
    let mut bytes = scalar.to_bytes();
    let mut hasher = tagged(THR_TAG)
        .chain_update(layout_hash)
        .chain_update(bytes);
    wipe(&mut bytes, [0; 32]);
    let mut hash = [0u8; 32];
    hasher.finalize_into_reset((&mut hash).into());
    wipe_hasher(&mut hasher);
    let mask = eleven_bit_values::<1>(&hash)[0];
    wipe(&mut hash, [0; 32]);
    mask
}

/// The self-checking mask hash up to its counter: TH("frostsnap/card/c1/mask", L ‖ ser32(s) ‖
/// u8(i) ‖ …).
fn self_hasher(layout_hash: &[u8; 32], scalar: Scalar<Secret, Zero>, index: u8) -> Sha256 {
    let mut bytes = scalar.to_bytes();
    let hash = tagged(MASK_TAG)
        .chain_update(layout_hash)
        .chain_update(bytes)
        .chain_update([index]);
    wipe(&mut bytes, [0; 32]);
    hash
}

/// The wallet-bound mask hash up to its counter: TH("frostsnap/card/c1/mask-wallet", L ‖
/// ser32(s) ‖ u8(i) ‖ ser(F) ‖ …).
fn wallet_hasher(
    layout_hash: &[u8; 32],
    scalar: Scalar<Secret, Zero>,
    index: u8,
    poly: &Poly,
) -> Sha256 {
    let mut bytes = scalar.to_bytes();
    let hash = tagged(MASK_WALLET_TAG)
        .chain_update(layout_hash)
        .chain_update(bytes)
        .chain_update([index]);
    wipe(&mut bytes, [0; 32]);
    hash_poly(hash, poly)
}

/// m1..m24 for one counter: the first 264 bits of B_0 ‖ B_1, B_b the self hash finished with
/// u8(ctr) ‖ u8(b). The last 248 bits are dropped. Each copy of the hasher is finished in place
/// and wiped: its midstate is a function of s (crypto-core review nit 1).
fn self_masks_from(hasher: &Sha256, counter: u8) -> [u16; SELF_MASKS] {
    let mut blocks = [[0u8; 32]; 2];
    for (b, block) in blocks.iter_mut().enumerate() {
        let mut copy = hasher.clone();
        copy.update([counter, b as u8]);
        copy.finalize_into_reset(block.into());
        wipe_hasher(&mut copy);
    }
    let masks = eleven_bit_values(blocks.as_flattened());
    wipe(&mut blocks, [[0; 32]; 2]);
    masks
}

/// m25..m36 for one counter: the first 132 bits of the wallet hash finished with u8(ctr).
fn wallet_masks_from(hasher: &Sha256, counter: u8) -> [u16; WALLET_MASKS] {
    let mut copy = hasher.clone();
    copy.update([counter]);
    let mut block = [0u8; 32];
    copy.finalize_into_reset((&mut block).into());
    wipe_hasher(&mut copy);
    let masks = eleven_bit_values(&block);
    wipe(&mut block, [0; 32]);
    masks
}

/// The self-checking masks m1..m24 of share `index` with this scalar, at counter `ctr`.
pub fn self_masks(scalar: Scalar<Secret, Zero>, index: u8, ctr: u8) -> [u16; SELF_MASKS] {
    let mut hasher = self_hasher(&layout::layout_hash(), scalar, index);
    let masks = self_masks_from(&hasher, ctr);
    wipe_hasher(&mut hasher);
    masks
}

/// The wallet-bound masks m25..m36 of share `index` with this scalar under F, at counter `ctr`.
pub fn wallet_masks(
    scalar: Scalar<Secret, Zero>,
    index: u8,
    poly: &Poly,
    ctr: u8,
) -> [u16; WALLET_MASKS] {
    let mut hasher = wallet_hasher(&layout::layout_hash(), scalar, index, poly);
    let masks = wallet_masks_from(&hasher, ctr);
    wipe_hasher(&mut hasher);
    masks
}

/// words_hash = TH("frostsnap/card/c1/words", L ‖ BE16(y1) ‖ … ‖ BE16(y64)), which the device
/// persists with its seal and checks every re-encode against.
pub fn card_words_hash(words: &[u16; WORDS]) -> [u8; 32] {
    words
        .iter()
        .fold(
            tagged(WORDS_TAG).chain_update(layout::layout_hash()),
            |hash, word| hash.chain_update(word.to_be_bytes()),
        )
        .finalize()
        .into()
}

/// 1 if the words are pairwise distinct, else 0. It compares all 2,016 pairs without branching
/// on them.
fn distinct(words: &[u16; WORDS]) -> u16 {
    let mut repeated = 0u32;
    for a in 0..WORDS {
        for b in a + 1..WORDS {
            // x - 1 wraps (setting the top bit) only when x = 0, as x < 2^11.
            repeated |= ((words[a] ^ words[b]) as u32).wrapping_sub(1) >> 31;
        }
    }
    (repeated ^ 1) as u16
}

/// 1 if `diff` is 0, else 0, without branching. `diff` must be below 2^31.
fn is_zero(diff: u32) -> u16 {
    (diff.wrapping_sub(1) >> 31) as u16
}

/// A card: its 61 values (in slot order) and its 64 words. They're secret, so it wipes them
/// when it's dropped and its `Debug` shows none of them.
#[derive(Clone, PartialEq, Eq)]
pub struct Card {
    /// The value in each slot of [`layout::BASIS`].
    pub values: [u16; COEFFS],
    /// Word `j + 1` is `words[j]`, a BIP39 word index. Words 1-32 are the device half and
    /// words 33-64 the phone half.
    pub words: [u16; WORDS],
    /// The mask counter. It's never shown: decoders find it from the self-checking masks.
    pub counter: u8,
}

impl Card {
    /// The phone half P, words 33-64: the values in slots 5..37.
    pub fn phone_half(&self) -> [u16; HALF] {
        self.words[layout::PHONE_HALF].try_into().expect("32 words")
    }
}

impl Drop for Card {
    fn drop(&mut self) {
        wipe(&mut self.values, [0; COEFFS]);
        wipe(&mut self.words, [0; WORDS]);
        // A function of s.
        wipe(&mut self.counter, 0);
    }
}

impl core::fmt::Debug for Card {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Card").finish_non_exhaustive()
    }
}

/// Overwrites a secret in place (pitfall 20). `black_box` makes the compiler treat the cleared
/// value as read, so the write isn't dropped as dead.
pub(crate) fn wipe<T: Copy>(secret: &mut T, cleared: T) {
    *secret = cleared;
    core::hint::black_box(secret);
}

/// Overwrites a hasher's whole state, midstate and buffer, with a fresh one (pitfall 20). A
/// mask hasher past L ‖ s is a function of (L, s), from which every mask follows; digest's
/// `CoreWrapper` has no zeroize of its own (crypto-core review nit 1).
pub(crate) fn wipe_hasher(hasher: &mut Sha256) {
    *hasher = Sha256::new();
    core::hint::black_box(hasher);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    Meta(MetaError),
    /// The share scalar is zero.
    ZeroScalar,
    /// No counter up to 255 gives 64 distinct words (probability below 2^-170).
    NoDistinctCounter,
    /// Two encodes of the same share disagree: a fault.
    SelfCheck,
}

impl From<MetaError> for EncodeError {
    fn from(error: MetaError) -> Self {
        EncodeError::Meta(error)
    }
}

/// The card for a share of the wallet with this shared key. It's a function of the share and
/// the wallet's public data alone, so every device and session produce the same card.
pub fn encode(
    share: &SecretShare,
    shared_key: &SharedKey<Normal, impl ZeroChoice>,
) -> Result<Card, EncodeError> {
    encode_with::<TableFree>(share, shared_key)
}

/// [`encode`] twice, refusing the card if the two disagree (spec L1.2.6 step 5). The device
/// encodes this way, as the masks and the threshold word can't be checked any other way.
pub fn encode_checked(
    share: &SecretShare,
    shared_key: &SharedKey<Normal, impl ZeroChoice>,
) -> Result<Card, EncodeError> {
    let card = encode(share, shared_key)?;
    if encode(share, shared_key)? != card {
        return Err(EncodeError::SelfCheck);
    }
    Ok(card)
}

/// [`encode`] with the given arithmetic.
pub fn encode_with<F: Field>(
    share: &SecretShare,
    shared_key: &SharedKey<Normal, impl ZeroChoice>,
) -> Result<Card, EncodeError> {
    let meta = Meta::new(share.index, shared_key.threshold())?;
    encode_meta_with::<F>(share.share, meta, shared_key.point_polynomial())
}

/// [`encode`] with the metadata given and not range checked, and any F. Tests use it to write
/// cards with other thresholds or indices.
pub fn encode_meta(
    scalar: Scalar<Secret, Zero>,
    meta: Meta,
    poly: &Poly,
) -> Result<Card, EncodeError> {
    encode_meta_with::<TableFree>(scalar, meta, poly)
}

fn encode_meta_with<F: Field>(
    scalar: Scalar<Secret, Zero>,
    meta: Meta,
    poly: &Poly,
) -> Result<Card, EncodeError> {
    EncodeJob::<F>::new_meta_with(scalar, meta, poly)?.run()
}

/// [`encode`] in steps a device can run between UI polls. Each [`step`](Self::step) tries
/// [`COUNTERS_PER_STEP`] mask counters; the first [`FIXED_COUNTERS`] take 5 steps for every
/// share. It holds the share's values, so it wipes them when it's dropped and its `Debug` shows
/// none of them.
pub struct EncodeJob<F = TableFree> {
    /// The threshold word and h0..h23; the masks are filled in at the end.
    values: [u16; COEFFS],
    /// The device words with every mask zero. Only the masks change between counters, so the
    /// rest is evaluated once.
    base: [u16; HALF],
    /// ℓ_k(x_j) for the 32 device words: public, built once with the tables, and boxed so the
    /// 3.9 KB stays out of the job's frame.
    rows: Box<[[u16; COEFFS]]>,
    self_hasher: Sha256,
    wallet_hasher: Sha256,
    chosen: [u16; MASKS],
    counter: u8,
    found: u16,
    /// The next counter to try; 256 once all are tried.
    next: u16,
    field: PhantomData<F>,
}

impl EncodeJob {
    /// The job that [`encode`] runs.
    pub fn new(
        share: &SecretShare,
        shared_key: &SharedKey<Normal, impl ZeroChoice>,
    ) -> Result<Self, EncodeError> {
        Self::new_with(share, shared_key)
    }

    /// The job that [`encode_meta`] runs.
    pub fn new_meta(
        scalar: Scalar<Secret, Zero>,
        meta: Meta,
        poly: &Poly,
    ) -> Result<Self, EncodeError> {
        Self::new_meta_with(scalar, meta, poly)
    }
}

impl<F: Field> EncodeJob<F> {
    /// [`EncodeJob::new`] with the given arithmetic.
    pub fn new_with(
        share: &SecretShare,
        shared_key: &SharedKey<Normal, impl ZeroChoice>,
    ) -> Result<Self, EncodeError> {
        let meta = Meta::new(share.index, shared_key.threshold())?;
        Self::new_meta_with(share.share, meta, shared_key.point_polynomial())
    }

    fn new_meta_with(
        scalar: Scalar<Secret, Zero>,
        meta: Meta,
        poly: &Poly,
    ) -> Result<Self, EncodeError> {
        Self::new_raw(scalar, meta.index, meta.threshold as u16, poly)
    }

    /// The job with the threshold field (the 11 bits XORed with μ) given whole, so the
    /// canonical check can re-encode a card whose threshold word doesn't read.
    fn new_raw(
        scalar: Scalar<Secret, Zero>,
        index: u8,
        threshold_field: u16,
        poly: &Poly,
    ) -> Result<Self, EncodeError> {
        if scalar.is_zero() {
            return Err(EncodeError::ZeroScalar);
        }
        let l = layout::layout_hash();
        let mut bytes = scalar.to_bytes();
        let mut values = [0u16; COEFFS];
        layout::pack(index, &bytes, &mut values);
        wipe(&mut bytes, [0; 32]);
        values[THRESHOLD_SLOT] = (threshold_field & WORD_MASK) ^ threshold_mask_with(&l, scalar);
        let weights = layout::basis_weights();
        let rows: Box<[[u16; COEFFS]]> = layout::DEVICE_HALF
            .map(|j| layout::lagrange_row_with(&weights, layout::word_point(j)))
            .collect();
        let base = core::array::from_fn(|j| {
            let row = &rows[j];
            core::iter::once(THRESHOLD_SLOT)
                .chain(FIRST_SHARE_SLOT..COEFFS)
                .fold(0, |word, k| word ^ F::mul(row[k], values[k]))
        });
        Ok(Self {
            values,
            base,
            rows,
            self_hasher: self_hasher(&l, scalar, index),
            wallet_hasher: wallet_hasher(&l, scalar, index, poly),
            chosen: [0; MASKS],
            counter: 0,
            found: 0,
            next: 0,
            field: PhantomData,
        })
    }

    /// m1..m36 for one counter.
    fn masks(&self, counter: u8) -> [u16; MASKS] {
        let mut masks = [0u16; MASKS];
        masks[..SELF_MASKS].copy_from_slice(&self_masks_from(&self.self_hasher, counter));
        masks[SELF_MASKS..].copy_from_slice(&wallet_masks_from(&self.wallet_hasher, counter));
        masks
    }

    /// The words with these masks: each device word is its base plus Σ ℓ_k·m_k over the mask
    /// slots, and the phone half is m5..m36.
    fn words(&self, masks: &[u16; MASKS]) -> [u16; WORDS] {
        let mut words = [0u16; WORDS];
        for (j, word) in words[layout::DEVICE_HALF].iter_mut().enumerate() {
            let row = &self.rows[j];
            *word = MASK_SLOTS
                .iter()
                .zip(masks)
                .fold(self.base[j], |word, (&slot, &m)| {
                    word ^ F::mul(row[slot], m)
                });
        }
        words[layout::PHONE_HALF].copy_from_slice(&masks[MASKS - HALF..]);
        words
    }

    /// Tries the next counters. `Some` once the card is chosen (or, past every counter, the
    /// error); a further step gives the same result again.
    pub fn step(&mut self) -> Option<Result<Card, EncodeError>> {
        let fixed = FIXED_COUNTERS as u16;
        if self.next >= fixed && self.found == 1 {
            return Some(Ok(self.card()));
        }
        let fixed_phase = self.next < fixed;
        let last = if fixed_phase { fixed } else { 256 };
        let end = (self.next + COUNTERS_PER_STEP as u16).min(last);
        for ctr in self.next..end {
            self.next = ctr + 1;
            let ctr = ctr as u8;
            let mut masks = self.masks(ctr);
            let mut words = self.words(&masks);
            if fixed_phase {
                // Every share tries the same counters and keeps the first that works, selected
                // without branching on which.
                let take = distinct(&words) & !self.found & 1;
                let mask = take.wrapping_neg();
                for (c, m) in self.chosen.iter_mut().zip(masks) {
                    *c ^= mask & (*c ^ m);
                }
                self.counter ^= mask as u8 & (self.counter ^ ctr);
                self.found |= take;
            } else if distinct(&words) == 1 {
                // Variable time from here, for the rare share that needs it.
                (self.chosen, self.counter, self.found) = (masks, ctr, 1);
            }
            wipe(&mut masks, [0; MASKS]);
            wipe(&mut words, [0; WORDS]);
            if !fixed_phase && self.found == 1 {
                break;
            }
        }
        if self.next < fixed {
            return None;
        }
        if self.found == 1 {
            return Some(Ok(self.card()));
        }
        (self.next == 256).then_some(Err(EncodeError::NoDistinctCounter))
    }

    /// Every step to the end.
    fn run(mut self) -> Result<Card, EncodeError> {
        loop {
            if let Some(card) = self.step() {
                return card;
            }
        }
    }

    fn card(&self) -> Card {
        let mut values = self.values;
        for (&slot, &m) in MASK_SLOTS.iter().zip(&self.chosen) {
            values[slot] = m;
        }
        Card {
            values,
            words: self.words(&self.chosen),
            counter: self.counter,
        }
    }

    /// Counters tried so far, and the [`FIXED_COUNTERS`] every share tries.
    pub fn progress(&self) -> (u16, u16) {
        (self.next, FIXED_COUNTERS as u16)
    }
}

impl<F> Drop for EncodeJob<F> {
    fn drop(&mut self) {
        wipe(&mut self.values, [0; COEFFS]);
        wipe(&mut self.base, [0; HALF]);
        wipe(&mut self.chosen, [0; MASKS]);
        wipe(&mut self.counter, 0);
        wipe_hasher(&mut self.self_hasher);
        wipe_hasher(&mut self.wallet_hasher);
    }
}

impl<F> core::fmt::Debug for EncodeJob<F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EncodeJob")
            .field("next", &self.next)
            .finish_non_exhaustive()
    }
}

/// What was read from each word: a BIP39 word index, or `None` where the word couldn't be
/// read (an erasure). Decoding refuses an index of 2048 or more.
pub type Entries = [Option<u16>; WORDS];

/// Refuses entries that aren't word indices. It's one branch on all of them together.
fn check_words(entries: &Entries) -> Result<(), DecodeError> {
    let all = entries
        .iter()
        .fold(0, |all, entry| all | entry.unwrap_or_default());
    if all >> WORD_BITS == 0 {
        Ok(())
    } else {
        Err(DecodeError::WordRange)
    }
}

/// The erased words, bit `j` for word `j + 1`.
pub fn erasures(entries: &Entries) -> WordMask {
    entries.iter().enumerate().fold(0, |mask, (j, entry)| {
        mask | ((entry.is_none() as WordMask) << j)
    })
}

/// Word `j + 1` of the codeword with these coefficients. It asserts j < 64: no word index
/// reaches a hidden point.
pub fn eval_word(coeffs: &[u16; COEFFS], j: usize) -> u16 {
    TableFree::eval(coeffs, layout::word_point(j))
}

/// f at hidden point `m` (x = 2019 + m, m < 29). With the encoder, the only way to a hidden
/// point: it takes m, never a word index.
pub fn eval_hidden(coeffs: &[u16; COEFFS], m: usize) -> u16 {
    TableFree::eval(coeffs, layout::hidden_point(m))
}

/// A codeword the entries decoded to, before anything is read from it. Its `Debug` leaves out
/// the coefficients.
#[derive(Clone, PartialEq, Eq)]
pub struct Codeword {
    /// The coefficients of f (the monomial form), lowest first.
    pub coeffs: [u16; COEFFS],
    /// Words whose entries were wrong, bit `j` for word `j + 1`.
    pub corrected: WordMask,
    /// Words that couldn't be read.
    pub erased: WordMask,
    /// The decode used up the redundancy (`2e + f >= 3`), so no spare word checked it. A
    /// combine should suspect this card first.
    pub no_spare_check: bool,
}

impl core::fmt::Debug for Codeword {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Codeword")
            .field("corrected", &self.corrected)
            .field("erased", &self.erased)
            .field("no_spare_check", &self.no_spare_check)
            .finish_non_exhaustive()
    }
}

impl Codeword {
    pub fn new(entries: &Entries, coeffs: [u16; COEFFS]) -> Self {
        // Every word is evaluated, an unreadable one too, so the work doesn't depend on how
        // many there are (owner R2-1).
        let mut corrected = 0;
        for (j, entry) in entries.iter().enumerate() {
            let differs = eval_word(&coeffs, j) != entry.unwrap_or_default();
            corrected |= ((differs & entry.is_some()) as WordMask) << j;
        }
        let erased = erasures(entries);
        let used = 2 * corrected.count_ones() + erased.count_ones();
        Self {
            coeffs,
            corrected,
            erased,
            no_spare_check: used >= PARITY as u32,
        }
    }

    /// Word `j + 1`, j < 64.
    pub fn word(&self, j: usize) -> u16 {
        eval_word(&self.coeffs, j)
    }

    /// f at the 29 hidden points: the 5 hidden random values, then h0..h23.
    pub fn hidden_values(&self) -> [u16; HIDDEN] {
        core::array::from_fn(|m| eval_hidden(&self.coeffs, m))
    }

    /// The 61 values, in slot order.
    pub fn values(&self) -> [u16; COEFFS] {
        values_from(&self.coeffs, &self.hidden_values())
    }
}

/// The 61 values from the coefficients and the hidden values already evaluated.
fn values_from(coeffs: &[u16; COEFFS], hidden: &[u16; HIDDEN]) -> [u16; COEFFS] {
    core::array::from_fn(|slot| match layout::hidden_index(slot) {
        Some(m) => hidden[m],
        None => eval_word(coeffs, HALF + slot - layout::FIRST_PHONE_SLOT),
    })
}

/// Rebuilds codewords from subsets of the entries, keeping its allocations between calls.
struct Rebuilder<F> {
    basis: Basis,
    xs: Vec<u16>,
    coeffs: Vec<u16>,
    column: Vec<u16>,
    field: PhantomData<F>,
}

impl<F: Field> Rebuilder<F> {
    fn new() -> Self {
        Self {
            basis: Basis::default(),
            xs: Vec::with_capacity(COEFFS),
            coeffs: Vec::with_capacity(COEFFS),
            column: vec![0; COEFFS],
            field: PhantomData,
        }
    }

    /// The codeword through every readable entry outside `erased`, if they all fit one. The
    /// first 61 of them fix it and the rest check it.
    fn rebuild(&mut self, entries: &Entries, erased: WordMask) -> Option<[u16; COEFFS]> {
        let (mut coeffs, fits) = self.rebuild_any(entries, erased, &mut Work::default());
        if fits {
            return Some(coeffs);
        }
        wipe(&mut coeffs, [0; COEFFS]);
        None
    }

    /// The polynomial through the first 61 readable entries outside `erased`, and whether the
    /// rest fit it. The work is the same whether they fit or not, and however many are spare:
    /// a missing spare is a check against a used entry, which always fits (owner R2-1).
    fn rebuild_any(
        &mut self,
        entries: &Entries,
        erased: WordMask,
        work: &mut Work,
    ) -> ([u16; COEFFS], bool) {
        let mut used = [0usize; COEFFS];
        let mut spare = [0usize; PARITY];
        let (mut n_used, mut n_spare) = (0, 0);
        for (j, entry) in entries.iter().enumerate() {
            if entry.is_none() || (erased >> j) & 1 == 1 {
                continue;
            }
            if n_used < COEFFS {
                used[n_used] = j;
                n_used += 1;
            } else {
                spare[n_spare] = j;
                n_spare += 1;
            }
        }
        assert_eq!(n_used, COEFFS, "at most {PARITY} words can be erased");
        let word = |j: usize| entries[j].expect("only readable words are used");

        self.xs.clear();
        self.xs.extend(used.iter().map(|&j| layout::word_point(j)));
        let table_ops = self.basis.table_ops;
        self.basis.reset(&self.xs);
        self.coeffs.clear();
        self.coeffs.resize(COEFFS, 0);
        for (i, &j) in used.iter().enumerate() {
            F::add_scaled(
                &mut self.basis,
                i,
                word(j),
                &mut self.coeffs,
                &mut self.column,
            );
        }
        // Every spare word is checked, with no early exit, and a set with fewer than 3 spare
        // words checks used ones in their place, so every rebuild evaluates 3 words.
        work.rebuilds += 1;
        work.table_ops += self.basis.table_ops - table_ops;
        let mut misfits = 0;
        for (k, &j) in spare.iter().enumerate() {
            let (j, real) = if k < n_spare {
                (j, true)
            } else {
                (used[k], false)
            };
            let misfit = F::eval(&self.coeffs, layout::word_point(j)) ^ word(j);
            misfits |= misfit & (real as u16).wrapping_neg();
            work.spare_evals += 1;
        }
        let coeffs = self.coeffs[..].try_into().expect("61 coefficients");
        (coeffs, misfits == 0)
    }
}

/// Advances `c` to the next `c.len()`-subset of `0..n` in lexicographic order.
fn next_combination(c: &mut [usize], n: usize) -> bool {
    let m = c.len();
    for i in (0..m).rev() {
        if c[i] < n - m + i {
            c[i] += 1;
            for j in i + 1..m {
                c[j] = c[j - 1] + 1;
            }
            return true;
        }
    }
    false
}

/// Every set of `e` words from a set of words (as masks), in lexicographic order.
#[derive(Clone, Debug)]
struct Subsets {
    boxes: [u8; WORDS],
    n: usize,
    c: [usize; PARITY],
    e: usize,
    more: bool,
}

impl Subsets {
    fn new(among: WordMask, e: usize) -> Self {
        assert!(e <= PARITY);
        let mut boxes = [0u8; WORDS];
        let mut n = 0;
        for j in 0..WORDS {
            if (among >> j) & 1 == 1 {
                boxes[n] = j as u8;
                n += 1;
            }
        }
        Self {
            boxes,
            n,
            c: [0, 1, 2],
            e,
            more: e <= n,
        }
    }
}

impl Iterator for Subsets {
    type Item = WordMask;

    fn next(&mut self) -> Option<WordMask> {
        if !self.more {
            return None;
        }
        let c = &mut self.c[..self.e];
        let mask = c
            .iter()
            .fold(0 as WordMask, |mask, &i| mask | 1 << self.boxes[i]);
        self.more = next_combination(c, self.n);
        Some(mask)
    }
}

/// Calls `visit` with every set of `e` words from `among` (as masks), in lexicographic order,
/// until it breaks.
fn each_subset<B>(
    among: WordMask,
    e: usize,
    mut visit: impl FnMut(WordMask) -> ControlFlow<B>,
) -> Option<B> {
    Subsets::new(among, e).find_map(|mask| match visit(mask) {
        ControlFlow::Break(b) => Some(b),
        ControlFlow::Continue(()) => None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// More than 3 words would have to be erased: the unreadable ones plus the wrong ones a
    /// repair plan guesses. Or a [`RepairJob`] for more than [`MAX_REPAIR_WRONG`] wrong words.
    TooManyErasures,
    /// No codeword is within unique-decoding distance (`2e + f <= 3`).
    Uncorrectable,
    /// An entry is 2048 or more, so it isn't a word.
    WordRange,
    /// The app's record says the card is this format, and this firmware reads only
    /// [`FORMAT_ID`]: "This card needs newer firmware". A card's format is never read from the
    /// card itself.
    UnknownFormat(u8),
    /// The scalar is zero (mod n). An all-ABANDON card reads as this.
    ZeroScalar,
    /// The index in h0 is 0, which no share has.
    ZeroIndex,
    /// s·G isn't the target's image at the card's index, so this codeword isn't the share. It
    /// comes only once the self-checking masks matched ([`read`] checks them first), so it means
    /// a genuine card for another target: the device's restore reports it as another wallet's
    /// card when it came from unique decoding (owner B11).
    NotInTarget,
    /// Read against a target, the self-checking masks didn't match, so s·G was never checked.
    /// Without a target the masks are only reported ([`MaskCheck`]).
    MasksFailed,
}

/// What a decoded share must be. The device builds it itself.
#[derive(Clone, Copy, Debug)]
pub enum Target<'a> {
    /// The device's own record of the share: T = {S_i} at index i. `threshold` is the
    /// wallet's, which a card's threshold is only compared with.
    Share { image: ShareImage, threshold: usize },
    /// The wallet polynomial F, which the app sends: T = F(i) at the card's index i.
    Polynomial(&'a Poly),
    /// F with its images at the allowed indices already listed ([`ImageTable::target`]): a
    /// listed index costs a lookup, and any other index is checked by evaluating F, as
    /// [`Target::Polynomial`] does. The set only filters repairs.
    Indexed(&'a ImageTable),
}

impl Target<'_> {
    pub fn threshold(&self) -> usize {
        match self {
            Target::Share { threshold, .. } => *threshold,
            Target::Polynomial(poly) => poly.len(),
            Target::Indexed(table) => table.poly.len(),
        }
    }

    /// Whether a repair candidate with this index is worth checking: never 0; for a share
    /// record, its index; for a listed F, an index in its set. The index is proof-bound, so this
    /// filter holds even for a lying chip's card. It filters repairs only: the s·G check
    /// doesn't use the set, so a set that leaves out the card's index (a share the wallet no
    /// longer maps) can make a repair miss but never refuses a card that unique decoding reads.
    pub fn allows(&self, index: u8) -> bool {
        index != 0
            && match self {
                Target::Share { image, .. } => Some(index) == share_index_byte(image.index),
                Target::Polynomial(_) => true,
                Target::Indexed(table) => table.allows(index),
            }
    }

    /// Whether s·G is the target's image at the card's index. There's no index search: the
    /// index is the card's own. A listed F looks its image up, and evaluates F at an index it
    /// doesn't list, as [`Target::Polynomial`] does: the set only filters repairs (build plan
    /// A6).
    fn check(&self, scalar: Scalar<Secret, Zero>, index: u8) -> Result<(), DecodeError> {
        let at = |poly: &Poly| poly::point::eval(poly, Scalar::<Public, Zero>::from(index as u32));
        let image = g!(scalar * G).normalize();
        let holds = index != 0
            && match self {
                Target::Share { image: record, .. } => {
                    Some(index) == share_index_byte(record.index) && image == record.image
                }
                Target::Polynomial(poly) => image == at(poly),
                Target::Indexed(table) if table.allows(index) => table.find(&image, index),
                Target::Indexed(table) => image == at(&table.poly),
            };
        if holds {
            Ok(())
        } else {
            Err(DecodeError::NotInTarget)
        }
    }

    /// F, when the target is the wallet polynomial.
    fn poly(&self) -> Option<&Poly> {
        match self {
            Target::Share { .. } => None,
            Target::Polynomial(poly) => Some(poly),
            Target::Indexed(table) => Some(&table.poly),
        }
    }
}

/// A share index as the card's byte, if it fits one.
fn share_index_byte(index: ShareIndex) -> Option<u8> {
    u32::try_from(index).ok().and_then(|i| u8::try_from(i).ok())
}

/// F's images at the allowed indices (by default 1..=255; the app sends the wallet's share
/// indices), so checking a candidate is a lookup instead of t point operations. The device builds
/// it a step at a time, so no step takes long: first F(2), …, F(t − 1) one per step, then the
/// indices in order with the same forward differences, normalising only the allowed ones.
#[derive(Clone)]
pub struct ImageTable {
    poly: Vec<Point<Normal, Public, Zero>>,
    /// Bit j: index j is allowed. Bit 0 never is.
    allowed: [u32; 8],
    /// The largest allowed index, or 0 when none is.
    last: u8,
    /// F(0), F(1), … until there are t of them; then Δ^k F at the last index stepped.
    diffs: Vec<Point<NonNormal, Public, Zero>>,
    differenced: bool,
    /// Indices stepped through so far.
    stepped: u8,
    /// F(j) for the allowed j stepped through, ascending.
    images: Vec<[u8; 33]>,
}

impl core::fmt::Debug for ImageTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ImageTable")
            .field("threshold", &self.poly.len())
            .field("allowed", &self.allowed_count())
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

/// Point additions in one step of [`ImageTable::build`], about 15 ms on the ESP32-C3, plus a
/// normalisation per allowed index.
const TABLE_ADDS_PER_STEP: usize = 64;

impl ImageTable {
    /// The table of F at `indices` (0 is ignored), or at every index 1..=255 when it's `None`.
    pub fn new(f: &Poly, indices: Option<&[u8]>) -> Self {
        let mut allowed = [0u32; 8];
        match indices {
            Some(indices) => indices
                .iter()
                .for_each(|&j| allowed[j as usize / 32] |= 1 << (j % 32)),
            None => allowed = [u32::MAX; 8],
        }
        allowed[0] &= !1;
        let last = (1..=u8::MAX)
            .rev()
            .find(|&j| (allowed[j as usize / 32] >> (j % 32)) & 1 == 1)
            .unwrap_or(0);
        let mut diffs = Vec::with_capacity(f.len());
        if let Some(constant) = f.first() {
            diffs.push(constant.non_normal());
        }
        if f.len() > 1 {
            diffs.push(f.iter().fold(Point::zero(), |sum, a| g!(sum + a)));
        }
        let mut table = Self {
            poly: f.to_vec(),
            allowed,
            last,
            diffs,
            differenced: false,
            stepped: 0,
            images: Vec::new(),
        };
        table.images.reserve_exact(table.allowed_count());
        table
    }

    fn allowed_count(&self) -> usize {
        self.allowed.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Whether `index` is one of the table's indices.
    pub fn allows(&self, index: u8) -> bool {
        (self.allowed[index as usize / 32] >> (index % 32)) & 1 == 1
    }

    /// One step of the build. `true` once every allowed index is listed.
    pub fn build(&mut self) -> bool {
        if self.is_complete() {
            return true;
        }
        let degree = self.poly.len().saturating_sub(1);
        if self.diffs.len() <= degree {
            let x = Scalar::<Public, Zero>::from(self.diffs.len() as u32);
            self.diffs.push(poly::point::eval(&self.poly, x));
            return false;
        }
        if !self.differenced {
            // diffs[k] = Δ^k F(0).
            for k in 1..=degree {
                for i in (k..=degree).rev() {
                    self.diffs[i] = g!(self.diffs[i] - self.diffs[i - 1]);
                }
            }
            self.differenced = true;
        }
        let indices = (TABLE_ADDS_PER_STEP / degree.max(1)).max(1);
        for _ in 0..indices {
            if self.is_complete() {
                break;
            }
            for k in 0..degree {
                self.diffs[k] = g!(self.diffs[k] + self.diffs[k + 1]);
            }
            self.stepped += 1;
            if self.allows(self.stepped) {
                self.images.push(self.diffs[0].normalize().to_bytes());
            }
        }
        self.is_complete()
    }

    pub fn is_complete(&self) -> bool {
        self.stepped == self.last
    }

    /// The target, once every allowed index is listed.
    pub fn target(&self) -> Option<Target<'_>> {
        self.is_complete().then_some(Target::Indexed(self))
    }

    /// Whether `image` is F(index), for an allowed index already listed.
    pub fn find(&self, image: &Point<Normal, Public, impl ZeroChoice>, index: u8) -> bool {
        if !self.allows(index) || index > self.stepped {
            return false;
        }
        // The index's place among the allowed ones.
        let below = (0..index).filter(|&j| self.allows(j)).count();
        self.images
            .get(below)
            .is_some_and(|listed| *listed == image.to_bytes())
    }
}

fn share_index(index: u8) -> Option<ShareIndex> {
    Scalar::<Public, Zero>::from(index).non_zero()
}

/// The mask checks of a decoded card. Without a target the codec only reports them; with one,
/// failed self masks are [`DecodeError::MasksFailed`], so `self_ok` is always true. The
/// device's restore takes a card only when they pass ([`MaskCheck::passed`], owner B7). Its
/// `Debug` leaves out the counter, a function of s.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MaskCheck {
    /// Some counter's self-checking masks m1..m24 are the card's: the 264-bit check that
    /// confirms the card is this format, written by an honest device.
    pub self_ok: bool,
    /// The first such counter.
    pub ctr: Option<u8>,
    /// With F: the wallet-bound masks m25..m36 at that counter are the card's, so the card was
    /// written for this F. `None` without F.
    pub wallet: Option<bool>,
}

impl MaskCheck {
    /// The masks that apply passed: the self-checking ones, and with F the wallet-bound ones.
    /// A phone half chosen to decode to another share fails them but with probability 2^-264,
    /// so a restore whose phone half came from a phone needs this before anything decoded
    /// leaves the chip or is loaded (owner B7, spec PK.8).
    pub fn passed(&self) -> bool {
        self.self_ok && self.wallet != Some(false)
    }
}

impl core::fmt::Debug for MaskCheck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MaskCheck")
            .field("self_ok", &self.self_ok)
            .field("wallet", &self.wallet)
            .finish_non_exhaustive()
    }
}

/// The canonical check (warn-only), with F: whether the card is what an honest encoder writes
/// for the decoded share under F, re-encoding it with the card's own threshold field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalCheck {
    /// All 36 masks are the share's under F, at the counter the self masks found.
    pub masks: bool,
    /// The threshold word reads: the top 3 bits of word XOR μ are 0.
    pub threshold_word: bool,
    /// h0..h23 are the share's as an encoder writes them: the scalar was written below n.
    pub hidden: bool,
    /// The self masks' counter is the smallest that gives 64 distinct words.
    pub counter: bool,
    /// The card's threshold is F's (its length), or 0 (not recorded). False if it doesn't read.
    pub threshold: bool,
}

impl CanonicalCheck {
    pub fn is_canonical(&self) -> bool {
        self.masks && self.threshold_word && self.hidden && self.counter && self.threshold
    }
}

/// The first counter whose self masks are the card's, trying all 256 in constant time, so the
/// work is the same for a card, whatever its counter, and for words that aren't one (owner
/// R2-1; spec PK.8 step 3 asks this of the counters below [`FIXED_COUNTERS`] at least).
fn find_counter(hasher: &Sha256, values: &[u16; COEFFS], work: &mut Work) -> Option<u8> {
    let matches = |ctr: u8| {
        let masks = self_masks_from(hasher, ctr);
        let diff = SELF_MASK_SLOTS
            .iter()
            .zip(masks)
            .fold(0, |diff, (&slot, m)| diff | (values[slot] ^ m));
        is_zero(diff as u32)
    };
    let (mut found, mut counter) = (0u16, 0u8);
    for ctr in 0..=u8::MAX {
        let take = matches(ctr) & !found & 1;
        counter ^= take.wrapping_neg() as u8 & (counter ^ ctr);
        found |= take;
        work.counters += 1;
    }
    (found == 1).then_some(counter)
}

/// The canonical check. `masks` is the mask check's verdict under F: the self masks found a
/// counter and the wallet masks match at it.
#[allow(clippy::too_many_arguments)]
fn canonical_check(
    scalar: Scalar<Secret, Zero>,
    index: u8,
    threshold_field: u16,
    threshold: Option<u8>,
    poly: &Poly,
    values: &[u16; COEFFS],
    counter: Option<u8>,
    masks: bool,
) -> CanonicalCheck {
    let threshold_ok = threshold.is_some_and(|t| t as usize == poly.len() || t == 0);
    let threshold_word = threshold.is_some();
    // The words, and so the smallest counter with distinct words, depend on the threshold
    // word as written, so the re-encode takes it whole.
    let card = EncodeJob::<TableFree>::new_raw(scalar, index, threshold_field, poly)
        .and_then(EncodeJob::run);
    let (hidden, canonical_counter) = match &card {
        // Constant time: the re-encoded h0..h23 against the decoded ones, with no early exit on
        // the first that differs (crypto-core review nit 2).
        Ok(card) => (
            is_zero(
                card.values[FIRST_SHARE_SLOT..]
                    .iter()
                    .zip(&values[FIRST_SHARE_SLOT..])
                    .fold(0, |diff, (&a, &b)| diff | (a ^ b)) as u32,
            ) == 1,
            Some(card.counter),
        ),
        Err(_) => (false, None),
    };
    CanonicalCheck {
        masks,
        threshold_word,
        hidden,
        counter: canonical_counter.is_some() && counter == canonical_counter,
        threshold: threshold_ok,
    }
}

/// A decoded card. Its `Debug` shows the share's index but not its scalar.
#[derive(Clone, PartialEq)]
pub struct Decoded {
    /// The share, at the card's own index.
    pub share: SecretShare,
    /// The index as written on the card. It's public and proof-bound, so it's the share's.
    pub index: u8,
    /// The card's threshold: `Some(0)` if not recorded, and `None` if the threshold word
    /// doesn't read (a lying chip, or another format). Unproven, so only warned about.
    pub threshold: Option<u8>,
    pub codeword: Codeword,
    /// The scalar was written as an integer of at least n and was reduced.
    pub reduced: bool,
    /// s·G is in the target. Without a target the result is unverified, and only the combine
    /// step may use it.
    pub verified: bool,
    /// The mask checks, which the device's restore requires ([`MaskCheck::passed`]).
    pub masks: MaskCheck,
    /// The canonical check, when F is known. It's warn-only: a mismatch never blocks a restore
    /// by itself.
    pub canonical: Option<CanonicalCheck>,
}

impl core::fmt::Debug for Decoded {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Decoded")
            .field("index", &self.index)
            .field("threshold", &self.threshold)
            .field("codeword", &self.codeword)
            .field("reduced", &self.reduced)
            .field("verified", &self.verified)
            .field("masks", &self.masks)
            .field("canonical", &self.canonical)
            .finish_non_exhaustive()
    }
}

/// Unique decoding: the codeword within `2e + f <= 3` of the entries, if there is one. It
/// rebuilds from the readable entries and, failing that, erases each one in turn.
pub fn decode_unique(entries: &Entries) -> Result<Codeword, DecodeError> {
    decode_unique_with::<TableFree>(entries)
}

/// [`decode_unique`] with the given arithmetic.
pub fn decode_unique_with<F: Field>(entries: &Entries) -> Result<Codeword, DecodeError> {
    check_words(entries)?;
    let erased = erasures(entries);
    let f = erased.count_ones() as usize;
    if f > PARITY {
        return Err(DecodeError::TooManyErasures);
    }
    let mut rebuilder = Rebuilder::<F>::new();
    for e in 0..=(PARITY - f) / 2 {
        let fits = each_subset(!erased, e, |wrong| {
            match rebuilder.rebuild(entries, erased | wrong) {
                Some(coeffs) => ControlFlow::Break(coeffs),
                None => ControlFlow::Continue(()),
            }
        });
        if let Some(coeffs) = fits {
            return Ok(Codeword::new(entries, coeffs));
        }
    }
    Err(DecodeError::Uncorrectable)
}

/// Reads a codeword: the hidden values (the index and s), the self-checking masks, then the
/// check against the target, the threshold word and the wallet-bound masks.
///
/// `recorded_format` is the card's format if a record of it exists (the app's pinned
/// format_id): any but [`FORMAT_ID`] is [`DecodeError::UnknownFormat`]. With no record, or the
/// right one, the card is read as C′: a card of another format shows up only as failed masks.
/// Without a target the codec reports them, and the device's restore refuses on them (spec
/// PK.8, owner B7). With a target, failed self masks are [`DecodeError::MasksFailed`], and the
/// target sees a stand-in's s·G in place of the codeword's, so nothing the device does depends
/// on s·G against a target the phone chose for a codeword that isn't the card (secfix-attack
/// B1). The work is the same whatever the codeword (owner R2-1): every hash, s·G and the
/// re-encode run whether the masks, the target or a zero scalar or index would have ended the
/// read early.
pub fn read(
    codeword: &Codeword,
    target: Option<&Target>,
    recorded_format: Option<u8>,
) -> Result<Decoded, DecodeError> {
    read_counted(codeword, target, recorded_format, &mut Work::default())
}

/// [`read`], counting its work.
fn read_counted(
    codeword: &Codeword,
    target: Option<&Target>,
    recorded_format: Option<u8>,
    work: &mut Work,
) -> Result<Decoded, DecodeError> {
    if let Some(format) = recorded_format.filter(|&f| f != FORMAT_ID) {
        return Err(DecodeError::UnknownFormat(format));
    }
    let mut hidden = codeword.hidden_values();
    let result = read_hidden(codeword, &hidden, target, MasksFirst::Yes, work);
    wipe(&mut hidden, [0; HIDDEN]);
    result
}

/// Whether a read against a target checks the self masks before s·G. Every restore does (owner
/// B7, secfix-attack B1); only the offline repair tool, with all 64 words on one trusted
/// machine and no phone, may skip them (spec PK.8 escape hatch, owner B13).
#[derive(Clone, Copy, PartialEq, Eq)]
enum MasksFirst {
    Yes,
    #[cfg_attr(not(feature = "offline-repair"), allow(dead_code))]
    No,
}

fn read_hidden(
    codeword: &Codeword,
    hidden: &[u16; HIDDEN],
    target: Option<&Target>,
    masks_first: MasksFirst,
    work: &mut Work,
) -> Result<Decoded, DecodeError> {
    work.reads += 1;
    let mut values = [0u16; COEFFS];
    values[FIRST_SHARE_SLOT..].copy_from_slice(&hidden[RANDOM_HIDDEN..]);
    let (index, mut bytes) = layout::unpack(&values);
    let reduced = Scalar::<Secret, Zero>::from_bytes(bytes).is_none();
    let scalar = Scalar::<Secret, Zero>::from_bytes_mod_order(bytes);
    wipe(&mut bytes, [0; 32]);
    wipe(&mut values, [0; COEFFS]);
    let refused = match share_index(index) {
        _ if scalar.is_zero() => Some(DecodeError::ZeroScalar),
        None => Some(DecodeError::ZeroIndex),
        Some(_) => None,
    };
    // A read with the masks first does the same work whatever the codeword (owner R2-1): a
    // zero scalar or index is refused only at the end, and until then stand-ins take their
    // place. Only the offline tool, on one trusted machine, stops early.
    if let (Some(e), MasksFirst::No) = (refused, masks_first) {
        return Err(e);
    }
    let (scalar, index) = match refused {
        Some(_) => (Scalar::<Secret, NonZero>::one().mark_zero(), 1),
        None => (scalar, index),
    };
    let mut values = values_from(&codeword.coeffs, hidden);
    let l = layout::layout_hash();
    // The offline tool ignores the masks, so the target alone decides, before the mask search.
    if let (Some(target), MasksFirst::No) = (target, masks_first) {
        if let Err(e) = target.check(scalar, index) {
            wipe(&mut values, [0; COEFFS]);
            return Err(e);
        }
    }
    let mut hasher = self_hasher(&l, scalar, index);
    let counter = find_counter(&hasher, &values, work);
    wipe_hasher(&mut hasher);
    // With a target, the self masks come first, and a codeword they don't confirm never meets
    // the target. The target is the phone's, and s·G against it would make the device's work
    // (a repair search or not) depend on a bit of s the phone picked (secfix-attack B1). So
    // an unconfirmed codeword's s·G is a stand-in's against the target, whose result is
    // dropped: the work is the same either way, and nothing after it depends on it.
    let checked = match (target, masks_first) {
        (Some(target), MasksFirst::Yes) => {
            work.images += 1;
            Some(match counter {
                Some(_) => target.check(scalar, index),
                None => {
                    let stand_in = Scalar::<Secret, NonZero>::one().mark_zero();
                    let _ = core::hint::black_box(target.check(stand_in, 1));
                    Err(DecodeError::MasksFailed)
                }
            })
        }
        _ => None,
    };
    let threshold_field = values[THRESHOLD_SLOT] ^ threshold_mask_with(&l, scalar);
    let threshold = (threshold_field >> 8 == 0).then_some(threshold_field as u8);
    let poly = target.and_then(Target::poly);
    // Both run whatever the masks and the target said, so that a read's work doesn't show
    // either.
    let wallet = poly.map(|poly| {
        work.wallet_hashes += 1;
        let mut hasher = wallet_hasher(&l, scalar, index, poly);
        let mut masks = wallet_masks_from(&hasher, counter.unwrap_or(0));
        wipe_hasher(&mut hasher);
        // Constant time: the card's words 53..64 against the expected masks, with no early
        // exit on the first that differs (crypto-core review nit 2).
        let diff = values[WALLET_MASK_SLOTS]
            .iter()
            .zip(&masks)
            .fold(0, |diff, (&v, &m)| diff | (v ^ m));
        wipe(&mut masks, [0; WALLET_MASKS]);
        counter.is_some() & (is_zero(diff as u32) == 1)
    });
    let canonical = poly.map(|poly| {
        work.encodes += 1;
        canonical_check(
            scalar,
            index,
            threshold_field,
            threshold,
            poly,
            &values,
            counter,
            wallet == Some(true),
        )
    });
    wipe(&mut values, [0; COEFFS]);
    if let Some(e) = refused {
        return Err(e);
    }
    if let Some(Err(e)) = checked {
        return Err(e);
    }
    let share_index = share_index(index).expect("refused above");
    Ok(Decoded {
        share: SecretShare {
            index: share_index,
            share: scalar,
        },
        index,
        threshold,
        codeword: codeword.clone(),
        reduced,
        verified: target.is_some(),
        masks: MaskCheck {
            self_ok: counter.is_some(),
            ctr: counter,
            wallet,
        },
        canonical,
    })
}

/// [`decode_unique`] then [`read`]. If this fails and there's a target, a repair goes on to
/// [`repair_search`]. The device's restore uses [`RestoreSearch`] instead, which does both as
/// constant work.
pub fn decode(
    entries: &Entries,
    target: Option<&Target>,
    recorded_format: Option<u8>,
) -> Result<Decoded, DecodeError> {
    read(&decode_unique(entries)?, target, recorded_format)
}

/// The numbers of wrong words a restore's repair tries, in order, when `unreadable` words
/// couldn't be read: from just past unique decoding up to what the spare words and
/// [`MAX_REPAIR_WRONG`] allow. Empty when nothing is left to try.
pub fn repair_range(unreadable: usize) -> RangeInclusive<usize> {
    let room = PARITY.saturating_sub(unreadable);
    room / 2 + 1..=room.min(MAX_REPAIR_WRONG)
}

/// What a repair search did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RepairStats {
    /// Sets of words tried as the wrong words, each costing a prefilter check.
    pub subsets: u64,
    /// Sets the prefilter passed (their corrected index is one the target allows), each costing
    /// a full rebuild.
    pub rebuilds: u64,
    /// Rebuilds the spare entries agreed with (every one, when none is spare).
    pub fits: u64,
    /// Candidates whose index the target allows, each costing a read and an s·G.
    pub images: u64,
}

/// The work a restore's decode did, counted where it's done, so a test can check that every
/// slice does the same whatever the words (owner R2-1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    /// Prefilter checks, each solving a 3×3 system.
    pub checks: u64,
    /// Table-free multiplications in them.
    pub check_muls: u64,
    /// Rebuilds of a codeword from 61 entries.
    pub rebuilds: u64,
    /// Words a rebuild evaluated to check it.
    pub spare_evals: u64,
    /// Reads of a codeword.
    pub reads: u64,
    /// Mask counters a read hashed.
    pub counters: u64,
    /// s·G against a target.
    pub images: u64,
    /// Wallet-bound mask hashes.
    pub wallet_hashes: u64,
    /// Re-encodes for the canonical check.
    pub encodes: u64,
    /// Table operations on public points: the prefilter's 3×3 inverses and the rebuilds'
    /// bases. They have no branch on a value, so each reads the tables whatever the operands
    /// (review B15 secrecy S-1).
    pub table_ops: u64,
}

impl Work {
    /// The work done since `earlier`.
    pub fn since(&self, earlier: &Work) -> Work {
        Work {
            checks: self.checks - earlier.checks,
            check_muls: self.check_muls - earlier.check_muls,
            rebuilds: self.rebuilds - earlier.rebuilds,
            spare_evals: self.spare_evals - earlier.spare_evals,
            reads: self.reads - earlier.reads,
            counters: self.counters - earlier.counters,
            images: self.images - earlier.images,
            wallet_hashes: self.wallet_hashes - earlier.wallet_hashes,
            encodes: self.encodes - earlier.encodes,
            table_ops: self.table_ops - earlier.table_ops,
        }
    }
}

// A search returns one outcome, so the size of `Found` costs nothing worth a box.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum RepairOutcome {
    Found(Decoded),
    /// Every set of words was tried.
    Exhausted,
    /// `stop` ended the search.
    Stopped,
}

/// Tries every set of `wrong` readable words, in lexicographic order, as the wrong words, with
/// the unreadable words erased too. Only the self masks and then s·G in the target ([`read`])
/// accept a candidate, so the first candidate the masks confirm is the card; the syndromes
/// and the corrected index filter first. `wrong` plus the unreadable words can be at most 3, so
/// every set gives one candidate.
///
/// `stop` is polled after each set. The search returns at the first hit: the spec's random
/// order, fixed workload and two-hit check are the caller's. Unlike [`RepairJob`] it takes any
/// `wrong` up to 3, for tests and benches.
pub fn repair_search(
    entries: &Entries,
    wrong: usize,
    target: &Target,
    stats: &mut RepairStats,
    stop: &mut impl FnMut(&RepairStats) -> bool,
) -> Result<RepairOutcome, DecodeError> {
    repair_search_with::<TableFree>(entries, wrong, target, stats, stop)
}

/// [`repair_search`] with the given arithmetic.
pub fn repair_search_with<F: Field>(
    entries: &Entries,
    wrong: usize,
    target: &Target,
    stats: &mut RepairStats,
    stop: &mut impl FnMut(&RepairStats) -> bool,
) -> Result<RepairOutcome, DecodeError> {
    let mut search = Search::<F>::new(entries, wrong)?;
    loop {
        if let Some(outcome) = search.try_next(target, stats) {
            return Ok(outcome);
        }
        if stop(stats) {
            return Ok(RepairOutcome::Stopped);
        }
    }
}

/// The syndrome prefilter (design appendix A): for a set Q of wrong or unreadable words, it
/// solves the error values on Q from the syndromes, checks any syndrome left over, and gives the
/// index of the corrected word, in O(|Q|²) instead of a rebuild's O(k²).
///
/// A codeword c satisfies Σ_j v_j·α_j^m·c_j = 0 for m = 0..3, with v_j = 1/Π_{i≠j}(α_j − α_i)
/// (the dual of an evaluation code is one with these column multipliers). So the received
/// word y, with unreadable words read as 0, has syndromes S_m = Σ_{p∈Q} v_p·α_p^m·e_p when it
/// differs from c only on Q. h0 = f(2024) is a linear function of the codeword, λ·c, with λ_j
/// = ℓ_j(2024) over words 1-61, so the corrected h0 is λ·y + Σ_{p∈Q} λ_p·e_p, and its top 8
/// bits are the index.
///
/// It never drops a set the full rebuild would accept: such a set's codeword differs from y
/// only on Q, so its errors solve the syndromes and give its h0. Every set it passes still
/// gets the full rebuild, the spare check and s·G, so a bug here can only miss a repair.
struct Prefilter {
    /// v_j·α_j^m for each word j: the parity checks. Public.
    checks: Vec<[u16; PARITY]>,
    /// λ_j: h0 = Σ_j λ_j·c_j on codewords, 0 for words 62-64. Public.
    split: Vec<u16>,
    /// The received word's syndromes. Secret.
    syndromes: [u16; PARITY],
    /// λ·y. Secret.
    split_received: u16,
}

impl Prefilter {
    fn new<F: Field>(entries: &Entries) -> Self {
        // The points, the checks and λ are public, so the tables build them.
        let checks: Vec<[u16; PARITY]> = (0..WORDS)
            .map(|j| {
                let x = layout::word_point(j);
                let product = (0..WORDS)
                    .filter(|&i| i != j)
                    .fold(1, |acc, i| gf::mul(acc, x ^ layout::word_point(i)));
                let mut row = [gf::inv(product); PARITY];
                for m in 1..PARITY {
                    row[m] = gf::mul(row[m - 1], x);
                }
                row
            })
            .collect();
        // ℓ_j(x_h0) over words 1-61: Π_{i≠j}(x_h0 − x_i)/(x_j − x_i).
        let h0 = layout::hidden_point(RANDOM_HIDDEN);
        let split: Vec<u16> = (0..WORDS)
            .map(|j| {
                if j >= COEFFS {
                    return 0;
                }
                let x = layout::word_point(j);
                let (num, den) = (0..COEFFS).filter(|&i| i != j).fold((1, 1), |(n, d), i| {
                    let xi = layout::word_point(i);
                    (gf::mul(n, h0 ^ xi), gf::mul(d, x ^ xi))
                });
                gf::mul(num, gf::inv(den))
            })
            .collect();
        let mut syndromes = [0u16; PARITY];
        let mut split_received = 0;
        for (j, entry) in entries.iter().enumerate() {
            let y = entry.unwrap_or(0);
            for (s, &h) in syndromes.iter_mut().zip(&checks[j]) {
                *s ^= F::mul(y, h);
            }
            split_received ^= F::mul(y, split[j]);
        }
        Self {
            checks,
            split,
            syndromes,
            split_received,
        }
    }

    /// Whether some error values on the words in `q` explain the syndromes, and if so the
    /// index (the top 8 bits) of the corrected h0. `q` has at most 3 words. The work is the
    /// same for every `q` (owner R2-1): the system is always solved as 3×3, a missing word
    /// standing in as an identity row whose error is then dropped, and every syndrome is
    /// checked, the solved ones masked out.
    fn check<F: Field>(&self, q: WordMask, work: &mut Work) -> Option<u8> {
        // The first 3 words of `q`, with no branch on which they are (review B15 secrecy S-1).
        let mut positions = [0usize; PARITY];
        let mut n = 0;
        for j in 0..WORDS {
            let take = ((q >> j) as usize & 1) & (n < PARITY) as usize;
            let slot = n - (n == PARITY) as usize;
            positions[slot] ^= (positions[slot] ^ j) & take.wrapping_neg();
            n += take;
        }
        let mut muls = 0;
        let mut mul = |a: u16, b: u16| {
            muls += 1;
            F::mul(a, b)
        };
        // Solve Σ_p checks[p][m]·e_p = S_m for m < n. The matrix is public (a Vandermonde
        // matrix times the nonzero v_p), so it's inverted with the tables. Padded with the
        // identity, its inverse is the n×n inverse padded with the identity. The inverse does
        // the same table work for every matrix, the identity of a stand-in included, so a
        // check costs the same whichever words it leaves out (review B15 secrecy S-1). The
        // addresses it reads follow the (public) matrix: a stand-in's identity reads the same
        // few table entries, a real check's spread over the tables. On the device the tables
        // sit in flash behind the cache, but every slice's rebuild reads across both tables
        // (thousands of reads over their 12 KB), so a search slice and a stand-in slice touch
        // the same table lines; what's left is the order of cache misses within a slice. The
        // device bench's shortest and longest slice is the check (spec PK-O26).
        let inverse = invert(
            |m, i| {
                let real = (((m < n) & (i < n)) as u16).wrapping_neg();
                (self.checks[positions[i]][m] & real) | ((m == i) as u16 & !real)
            },
            &mut work.table_ops,
        );
        let mut errors = [0u16; PARITY];
        for (i, e) in errors.iter_mut().enumerate() {
            let solved = (0..PARITY).fold(0, |e, m| e ^ mul(inverse[i][m], self.syndromes[m]));
            *e = solved & ((i < n) as u16).wrapping_neg();
        }
        // Any syndrome left over must agree; every one is checked, with no early exit. An
        // error of 0 at a padding position adds nothing.
        let mut misfit = 0;
        for m in 0..PARITY {
            let implied =
                (0..PARITY).fold(0, |s, i| s ^ mul(self.checks[positions[i]][m], errors[i]));
            misfit |= (implied ^ self.syndromes[m]) & ((m >= n) as u16).wrapping_neg();
        }
        let h0 = (0..PARITY).fold(self.split_received, |c, i| {
            c ^ mul(self.split[positions[i]], errors[i])
        });
        wipe(&mut errors, [0; PARITY]);
        work.checks += 1;
        work.check_muls += muls;
        (misfit == 0).then_some(layout::index_of_h0(h0))
    }
}

impl Drop for Prefilter {
    fn drop(&mut self) {
        wipe(&mut self.syndromes, [0; PARITY]);
        wipe(&mut self.split_received, 0);
    }
}

/// The inverse of the public 3×3 matrix with entries `entry(row, column)`, which must be
/// invertible: its adjugate over its determinant (in characteristic 2 the cofactors have no
/// signs). Unlike elimination it has no pivot to search for and no branch on a value, so it
/// does the same table work for every matrix (review B15 secrecy S-1). `ops` counts it.
fn invert(entry: impl Fn(usize, usize) -> u16, ops: &mut u64) -> [[u16; PARITY]; PARITY] {
    const _: () = assert!(PARITY == 3);
    let a: [[u16; PARITY]; PARITY] =
        core::array::from_fn(|r| core::array::from_fn(|c| entry(r, c)));
    let others = |k: usize| ((k + 1) % PARITY, (k + 2) % PARITY);
    // cofactor[r][c]: the determinant of `a` without row r and column c.
    let cofactor: [[u16; PARITY]; PARITY] = core::array::from_fn(|r| {
        let (r1, r2) = others(r);
        core::array::from_fn(|c| {
            let (c1, c2) = others(c);
            gf::mul_ct(a[r1][c1], a[r2][c2]) ^ gf::mul_ct(a[r1][c2], a[r2][c1])
        })
    });
    let det = (0..PARITY).fold(0, |d, c| d ^ gf::mul_ct(a[0][c], cofactor[0][c]));
    let scale = gf::inv_ct(det);
    *ops += 2 * 9 + 3 + 1 + 9;
    core::array::from_fn(|i| core::array::from_fn(|j| gf::mul_ct(cofactor[j][i], scale)))
}

/// A repair search's state between sets of words.
struct Search<F> {
    entries: Entries,
    erased: WordMask,
    guesses: Subsets,
    rebuilder: Rebuilder<F>,
    /// `None` only in the test oracle, which rebuilds every set.
    prefilter: Option<Prefilter>,
}

impl<F: Field> Search<F> {
    fn new(entries: &Entries, wrong: usize) -> Result<Self, DecodeError> {
        let mut search = Self::without_prefilter(entries, wrong)?;
        search.prefilter = Some(Prefilter::new::<F>(entries));
        Ok(search)
    }

    /// The full-rebuild search, which the tests use as the oracle for the prefilter.
    fn without_prefilter(entries: &Entries, wrong: usize) -> Result<Self, DecodeError> {
        check_words(entries)?;
        let erased = erasures(entries);
        match wrong.checked_add(erased.count_ones() as usize) {
            Some(m) if m <= PARITY => {}
            _ => return Err(DecodeError::TooManyErasures),
        }
        Ok(Self {
            entries: *entries,
            erased,
            guesses: Subsets::new(!erased, wrong),
            rebuilder: Rebuilder::new(),
            prefilter: None,
        })
    }

    /// Tries the next set: `Found` at a hit, `Exhausted` once every set has been tried, and
    /// `None` to go on.
    fn try_next(&mut self, target: &Target, stats: &mut RepairStats) -> Option<RepairOutcome> {
        // No format record: a repair never has a different one to hand.
        self.try_next_by(
            |index| target.allows(index),
            |entries, coeffs| read(&Codeword::new(entries, coeffs), Some(target), None).ok(),
            stats,
        )
    }

    /// [`try_next`](Self::try_next) with the index filter and the acceptance as parameters:
    /// `allows` filters candidates by their corrected index, and `accept` reads a candidate
    /// (the entries and its coefficients) and decides whether it's the card.
    fn try_next_by(
        &mut self,
        allows: impl Fn(u8) -> bool,
        accept: impl FnOnce(&Entries, [u16; COEFFS]) -> Option<Decoded>,
        stats: &mut RepairStats,
    ) -> Option<RepairOutcome> {
        let Some(guess) = self.guesses.next() else {
            return Some(RepairOutcome::Exhausted);
        };
        stats.subsets += 1;
        if let Some(prefilter) = &self.prefilter {
            // Branching on the result is hidden by the restore's fixed decode time.
            if !prefilter
                .check::<F>(self.erased | guess, &mut Work::default())
                .is_some_and(&allows)
            {
                return None;
            }
        }
        stats.rebuilds += 1;
        let entries = &self.entries;
        if let Some(coeffs) = self.rebuilder.rebuild(entries, self.erased | guess) {
            stats.fits += 1;
            let h0 = eval_hidden(&coeffs, RANDOM_HIDDEN);
            if allows(layout::index_of_h0(h0)) {
                stats.images += 1;
                if let Some(decoded) = accept(entries, coeffs) {
                    return Some(RepairOutcome::Found(decoded));
                }
            }
        }
        None
    }
}

impl<F> Drop for Search<F> {
    fn drop(&mut self) {
        wipe(&mut self.entries, [None; WORDS]);
        for c in &mut self.rebuilder.coeffs {
            wipe(c, 0);
        }
    }
}

/// [`repair_search`] in steps a device can run between UI polls: each [`step`](Self::step)
/// tries up to `max_subsets` sets of words. It holds the entries, so it wipes them when it's
/// dropped and its `Debug` shows only its stats.
pub struct RepairJob {
    search: Search<TableFree>,
    stats: RepairStats,
}

impl RepairJob {
    /// A search of every set of `wrong` readable words, refusing what [`repair_search`]
    /// refuses and more than [`MAX_REPAIR_WRONG`] wrong words.
    pub fn new(entries: &Entries, wrong: usize) -> Result<Self, DecodeError> {
        if wrong > MAX_REPAIR_WRONG {
            return Err(DecodeError::TooManyErasures);
        }
        Ok(Self {
            search: Search::new(entries, wrong)?,
            stats: RepairStats::default(),
        })
    }

    /// The same search without the prefilter, rebuilding every set: the oracle the prefilter
    /// is tested against. It takes any `wrong` up to 3.
    #[cfg(test)]
    fn oracle(entries: &Entries, wrong: usize) -> Result<Self, DecodeError> {
        Ok(Self {
            search: Search::without_prefilter(entries, wrong)?,
            stats: RepairStats::default(),
        })
    }

    /// [`new`](Self::new) without the cap, for tests that raise it.
    #[cfg(test)]
    fn uncapped(entries: &Entries, wrong: usize) -> Result<Self, DecodeError> {
        Ok(Self {
            search: Search::new(entries, wrong)?,
            stats: RepairStats::default(),
        })
    }

    /// Tries up to `max_subsets` more sets (at least 1), stopping early after a candidate's
    /// s·G, so a step has at most one. `None` means there are more to try. After `Found` a
    /// further step goes on from the next set.
    pub fn step(&mut self, target: &Target, max_subsets: u32) -> Option<RepairOutcome> {
        let images = self.stats.images;
        for _ in 0..max_subsets.max(1) {
            let outcome = self.search.try_next(target, &mut self.stats);
            if outcome.is_some() || self.stats.images > images {
                return outcome;
            }
        }
        None
    }

    pub fn stats(&self) -> &RepairStats {
        &self.stats
    }
}

impl core::fmt::Debug for RepairJob {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RepairJob")
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

/// Sets of words one [`RestoreSearch::slice`] puts through the prefilter, real or stand-in.
pub const SEARCH_SUBSETS_PER_SLICE: usize = 64;
/// Sets the prefilter passed that a [`RestoreSearch`] holds until its reads reach them. With
/// the queue full, a slice's checks are stand-ins.
const SEARCH_QUEUE: usize = 16;

/// A share index set, as the app sends the wallet's: the indices a repair candidate may have.
/// `None` allows every index but 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexSet([u32; 8]);

impl IndexSet {
    pub fn new(indices: Option<&[u8]>) -> Self {
        let mut allowed = [0u32; 8];
        match indices {
            Some(indices) => indices
                .iter()
                .for_each(|&j| allowed[j as usize / 32] |= 1 << (j % 32)),
            None => allowed = [u32::MAX; 8],
        }
        allowed[0] &= !1;
        Self(allowed)
    }

    pub fn allows(&self, index: u8) -> bool {
        (self.0[index as usize / 32] >> (index % 32)) & 1 == 1
    }
}

/// Whether a set of words is tried within unique decoding or as a repair against F.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchStage {
    Unique,
    Repair,
}

/// How a [`RestoreSearch`] ended.
// It's returned once per decode, so the size of `Read` costs nothing worth a box.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SearchEnd {
    /// The entries can't be searched: more than 3 are unreadable
    /// ([`DecodeError::TooManyErasures`]) or one isn't a word ([`DecodeError::WordRange`]).
    Refused(DecodeError),
    /// No codeword the self masks confirmed was found.
    NotFound,
    /// The codeword the self masks confirmed, read in full against F when there is one, and
    /// the stage it was found in.
    Read(SearchStage, Result<Decoded, DecodeError>),
}

/// A device restore's decode as constant work (owner R2-1): unique decoding, then with F a
/// repair of up to [`MAX_REPAIR_WRONG`] wrong words, in slices that each do the same work
/// whatever the words, and a [`finish`](Self::finish) that does the same work whether a card
/// was found or not.
///
/// Each [`slice`](Self::slice) puts [`SEARCH_SUBSETS_PER_SLICE`] sets of words through the
/// syndrome prefilter, then reads one codeword: a rebuild and the self masks. A set the
/// prefilter passes waits in a queue for a later slice's read. Once there's nothing left to
/// check, or the queue is full, or a card was found, the checks are stand-ins, and once there's
/// nothing to read, the read is of the entries as they are. So no slice ends early, a slice
/// with the card in it costs what any other does, and the work goes on after the search is
/// over, until the caller stops. Unique decoding uses the prefilter too (it's exact while a
/// syndrome is spare, so it passes the sets a rebuild would fit), in the same order as
/// [`decode_unique`]. The search never meets F: a repair candidate is filtered by its
/// corrected index (the phone's index set, which is public) and confirmed by the self masks,
/// and only [`finish`](Self::finish) checks s·G, once, against F.
pub struct RestoreSearch {
    /// The entries, or stand-ins when they can't be searched.
    entries: Entries,
    erased: WordMask,
    prefilter: Prefilter,
    rebuilder: Rebuilder<TableFree>,
    refused: Option<DecodeError>,
    /// The numbers of wrong words to try, in order, with their stage.
    levels: Vec<(usize, SearchStage)>,
    level: usize,
    guesses: Option<Subsets>,
    /// With F: the index set a repair candidate must be in. Without F there's no repair.
    repair: Option<IndexSet>,
    queue: [(WordMask, SearchStage); SEARCH_QUEUE],
    queued: core::ops::Range<usize>,
    /// A unique decode was read, so no more sets within unique decoding are tried.
    unique_done: bool,
    /// The codeword the self masks confirmed.
    found: Option<([u16; COEFFS], SearchStage)>,
    finished: bool,
    work: Work,
}

impl RestoreSearch {
    /// The search of `entries`. With `repair` (when the app sent F) it goes past unique
    /// decoding.
    pub fn new(entries: &Entries, repair: Option<IndexSet>) -> Self {
        let f = erasures(entries).count_ones() as usize;
        let refused = if f > PARITY {
            Some(DecodeError::TooManyErasures)
        } else {
            check_words(entries).err()
        };
        // Entries that can't be searched are replaced by stand-ins, which the slices work on.
        let entries = match refused {
            Some(_) => [Some(0); WORDS],
            None => *entries,
        };
        let erased = erasures(&entries);
        let mut levels = Vec::new();
        if refused.is_none() {
            levels.extend((0..=(PARITY - f) / 2).map(|e| (e, SearchStage::Unique)));
            if repair.is_some() {
                levels.extend(repair_range(f).map(|e| (e, SearchStage::Repair)));
            }
        }
        Self {
            prefilter: Prefilter::new::<TableFree>(&entries),
            entries,
            erased,
            rebuilder: Rebuilder::new(),
            refused,
            levels,
            level: 0,
            guesses: None,
            repair,
            queue: [(0, SearchStage::Unique); SEARCH_QUEUE],
            queued: 0..0,
            unique_done: false,
            found: None,
            finished: false,
            work: Work::default(),
        }
    }

    /// The next set to check, or `None` when a slice's check should be a stand-in.
    fn next_guess(&mut self) -> Option<(WordMask, SearchStage)> {
        if self.finished || self.found.is_some() || self.queued.len() == SEARCH_QUEUE {
            return None;
        }
        loop {
            let &(wrong, stage) = self.levels.get(self.level)?;
            if !(stage == SearchStage::Unique && self.unique_done) {
                let erased = self.erased;
                let guesses = self
                    .guesses
                    .get_or_insert_with(|| Subsets::new(!erased, wrong));
                if let Some(guess) = guesses.next() {
                    return Some((guess, stage));
                }
            }
            self.level += 1;
            self.guesses = None;
        }
    }

    /// The next queued set worth reading.
    fn next_queued(&mut self) -> Option<(WordMask, SearchStage)> {
        if self.finished || self.found.is_some() {
            return None;
        }
        for at in self.queued.by_ref() {
            let (guess, stage) = self.queue[at % SEARCH_QUEUE];
            if !(stage == SearchStage::Unique && self.unique_done) {
                return Some((guess, stage));
            }
        }
        None
    }

    /// One slice: [`SEARCH_SUBSETS_PER_SLICE`] prefilter checks and one read, real or not.
    pub fn slice(&mut self) {
        for _ in 0..SEARCH_SUBSETS_PER_SLICE {
            match self.next_guess() {
                Some((guess, stage)) => {
                    let index = self
                        .prefilter
                        .check::<TableFree>(self.erased | guess, &mut self.work);
                    let passes = match (stage, &self.repair) {
                        (SearchStage::Unique, _) => index.is_some(),
                        (SearchStage::Repair, Some(set)) => index.is_some_and(|i| set.allows(i)),
                        (SearchStage::Repair, None) => false,
                    };
                    if passes {
                        self.queue[self.queued.end % SEARCH_QUEUE] = (guess, stage);
                        self.queued.end += 1;
                    }
                }
                None => {
                    core::hint::black_box(self.prefilter.check::<TableFree>(0, &mut self.work));
                }
            }
        }
        self.read_one();
    }

    /// Rebuilds and reads one queued set, or the entries as they are when none waits.
    fn read_one(&mut self) {
        let candidate = self.next_queued();
        let guess = candidate.map_or(0, |(guess, _)| guess);
        let (coeffs, fits) =
            self.rebuilder
                .rebuild_any(&self.entries, self.erased | guess, &mut self.work);
        // Only the masks and the index are wanted, so the words aren't compared with the
        // entries: the finish does that for the report.
        let codeword = Codeword {
            coeffs,
            corrected: 0,
            erased: self.erased,
            no_spare_check: false,
        };
        let read = read_counted(&codeword, None, None, &mut self.work);
        let Codeword { mut coeffs, .. } = codeword;
        let (confirmed, index) = match read {
            Ok(mut decoded) => {
                let out = (decoded.masks.self_ok, decoded.index);
                wipe_decoded(&mut decoded);
                out
            }
            Err(_) => (false, 0),
        };
        match candidate {
            Some((_, SearchStage::Unique)) if fits => {
                // The unique decode: the masks decide, and no other set within unique
                // decoding can fit.
                self.unique_done = true;
                if confirmed {
                    self.found = Some((coeffs, SearchStage::Unique));
                }
            }
            Some((_, SearchStage::Repair))
                if fits && confirmed && self.repair.is_some_and(|set| set.allows(index)) =>
            {
                self.found = Some((coeffs, SearchStage::Repair));
            }
            _ => {}
        }
        wipe(&mut coeffs, [0; COEFFS]);
    }

    /// Whether the search has nothing left to do: a card was found, or every set was tried.
    pub fn is_done(&self) -> bool {
        self.found.is_some()
            || self.refused.is_some()
            || (self.level >= self.levels.len() && self.queued.is_empty())
    }

    /// Ends the search: reads the codeword the self masks confirmed in full, against F when
    /// there is one (s·G, the wallet-bound masks, the canonical check), or a stand-in with
    /// the same work when there's none. After it, slices are all stand-ins. With F, s·G is
    /// checked against F evaluated at the card's index, or at 1 for a stand-in: the index is
    /// public, and the evaluation costs about the same.
    pub fn finish(&mut self, poly: Option<&Poly>) -> SearchEnd {
        self.finished = true;
        // The stand-in is the codeword through the entries as they are, rebuilt whether it's
        // used or not: values that look like a card's, as a constant of zeros wouldn't (the
        // host's compiler gives table-free multiplication by 0 a shortcut).
        let (mut stand_in, _) =
            self.rebuilder
                .rebuild_any(&self.entries, self.erased, &mut self.work);
        let (mut coeffs, stage) = match self.found.take() {
            Some((coeffs, stage)) => (coeffs, Some(stage)),
            None => (stand_in, None),
        };
        wipe(&mut stand_in, [0; COEFFS]);
        let target = poly.map(Target::Polynomial);
        let read = read_counted(
            &Codeword::new(&self.entries, coeffs),
            target.as_ref(),
            None,
            &mut self.work,
        );
        wipe(&mut coeffs, [0; COEFFS]);
        match (self.refused, stage) {
            (Some(refused), _) => {
                if let Ok(mut decoded) = read {
                    wipe_decoded(&mut decoded);
                }
                SearchEnd::Refused(refused)
            }
            (None, Some(stage)) => SearchEnd::Read(stage, read),
            (None, None) => {
                if let Ok(mut decoded) = read {
                    wipe_decoded(&mut decoded);
                }
                SearchEnd::NotFound
            }
        }
    }

    /// The work done so far.
    pub fn work(&self) -> &Work {
        &self.work
    }
}

impl Drop for RestoreSearch {
    fn drop(&mut self) {
        wipe(&mut self.entries, [None; WORDS]);
        if let Some((coeffs, _)) = self.found.as_mut() {
            wipe(coeffs, [0; COEFFS]);
        }
        for c in &mut self.rebuilder.coeffs {
            wipe(c, 0);
        }
    }
}

impl core::fmt::Debug for RestoreSearch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestoreSearch")
            .field("work", &self.work)
            .finish_non_exhaustive()
    }
}

/// The offline repair tool's read (spec PK.8 escape hatch, owner B13): [`read`] with the masks
/// reported but never required. With F it checks s·G = F(i) at the card's own index and
/// reports the wallet-bound masks and the canonical check; without F it's [`read`] without a
/// target. Only for all 64 words typed on one trusted machine: a restore whose words 33–64 came
/// from a phone must require the masks, which are all that stops the phone steering the share
/// (owner B7).
#[cfg(feature = "offline-repair")]
pub fn read_ignoring_masks(
    codeword: &Codeword,
    poly: Option<&Poly>,
) -> Result<Decoded, DecodeError> {
    let target = poly.map(Target::Polynomial);
    let mut hidden = codeword.hidden_values();
    let result = read_hidden(
        codeword,
        &hidden,
        target.as_ref(),
        MasksFirst::No,
        &mut Work::default(),
    );
    wipe(&mut hidden, [0; HIDDEN]);
    result
}

/// What the offline repair search accepts a candidate on.
#[cfg(feature = "offline-repair")]
#[derive(Clone, Copy, Debug)]
pub enum OfflineCheck<'a> {
    /// s·G = F(i) at the candidate's own index, whatever its masks say. A wrong candidate passes
    /// with probability about 2^-256.
    Polynomial(&'a Poly),
    /// The 24 self-checking masks pass, so with no target only a card an honest device wrote
    /// can be repaired past unique decoding. A wrong candidate passes with probability 2^-264.
    SelfMasks,
}

/// The index and scalar a candidate's coefficients hold, evaluated with the tables: the
/// offline search's quick look before a full [`read`].
#[cfg(feature = "offline-repair")]
fn quick_share(coeffs: &[u16; COEFFS]) -> (u8, Scalar<Secret, Zero>) {
    let mut values = [0u16; COEFFS];
    for m in RANDOM_HIDDEN..HIDDEN {
        values[FIRST_SHARE_SLOT + m - RANDOM_HIDDEN] =
            Tables::eval(coeffs, layout::hidden_point(m));
    }
    let (index, mut bytes) = layout::unpack(&values);
    let scalar = Scalar::<Secret, Zero>::from_bytes_mod_order(bytes);
    wipe(&mut bytes, [0; 32]);
    wipe(&mut values, [0; COEFFS]);
    (index, scalar)
}

/// Whether a candidate's self-checking masks pass, from only the values they cover,
/// evaluated with the tables: the offline search's quick look before a full [`read`].
#[cfg(feature = "offline-repair")]
fn quick_self_masks(coeffs: &[u16; COEFFS], layout_hash: &[u8; 32]) -> bool {
    let (index, mut scalar) = quick_share(coeffs);
    let mut values = [0u16; COEFFS];
    for &slot in SELF_MASK_SLOTS.iter() {
        let x = match layout::hidden_index(slot) {
            Some(m) => layout::hidden_point(m),
            None => layout::word_point(HALF + slot - layout::FIRST_PHONE_SLOT),
        };
        values[slot] = Tables::eval(coeffs, x);
    }
    // Variable time, unlike [`find_counter`]: a counter whose first block's 23 masks miss is
    // dropped without its second block.
    let mut hasher = self_hasher(layout_hash, scalar, index);
    let found = (0..=u8::MAX).any(|ctr| {
        let mut first: [u8; 32] = hasher.clone().chain_update([ctr, 0]).finalize().into();
        let head: [u16; SELF_MASKS - 1] = eleven_bit_values(&first);
        wipe(&mut first, [0; 32]);
        SELF_MASK_SLOTS
            .iter()
            .zip(head)
            .all(|(&slot, m)| values[slot] == m)
            && SELF_MASK_SLOTS
                .iter()
                .zip(self_masks_from(&hasher, ctr))
                .all(|(&slot, m)| values[slot] == m)
    });
    wipe_hasher(&mut hasher);
    wipe(&mut scalar, Scalar::zero());
    wipe(&mut values, [0; COEFFS]);
    found
}

/// Overwrites a decoded share and its codeword.
pub fn wipe_decoded(decoded: &mut Decoded) {
    wipe(&mut decoded.share.share, Scalar::zero());
    wipe(&mut decoded.codeword.coeffs, [0; COEFFS]);
    wipe(&mut decoded.masks.ctr, None);
}

/// The offline repair tool's search: every set of `wrong` readable words, with the unreadable
/// words erased too, tried as the wrong words, and every distinct codeword that `check`
/// accepts. Unlike [`repair_search`] it never stops at a hit, so a caller can refuse an
/// ambiguous card, and it never needs the masks with a target. Any index but 0 is tried: the
/// tool has no index set. `wrong` plus the unreadable words can be at most 3.
#[cfg(feature = "offline-repair")]
pub fn offline_repair_search(
    entries: &Entries,
    wrong: usize,
    check: OfflineCheck,
    stats: &mut RepairStats,
) -> Result<Vec<Decoded>, DecodeError> {
    // The machine is trusted and offline, so the search uses the tables on secret values
    // (spec §2.6 forbids them on the device), and looks at each candidate's share, and for
    // the masks its self-mask values, before it builds the codeword and reads it in full.
    let mut search = Search::<Tables>::new(entries, wrong)?;
    let layout_hash = layout::layout_hash();
    // F at every index, so checking a candidate costs its s·G and a lookup.
    let table = match check {
        OfflineCheck::Polynomial(poly) => {
            let mut table = ImageTable::new(poly, None);
            while !table.build() {}
            Some(table)
        }
        OfflineCheck::SelfMasks => None,
    };
    let target = table.as_ref().and_then(ImageTable::target);
    let mut found: Vec<Decoded> = Vec::new();
    loop {
        let accept = |entries: &Entries, mut coeffs: [u16; COEFFS]| {
            let decoded = match (check, &target) {
                (OfflineCheck::Polynomial(poly), Some(target)) => {
                    let (index, mut scalar) = quick_share(&coeffs);
                    let holds = target.check(scalar, index).is_ok();
                    wipe(&mut scalar, Scalar::zero());
                    holds
                        .then(|| read_ignoring_masks(&Codeword::new(entries, coeffs), Some(poly)))
                        .and_then(Result::ok)
                }
                (OfflineCheck::SelfMasks, _) => quick_self_masks(&coeffs, &layout_hash)
                    .then(|| read(&Codeword::new(entries, coeffs), None, None))
                    .and_then(Result::ok)
                    .filter(|decoded| decoded.masks.self_ok),
                (OfflineCheck::Polynomial(_), None) => unreachable!("F has its table"),
            };
            wipe(&mut coeffs, [0; COEFFS]);
            decoded
        };
        match search.try_next_by(|index| index != 0, accept, stats) {
            None => {}
            Some(RepairOutcome::Found(mut decoded)) => {
                // Sets that hold every wrong word and some right ones give the same codeword.
                if found
                    .iter()
                    .any(|seen| seen.codeword.coeffs == decoded.codeword.coeffs)
                {
                    wipe_decoded(&mut decoded);
                } else {
                    found.push(decoded);
                }
            }
            Some(RepairOutcome::Exhausted | RepairOutcome::Stopped) => return Ok(found),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        reveal_card::card::{eval, interpolate},
        Fingerprint, ShareBackup,
    };
    use layout::BASIS;
    use rand::{seq::SliceRandom, Rng, RngCore};
    use schnorr_fun::frost::SharedKey;

    struct Wallet {
        shared_key: SharedKey,
        shares: Vec<SecretShare>,
    }

    impl Wallet {
        fn new(threshold: usize, n: usize, rng: &mut impl RngCore) -> Self {
            let (backups, shared_key) = ShareBackup::generate_shares(
                Scalar::random(rng),
                threshold,
                n,
                Fingerprint::NONE,
                rng,
            );
            let shares = backups
                .into_iter()
                .map(|backup| backup.extract_secret(&shared_key).unwrap())
                .collect();
            Self { shared_key, shares }
        }

        fn poly(&self) -> &Poly {
            self.shared_key.point_polynomial()
        }

        fn record(&self, share: &SecretShare) -> Target<'static> {
            Target::Share {
                image: share.share_image(),
                threshold: self.shared_key.threshold(),
            }
        }
    }

    fn byte(index: ShareIndex) -> u8 {
        share_index_byte(index).unwrap()
    }

    /// The card with these values, its words from an interpolation through [`BASIS`]: not the
    /// encoder's Lagrange rows.
    fn card_from_values(values: [u16; COEFFS]) -> Card {
        let coeffs = interpolate(&BASIS, &values);
        Card {
            values,
            words: core::array::from_fn(|j| eval(&coeffs, (j + 1) as u16)),
            counter: 0,
        }
    }

    /// m1..m36 at a counter, through the public mask functions.
    fn all_masks(scalar: Scalar<Secret, Zero>, index: u8, poly: &Poly, ctr: u8) -> Vec<u16> {
        self_masks(scalar, index, ctr)
            .into_iter()
            .chain(wallet_masks(scalar, index, poly, ctr))
            .collect()
    }

    /// Replaces the words at `boxes` with other words.
    fn corrupt(entries: &mut Entries, boxes: &[usize], rng: &mut impl Rng) {
        for &j in boxes {
            let old = entries[j].unwrap();
            while entries[j] == Some(old) {
                entries[j] = Some(rng.gen_range(0..2048));
            }
        }
    }

    fn random_words(count: usize, rng: &mut impl Rng) -> Vec<usize> {
        let all: Vec<usize> = (0..WORDS).collect();
        all.choose_multiple(rng, count).copied().collect()
    }

    fn mask(boxes: &[usize]) -> WordMask {
        boxes.iter().fold(0, |mask, &j| mask | 1 << j)
    }

    fn found(outcome: RepairOutcome) -> Decoded {
        match outcome {
            RepairOutcome::Found(decoded) => decoded,
            outcome => panic!("{outcome:?}"),
        }
    }

    #[test]
    fn table_free_arithmetic_matches_the_tables() {
        for a in 0..gf::ORDER as u16 {
            for b in 0..gf::ORDER as u16 {
                assert_eq!(TableFree::mul(a, b), Tables::mul(a, b), "{a} * {b}");
            }
        }
        let mut rng = rand::thread_rng();
        let coeffs: Vec<u16> = (0..COEFFS).map(|_| rng.gen_range(0..2048)).collect();
        let xs: Vec<u16> = (0..COEFFS).map(layout::word_point).collect();
        let mut basis = Basis::new(&xs);
        let mut column = vec![0; COEFFS];
        let (mut tables, mut table_free) = (vec![0; COEFFS], vec![0; COEFFS]);
        for (i, &y) in coeffs.iter().enumerate() {
            assert_eq!(TableFree::eval(&coeffs, xs[i]), eval(&coeffs, xs[i]));
            Tables::add_scaled(&mut basis, i, y, &mut tables, &mut column);
            TableFree::add_scaled(&mut basis, i, y, &mut table_free, &mut column);
        }
        assert_eq!(tables, table_free);
    }

    #[test]
    fn both_arithmetics_decode_and_repair_alike() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        assert_eq!(
            encode_with::<Tables>(share, &wallet.shared_key),
            encode(share, &wallet.shared_key)
        );
        let card = encode(share, &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        entries[3] = None;
        corrupt(&mut entries, &[9], &mut rng);
        assert_eq!(
            decode_unique_with::<Tables>(&entries),
            decode_unique(&entries)
        );
        corrupt(&mut entries, &[30], &mut rng);
        let target = wallet.record(share);
        let mut stats = [RepairStats::default(); 2];
        let outcomes = [
            repair_search_with::<Tables>(&entries, 2, &target, &mut stats[0], &mut |_| false),
            repair_search(&entries, 2, &target, &mut stats[1], &mut |_| false),
        ];
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(stats[0], stats[1]);
        assert_eq!(found(outcomes[1].clone().unwrap()).share, *share);
    }

    #[test]
    fn entries_that_arent_words_are_refused() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let card = encode(&wallet.shares[0], &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        entries[5] = Some(2048);
        assert_eq!(decode_unique(&entries), Err(DecodeError::WordRange));
        assert_eq!(
            decode_unique_with::<Tables>(&entries),
            Err(DecodeError::WordRange)
        );
        let mut stats = RepairStats::default();
        assert_eq!(
            repair_search(
                &entries,
                1,
                &wallet.record(&wallet.shares[0]),
                &mut stats,
                &mut |_| false
            ),
            Err(DecodeError::WordRange)
        );
        entries[5] = Some(u16::MAX);
        assert_eq!(decode(&entries, None, None), Err(DecodeError::WordRange));
    }

    #[test]
    fn encode_lays_out_the_share() {
        let mut rng = rand::thread_rng();
        for (t, n) in [(2, 3), (3, 5)] {
            let wallet = Wallet::new(t, n, &mut rng);
            for share in &wallet.shares {
                let card = encode(share, &wallet.shared_key).unwrap();
                assert_eq!(encode(share, &wallet.shared_key).unwrap(), card);
                let index = byte(share.index);
                // h0..h23 = u8(i) ‖ ser32(s).
                assert_eq!(
                    layout::unpack(&card.values),
                    (index, share.share.to_bytes())
                );
                // The threshold word at x = 2021.
                assert_eq!(
                    card.values[THRESHOLD_SLOT] ^ threshold_mask(share.share),
                    t as u16
                );
                // m1..m36 in their slots, at the card's counter.
                assert_eq!(
                    MASK_SLOTS.map(|k| card.values[k]).to_vec(),
                    all_masks(share.share, index, wallet.poly(), card.counter)
                );
                // The words are f's values, by an independent interpolation through the 61
                // slot points, and the phone half is P.
                assert_eq!(card.words, card_from_values(card.values).words);
                assert_eq!(
                    card.phone_half()[..],
                    card.values[layout::FIRST_PHONE_SLOT..FIRST_SHARE_SLOT]
                );
                assert_eq!(distinct(&card.words), 1);
            }
        }
    }

    /// The spec's fixed inputs (§2.15): s = h2s(SHA256(label)), i = 3, t = 2, and
    /// F = [a0·G, a1·G] with F(3) = s·G.
    fn spec_inputs(label: &[u8]) -> (SecretShare, SharedKey<Normal, Zero>) {
        let s = Scalar::from_bytes_mod_order(Sha256::digest(label).into());
        let a0 = Scalar::<Secret, Zero>::from_bytes_mod_order(
            Sha256::digest(b"frostsnap card tv1 a0").into(),
        );
        let index = share_index(3).unwrap();
        let i_inv = index.invert();
        let a1 = s!((s - a0) * i_inv);
        let f = vec![g!(a0 * G).normalize(), g!(a1 * G).normalize()];
        let share = SecretShare { index, share: s };
        (share, SharedKey::from_poly(f))
    }

    fn hex(bytes: &[u8]) -> alloc::string::String {
        bytes.iter().map(|b| alloc::format!("{b:02x}")).collect()
    }

    /// ser(F) of the spec inputs, which the Python reference takes as public input.
    const SPEC_SER_F: &str = "02c55cab9ac3dd3445a6248b63dd6f56cd6441c5837c23a6db64bb8a225b51ca4a03b581d4d32076486d9fdb69254ca4d1f5a78b3767d866c357afac399e9dfbf624";

    #[test]
    fn matches_the_python_reference() {
        // From `frost_backup/tests/ref/cprime_ref.py` (run it with no arguments), a separate
        // implementation written from the spec text (PK.1, PK.2, with the hidden random values
        // of owner B6). Not frozen vectors: the encoding is draft (spec PK-O3).
        let (share, key) = spec_inputs(b"frostsnap card tv1 s");
        let ser_f: Vec<u8> = key
            .point_polynomial()
            .iter()
            .flat_map(|p| p.to_bytes())
            .collect();
        assert_eq!(hex(&ser_f), SPEC_SER_F);
        let card = encode(&share, &key).unwrap();
        assert_eq!(
            hex(&layout::layout_hash()),
            "39eb840dc376839013945b7d34da64c315d5e2a487e049ff2f0fa118db305977"
        );
        assert_eq!((threshold_mask(share.share), card.counter), (1135, 1));
        assert_eq!(
            card.values,
            [
                1217, 200, 1133, 1385, 562, 166, 932, 2005, 1522, 1345, 1169, 1951, 913, 573, 262,
                939, 872, 739, 915, 894, 554, 189, 1691, 42, 20, 937, 282, 26, 1214, 1546, 419,
                716, 544, 1434, 1501, 1863, 398, 24, 293, 1574, 1445, 1220, 198, 1051, 323, 504,
                1993, 1387, 771, 709, 464, 1562, 1538, 78, 1428, 1850, 844, 1630, 606, 101, 332
            ]
        );
        assert_eq!(
            card.words,
            [
                996, 810, 215, 445, 1933, 1032, 667, 1367, 77, 1103, 1798, 859, 1720, 912, 527,
                1651, 882, 1982, 194, 177, 1641, 1313, 128, 702, 151, 1955, 48, 649, 148, 84, 276,
                1291, 166, 932, 2005, 1522, 1345, 1169, 1951, 913, 573, 262, 939, 872, 739, 915,
                894, 554, 189, 1691, 42, 20, 937, 282, 26, 1214, 1546, 419, 716, 544, 1434, 1501,
                1863, 398
            ]
        );
        // The first s_k = h2s(SHA256("frostsnap card tv1 s" ‖ BE32(k))) needing counters 1 and
        // 2, under the same i, t and F (F needn't match s_k to define an encoding).
        for (k, counter, words) in [
            (
                2u32,
                1,
                [
                    376, 1664, 1625, 135, 1019, 1290, 561, 1875, 1325, 873, 868, 674, 1731, 958,
                    1906, 1772, 13, 1449, 1233, 1320, 1826, 707, 29, 754, 411, 83, 1936, 308, 1475,
                    461, 1943, 1047, 977, 1944, 410, 110, 1654, 151, 1392, 653, 972, 1131, 1361,
                    512, 499, 1370, 1332, 1269, 1893, 175, 1374, 535, 945, 78, 1218, 1376, 1046,
                    399, 1590, 997, 651, 664, 1277, 24,
                ],
            ),
            (
                4,
                2,
                [
                    256, 1162, 1944, 247, 1696, 1287, 612, 1594, 1208, 973, 19, 1855, 1083, 243,
                    1042, 657, 1914, 1846, 127, 110, 125, 483, 965, 1863, 142, 1744, 1175, 1423,
                    1684, 793, 344, 1686, 211, 2002, 1721, 858, 1339, 1851, 508, 1048, 220, 306,
                    1183, 1576, 1468, 936, 1679, 641, 140, 1189, 745, 709, 1972, 1601, 395, 1156,
                    802, 455, 653, 1455, 1812, 1346, 2044, 1118,
                ],
            ),
        ] {
            let mut label = b"frostsnap card tv1 s".to_vec();
            label.extend(k.to_be_bytes());
            let s = Scalar::from_bytes_mod_order(Sha256::digest(&label).into());
            let meta = Meta {
                threshold: 2,
                index: 3,
            };
            let card = encode_meta(s, meta, key.point_polynomial()).unwrap();
            assert_eq!((card.counter, card.words), (counter, words), "k = {k}");
        }
    }

    /// PK.11's pinned L, μ and masks on the spec inputs, at counters 1 (the card's) and 0, as
    /// the Python reference prints them.
    #[test]
    fn pinned_l_mu_masks() {
        let (share, key) = spec_inputs(b"frostsnap card tv1 s");
        let (s, poly) = (share.share, key.point_polynomial());
        // L by hand: TH over u8 len(FMT_C) ‖ FMT_C.
        let tag = Sha256::digest(layout::LAYOUT_TAG);
        let mut by_hand = Sha256::new();
        by_hand.update(tag);
        by_hand.update(tag);
        by_hand.update([layout::FMT_C.len() as u8]);
        by_hand.update(layout::FMT_C);
        let l: [u8; 32] = by_hand.finalize().into();
        assert_eq!(l, layout::layout_hash());
        assert_eq!(threshold_mask(s), 1135);
        assert_eq!(
            all_masks(s, 3, poly, 1),
            [
                1217, 200, 1385, 562, 166, 932, 2005, 1522, 1345, 1169, 1951, 913, 573, 262, 939,
                872, 739, 915, 894, 554, 189, 1691, 42, 20, 937, 282, 26, 1214, 1546, 419, 716,
                544, 1434, 1501, 1863, 398
            ]
        );
        // Counter 0's 64 words repeat one, so the card takes counter 1; its masks still pin the
        // counter byte's place in the hash.
        assert_eq!(
            all_masks(s, 3, poly, 0),
            [
                991, 1216, 1197, 22, 1699, 79, 1945, 538, 282, 1051, 1448, 1633, 1934, 1368, 659,
                1320, 1047, 2006, 1565, 284, 1692, 1300, 2013, 598, 215, 1547, 1162, 593, 489,
                1444, 731, 367, 1500, 786, 996, 374
            ]
        );
    }

    /// Crypto-core review nit 1: a mask hasher past L ‖ s is a function of (L, s), so a wiped
    /// one holds a fresh state, midstate and buffer both.
    #[test]
    fn a_wiped_mask_hasher_holds_nothing_of_s() {
        let (share, key) = spec_inputs(b"frostsnap card tv1 s");
        let l = layout::layout_hash();
        let fresh: [u8; 32] = Sha256::new().finalize().into();
        for mut hasher in [
            self_hasher(&l, share.share, 3),
            wallet_hasher(&l, share.share, 3, key.point_polynomial()),
        ] {
            wipe_hasher(&mut hasher);
            let after: [u8; 32] = hasher.finalize().into();
            assert_eq!(after, fresh);
        }
    }

    #[test]
    fn card_words_hash_is_th_of_the_words() {
        let (share, key) = spec_inputs(b"frostsnap card tv1 s");
        let card = encode(&share, &key).unwrap();
        let tag = Sha256::digest(WORDS_TAG);
        let mut by_hand = Sha256::new();
        by_hand.update(tag);
        by_hand.update(tag);
        by_hand.update(layout::layout_hash());
        for word in card.words {
            by_hand.update(word.to_be_bytes());
        }
        let expected: [u8; 32] = by_hand.finalize().into();
        assert_eq!(card_words_hash(&card.words), expected);
        let mut other = card.words;
        other[40] ^= 1;
        assert_ne!(card_words_hash(&other), expected);
    }

    /// Binding (2): every mask hash and μ name L, so a share under another layout descriptor
    /// gets other masks (spec PK.6; the 0-relation measurement is `red-secrecy/cross_format.txt`).
    #[test]
    fn masks_are_bound_to_the_layout() {
        let (share, key) = spec_inputs(b"frostsnap card tv1 s");
        let (s, poly) = (share.share, key.point_polynomial());
        let l = layout::layout_hash();
        // The same layout with its threshold word moved to x = 2022.
        let moved = core::str::from_utf8(layout::FMT_C)
            .unwrap()
            .replace("thr=2021", "thr=2022");
        let other = layout::layout_hash_of(moved.as_bytes());
        assert_ne!(l, other);
        assert_ne!(threshold_mask_with(&l, s), threshold_mask_with(&other, s));
        // Independent hashes: each 11-bit mask collides by chance with probability 1/2048, so
        // require the vectors to differ and nearly every mask to differ, not every one.
        for ctr in [0, 1, 7] {
            let ours = self_masks_from(&self_hasher(&l, s, 3), ctr);
            let theirs = self_masks_from(&self_hasher(&other, s, 3), ctr);
            assert_ne!(ours, theirs, "ctr {ctr}");
            let differ = ours.iter().zip(&theirs).filter(|(a, b)| a != b).count();
            assert!(
                differ >= SELF_MASKS - 3,
                "ctr {ctr}: {differ} self masks differ"
            );
            let ours = wallet_masks_from(&wallet_hasher(&l, s, 3, poly), ctr);
            let theirs = wallet_masks_from(&wallet_hasher(&other, s, 3, poly), ctr);
            assert_ne!(ours, theirs, "ctr {ctr}");
            let differ = ours.iter().zip(&theirs).filter(|(a, b)| a != b).count();
            assert!(
                differ >= WALLET_MASKS - 2,
                "ctr {ctr}: {differ} wallet masks differ"
            );
        }
        // The public functions use this firmware's L.
        assert_eq!(
            self_masks(s, 3, 0),
            self_masks_from(&self_hasher(&l, s, 3), 0)
        );
    }

    #[test]
    fn counter_is_the_first_with_distinct_words() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let meta = Meta {
            threshold: 2,
            index: byte(wallet.shares[0].index),
        };
        let mut seen = [0; 3];
        for _ in 0..400 {
            let scalar = Scalar::random(&mut rng).mark_zero();
            let card = encode_meta(scalar, meta, wallet.poly()).unwrap();
            assert_eq!(distinct(&card.words), 1);
            for ctr in 0..=card.counter {
                let mut values = card.values;
                for (&slot, m) in
                    MASK_SLOTS
                        .iter()
                        .zip(all_masks(scalar, meta.index, wallet.poly(), ctr))
                {
                    values[slot] = m;
                }
                let words = card_from_values(values).words;
                // Only the counter's masks give distinct words; every earlier counter repeats
                // one.
                assert_eq!(distinct(&words) == 1, ctr == card.counter);
            }
            seen[(card.counter as usize).min(2)] += 1;
            if seen[1] > 0 && seen[2] > 0 {
                break;
            }
        }
        // About 63% of cards need counter 1 or more, and 40% counter 2 or more.
        assert!(seen[1] > 0 && seen[2] > 0, "{seen:?}");
    }

    #[test]
    fn encode_refuses_metadata_out_of_range() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = SecretShare {
            index: Scalar::<Public, Zero>::from(256u32).non_zero().unwrap(),
            share: wallet.shares[0].share,
        };
        assert_eq!(
            encode(&share, &wallet.shared_key),
            Err(EncodeError::Meta(MetaError::MetadataRange))
        );
        let single = Wallet::new(1, 2, &mut rng);
        assert_eq!(
            encode(&single.shares[1], &single.shared_key),
            Err(EncodeError::Meta(MetaError::ThresholdOne))
        );
        assert!(encode(&single.shares[0], &single.shared_key).is_ok());
    }

    #[test]
    fn clean_cards_decode_with_and_without_a_target() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        for share in &wallet.shares {
            let card = encode(share, &wallet.shared_key).unwrap();
            let entries = card.words.map(Some);

            let decoded = decode(&entries, None, None).unwrap();
            assert_eq!(decoded.share, *share);
            assert_eq!(
                (decoded.index, decoded.threshold),
                (byte(share.index), Some(2))
            );
            assert_eq!(decoded.codeword.values(), card.values);
            assert_eq!(
                (decoded.codeword.corrected, decoded.codeword.erased),
                (0, 0)
            );
            assert!(!decoded.verified && !decoded.reduced && !decoded.codeword.no_spare_check);
            assert_eq!(
                decoded.masks,
                MaskCheck {
                    self_ok: true,
                    ctr: Some(card.counter),
                    wallet: None
                }
            );
            assert_eq!(decoded.canonical, None);

            let decoded = decode(&entries, Some(&wallet.record(share)), None).unwrap();
            assert_eq!(decoded.share, *share);
            assert!(decoded.verified);
            assert_eq!(decoded.canonical, None);

            let decoded = decode(&entries, Some(&Target::Polynomial(wallet.poly())), None).unwrap();
            assert_eq!(decoded.share, *share);
            assert_eq!(decoded.masks.wallet, Some(true));
            assert!(decoded.canonical.unwrap().is_canonical());

            // Another share's record rejects it.
            let other = wallet
                .shares
                .iter()
                .find(|s| s.index != share.index)
                .unwrap();
            assert_eq!(
                decode(&entries, Some(&wallet.record(other)), None),
                Err(DecodeError::NotInTarget)
            );
        }
    }

    #[test]
    fn one_wrong_word_anywhere_is_corrected() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[1];
        let card = encode(share, &wallet.shared_key).unwrap();
        for j in 0..WORDS {
            let mut entries = card.words.map(Some);
            corrupt(&mut entries, &[j], &mut rng);
            let decoded = decode(&entries, Some(&wallet.record(share)), None).unwrap();
            assert_eq!(decoded.share, *share);
            assert_eq!(decoded.codeword.corrected, 1 << j);
            assert!(!decoded.codeword.no_spare_check);
        }
    }

    #[test]
    fn erasures_are_filled() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();
        for f in 1..=PARITY {
            for _ in 0..10 {
                let boxes = random_words(f, &mut rng);
                let mut entries = card.words.map(Some);
                for &j in &boxes {
                    entries[j] = None;
                }
                let decoded = decode(&entries, None, None).unwrap();
                assert_eq!(decoded.share, *share);
                assert_eq!(decoded.codeword.erased, mask(&boxes));
                assert_eq!(decoded.codeword.corrected, 0);
                assert_eq!(decoded.codeword.no_spare_check, f == PARITY);
            }
        }
        // One wrong word and one erasure use up the redundancy too.
        let boxes = random_words(2, &mut rng);
        let mut entries = card.words.map(Some);
        entries[boxes[0]] = None;
        corrupt(&mut entries, &boxes[1..], &mut rng);
        let decoded = decode(&entries, None, None).unwrap();
        assert_eq!(decoded.share, *share);
        assert_eq!(decoded.codeword.corrected, 1 << boxes[1]);
        assert!(decoded.codeword.no_spare_check);
        // Four is too many.
        let mut entries = card.words.map(Some);
        for j in random_words(4, &mut rng) {
            entries[j] = None;
        }
        assert_eq!(
            decode(&entries, None, None),
            Err(DecodeError::TooManyErasures)
        );
    }

    #[test]
    fn two_wrong_words_fail_cleanly() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let card = encode(&wallet.shares[0], &wallet.shared_key).unwrap();
        for _ in 0..20 {
            let mut entries = card.words.map(Some);
            corrupt(&mut entries, &random_words(2, &mut rng), &mut rng);
            // No other codeword is within 1 of a word 2 from this one.
            assert_eq!(decode_unique(&entries), Err(DecodeError::Uncorrectable));
        }
        // An erasure and a wrong word with another erasure is past unique decoding too.
        let boxes = random_words(3, &mut rng);
        let mut entries = card.words.map(Some);
        entries[boxes[0]] = None;
        entries[boxes[1]] = None;
        corrupt(&mut entries, &boxes[2..], &mut rng);
        assert_eq!(decode_unique(&entries), Err(DecodeError::Uncorrectable));
    }

    /// A codeword of weight 4, 0 outside `boxes`. It's scaled so that h0 = f(2024) is 0 or 1:
    /// adding it to a card keeps the index and changes the scalar.
    fn weight_four(boxes: &[usize; 4]) -> [u16; WORDS] {
        let zeros: Vec<usize> = (0..WORDS).filter(|j| !boxes.contains(j)).collect();
        let xs: Vec<u16> = zeros
            .iter()
            .chain(&boxes[..1])
            .map(|&j| layout::word_point(j))
            .collect();
        let mut ys = vec![0u16; zeros.len()];
        ys.push(1);
        let coeffs = interpolate(&xs, &ys);
        // Every nonzero multiple has the same support.
        let scale = match eval(&coeffs, layout::hidden_point(RANDOM_HIDDEN)) {
            0 => 1,
            h0 => gf::inv(h0),
        };
        core::array::from_fn(|j| gf::mul(scale, eval(&coeffs, layout::word_point(j))))
    }

    #[test]
    fn a_hidden_second_slip_miscorrects_and_the_masks_catch_it() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[2];
        let card = encode(share, &wallet.shared_key).unwrap();
        // Two slips at a and b, with the box at e unreadable, put the entries one word from
        // the codeword card + δ.
        let boxes: [usize; 4] = random_words(4, &mut rng).try_into().unwrap();
        let [e, a, b, _] = boxes;
        let delta = weight_four(&boxes);
        let mut entries = card.words.map(Some);
        entries[e] = None;
        for j in [a, b] {
            entries[j] = Some(card.words[j] ^ delta[j]);
        }
        // Unique decoding "corrects" the fourth box instead, to a codeword with the same index.
        let wrong = decode_unique(&entries).unwrap();
        assert!(wrong.no_spare_check);
        assert_eq!(wrong.corrected, 1 << boxes[3]);
        assert_eq!(layout::index_of(&wrong.values()), byte(share.index));
        assert_ne!(layout::unpack(&wrong.values()).1, share.share.to_bytes());
        // The masks refuse it before its s·G meets the target; read cold, they report it.
        assert_eq!(
            read(&wrong, Some(&wallet.record(share)), None),
            Err(DecodeError::MasksFailed)
        );
        assert!(!read(&wrong, None, None).unwrap().masks.self_ok);
        let mut stats = RepairStats::default();
        let outcome = repair_search(&entries, 2, &wallet.record(share), &mut stats, &mut |_| {
            false
        })
        .unwrap();
        let decoded = found(outcome);
        assert_eq!(decoded.share, *share);
        assert_eq!(decoded.codeword.corrected, mask(&[a, b]));
    }

    #[test]
    fn recorded_format_2_is_unknown() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let entries = encode(share, &wallet.shared_key).unwrap().words.map(Some);
        let record = wallet.record(share);
        let poly = Target::Polynomial(wallet.poly());
        for target in [None, Some(&record), Some(&poly)] {
            assert_eq!(
                decode(&entries, target, Some(2)),
                Err(DecodeError::UnknownFormat(2))
            );
            assert_eq!(decode(&entries, target, Some(3)).unwrap().share, *share);
            assert_eq!(decode(&entries, target, None).unwrap().share, *share);
        }
        assert_eq!(FORMAT_ID, 3);
        assert_eq!(
            decode(&entries, None, Some(0)),
            Err(DecodeError::UnknownFormat(0))
        );
        // All ABANDON is the zero codeword: s = 0.
        assert_eq!(
            decode(&[Some(0); WORDS], None, None),
            Err(DecodeError::ZeroScalar)
        );
    }

    /// Spec PK.8: read cold, the codec reports failed masks and doesn't refuse on them, so an
    /// offline repair tool can ignore them. Against a target, failed self masks are
    /// [`DecodeError::MasksFailed`] before s·G is checked (secfix-attack B1), even when s·G is
    /// in the target. [`MaskCheck::passed`] is false for a cold read, and the device's restore
    /// refuses on it (owner B7).
    #[test]
    fn failed_masks_block_only_against_a_target() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[2];
        let card = encode(share, &wallet.shared_key).unwrap();
        let record = wallet.record(share);
        let poly = Target::Polynomial(wallet.poly());
        // A lying chip's self-checking mask: m1 (hidden, x = 2019) or m10 (word 38).
        for slot in [0, 10] {
            let mut values = card.values;
            values[slot] ^= 1;
            let liar = card_from_values(values).words.map(Some);
            for target in [&record, &poly] {
                assert_eq!(
                    decode(&liar, Some(target), None),
                    Err(DecodeError::MasksFailed)
                );
            }
            let decoded = decode(&liar, None, None).unwrap();
            assert_eq!(decoded.share, *share);
            assert!(!decoded.verified);
            assert_eq!(
                decoded.masks,
                MaskCheck {
                    self_ok: false,
                    ctr: None,
                    wallet: None
                }
            );
            assert!(!decoded.masks.passed());
        }
        // The honest card passes, with F or without.
        assert!(decode(&card.words.map(Some), Some(&poly), None)
            .unwrap()
            .masks
            .passed());
        assert!(decode(&card.words.map(Some), None, None)
            .unwrap()
            .masks
            .passed());
        // A wallet-bound mask (m30, word 58): the self check passes, the wallet check doesn't.
        let mut values = card.values;
        values[30] ^= 1;
        let liar = card_from_values(values).words.map(Some);
        let decoded = decode(&liar, Some(&poly), None).unwrap();
        assert_eq!(decoded.share, *share);
        assert_eq!(
            decoded.masks,
            MaskCheck {
                self_ok: true,
                ctr: Some(card.counter),
                wallet: Some(false)
            }
        );
        assert!(!decoded.canonical.unwrap().masks);
        assert!(
            !decoded.masks.passed(),
            "with F the wallet-bound masks apply"
        );
        assert!(decode(&liar, None, None).unwrap().masks.self_ok);
        assert!(decode(&liar, None, None).unwrap().masks.passed());
    }

    /// Key substitution: F' agrees with F at this share's index only. The share verifies; the
    /// wallet-bound masks give it away (spec PK.2).
    #[test]
    fn substituted_f_fails_wallet_masks() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[2];
        let card = encode(share, &wallet.shared_key).unwrap();
        let entries = card.words.map(Some);
        let a0 = Point::random(&mut rng).mark_zero();
        let image = share.share_image().image;
        let i_inv = share.index.invert();
        let a1 = g!(i_inv * (image - a0)).normalize();
        let substitute = [a0, a1];
        let decoded = decode(&entries, Some(&Target::Polynomial(&substitute)), None).unwrap();
        assert_eq!(decoded.share, *share);
        assert!(decoded.verified);
        assert_eq!(
            decoded.masks,
            MaskCheck {
                self_ok: true,
                ctr: Some(card.counter),
                wallet: Some(false)
            }
        );
        let check = decoded.canonical.unwrap();
        assert!(!check.masks && check.hidden && check.threshold && check.threshold_word);
        // The real F confirms it.
        let decoded = decode(&entries, Some(&Target::Polynomial(wallet.poly())), None).unwrap();
        assert_eq!(decoded.masks.wallet, Some(true));
    }

    /// The index is the card's own and proof-bound: a card written at another index is not the
    /// share, and nothing searches for one.
    #[test]
    fn a_wrong_index_is_not_in_target() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 5, &mut rng);
        let share = &wallet.shares[2];
        assert_eq!(byte(share.index), 3);
        let meta = Meta {
            threshold: 2,
            index: 39,
        };
        let card = encode_meta(share.share, meta, wallet.poly()).unwrap();
        let entries = card.words.map(Some);
        let poly = Target::Polynomial(wallet.poly());
        for target in [&poly, &wallet.record(share)] {
            assert_eq!(
                decode(&entries, Some(target), None),
                Err(DecodeError::NotInTarget)
            );
        }
        let mut table = ImageTable::new(wallet.poly(), None);
        while !table.build() {}
        assert_eq!(
            decode(&entries, Some(&table.target().unwrap()), None),
            Err(DecodeError::NotInTarget)
        );
        // Without a target it's read as written, and its masks are its own.
        let decoded = decode(&entries, None, None).unwrap();
        assert_eq!((decoded.share.share, decoded.index), (share.share, 39));
        assert_eq!(decoded.share.index, share_index(39).unwrap());
        assert!(decoded.masks.self_ok);
        // The index 0 is no share's.
        let zero = encode_meta(
            share.share,
            Meta {
                threshold: 2,
                index: 0,
            },
            wallet.poly(),
        )
        .unwrap();
        assert_eq!(
            decode(&zero.words.map(Some), None, None),
            Err(DecodeError::ZeroIndex)
        );
    }

    /// A restore from the cards alone (no record, no F) still confirms the format: the self
    /// masks need only what the card carries.
    #[test]
    fn cold_restore_passes_self_masks() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(3, 5, &mut rng);
        for share in &wallet.shares {
            let card = encode(share, &wallet.shared_key).unwrap();
            let mut entries = card.words.map(Some);
            let decoded = decode(&entries, None, None).unwrap();
            assert_eq!(decoded.share, *share);
            assert_eq!(decoded.threshold, Some(3));
            assert_eq!(
                decoded.masks,
                MaskCheck {
                    self_ok: true,
                    ctr: Some(card.counter),
                    wallet: None
                }
            );
            // And from a damaged card.
            for j in random_words(3, &mut rng) {
                entries[j] = None;
            }
            assert!(decode(&entries, None, None).unwrap().masks.self_ok);
        }
    }

    /// Spec PK.3: the threshold is warn-only. A card that says 3 for a 2-of-3 wallet decodes,
    /// verifies and repairs; only the canonical check flags it.
    #[test]
    fn threshold_mismatch_is_warn_only() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let meta = Meta {
            threshold: 3,
            index: byte(share.index),
        };
        let card = encode_meta(share.share, meta, wallet.poly()).unwrap();
        let mut entries = card.words.map(Some);
        let poly = Target::Polynomial(wallet.poly());
        let decoded = decode(&entries, Some(&poly), None).unwrap();
        assert_eq!(decoded.share, *share);
        assert!(decoded.verified);
        assert_eq!(decoded.threshold, Some(3));
        assert_eq!(decoded.masks.wallet, Some(true));
        let check = decoded.canonical.unwrap();
        assert!(check.masks && check.threshold_word && check.hidden && check.counter);
        assert!(!check.threshold && !check.is_canonical());
        let record = wallet.record(share);
        assert_eq!(
            decode(&entries, Some(&record), None).unwrap().threshold,
            Some(3)
        );
        // The repair path doesn't filter on it either.
        corrupt(&mut entries, &[5, 50], &mut rng);
        for target in [&record, &poly] {
            let mut stats = RepairStats::default();
            let decoded =
                found(repair_search(&entries, 2, target, &mut stats, &mut |_| false).unwrap());
            assert_eq!((decoded.share, decoded.threshold), (*share, Some(3)));
        }
    }

    #[test]
    fn a_garbled_threshold_word_still_decodes() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[1];
        let card = encode(share, &wallet.shared_key).unwrap();
        let mut values = card.values;
        values[THRESHOLD_SLOT] ^= 0x700;
        let entries = card_from_values(values).words.map(Some);
        let decoded = decode(&entries, Some(&Target::Polynomial(wallet.poly())), None).unwrap();
        assert_eq!(decoded.share, *share);
        assert!(decoded.verified);
        assert_eq!(decoded.threshold, None);
        // The masks don't hash the threshold, so they still check out.
        assert!(decoded.masks.self_ok && decoded.masks.wallet == Some(true));
        let check = decoded.canonical.unwrap();
        assert!(!check.threshold && !check.threshold_word && check.masks && check.hidden);
        let decoded = decode(&entries, Some(&wallet.record(share)), None).unwrap();
        assert_eq!((decoded.share, decoded.threshold), (*share, None));
        assert_eq!(decode(&entries, None, None).unwrap().threshold, None);
    }

    #[test]
    fn the_threshold_field_may_be_unrecorded() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let target = Target::Polynomial(wallet.poly());
        let index = byte(share.index);
        for (threshold, canonical) in [(0, true), (2, true), (3, false)] {
            let card = encode_meta(share.share, Meta { threshold, index }, wallet.poly()).unwrap();
            let decoded = decode(&card.words.map(Some), Some(&target), None).unwrap();
            assert_eq!(decoded.threshold, Some(threshold));
            let check = decoded.canonical.unwrap();
            assert!(check.masks && check.counter);
            assert_eq!(check.is_canonical(), canonical, "t_f = {threshold}");
        }
    }

    #[test]
    fn a_scalar_written_above_n_is_reduced() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let index = wallet.shares[0].index;
        let scalar = Scalar::<Secret, Zero>::from(5u32);
        let meta = Meta {
            threshold: 2,
            index: byte(index),
        };
        let card = encode_meta(scalar, meta, wallet.poly()).unwrap();
        // 5 + n, which still fits in 256 bits.
        let mut bytes = [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c,
            0xd0, 0x36, 0x41, 0x41,
        ];
        bytes[31] += 5;
        let mut values = card.values;
        layout::pack(meta.index, &bytes, &mut values);
        let entries = card_from_values(values).words.map(Some);
        let record = Target::Share {
            image: SecretShare {
                index,
                share: scalar,
            }
            .share_image(),
            threshold: 2,
        };
        let decoded = decode(&entries, Some(&record), None).unwrap();
        assert_eq!(decoded.share.share, scalar);
        assert!(decoded.reduced);
        let canonical = decode(&card.words.map(Some), Some(&record), None).unwrap();
        assert!(!canonical.reduced);
    }

    #[test]
    fn image_tables_list_the_allowed_indices() {
        let mut rng = rand::thread_rng();
        for t in [1, 2, 3, 4, 9] {
            let wallet = Wallet::new(t, 1, &mut rng);
            let image = |j: u8| wallet.shared_key.share_image(share_index(j).unwrap()).image;
            let mut table = ImageTable::new(wallet.poly(), None);
            let mut steps = 1;
            while !table.build() {
                assert!(table.target().is_none());
                steps += 1;
            }
            assert!(steps <= t + 255, "t = {t}: {steps} steps");
            let target = table.target().unwrap();
            assert_eq!(target.threshold(), t);
            for j in [1u8, 2, 3, 7, 100, 254, 255] {
                assert!(table.find(&image(j), j), "t = {t}, j = {j}");
                assert!(target.allows(j));
                if t > 1 {
                    // Another index's image: a direct comparison, no search.
                    let other = if j == 1 { 2 } else { 1 };
                    assert!(!table.find(&image(j), other), "t = {t}, j = {j}");
                }
            }
            assert!(!table.allows(0) && !target.allows(0));

            // The wallet's set: only its indices are listed, and the build stops at the
            // largest.
            let mut small = ImageTable::new(wallet.poly(), Some(&[3, 1, 0, 2, 2]));
            let mut steps = 1;
            while !small.build() {
                steps += 1;
            }
            assert!(steps <= t + 2, "t = {t}: {steps} steps");
            for j in [1u8, 2, 3] {
                assert!(small.find(&image(j), j));
            }
            assert!(!small.allows(0) && !small.allows(4) && !small.allows(7));
            assert!(!small.find(&image(7), 7));
            let target = small.target().unwrap();
            assert!(target.allows(2) && !target.allows(7));
        }
        // An empty set lists nothing and is complete at once.
        let wallet = Wallet::new(2, 3, &mut rng);
        let empty = ImageTable::new(wallet.poly(), Some(&[]));
        assert!(empty.is_complete());
        assert!(!empty.target().unwrap().allows(1));
    }

    /// The listed target decodes what the polynomial target does, canonical check included.
    #[test]
    fn indexed_target_decodes_as_the_polynomial() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(3, 4, &mut rng);
        let indices: Vec<u8> = wallet.shares.iter().map(|s| byte(s.index)).collect();
        let polynomial = Target::Polynomial(wallet.poly());
        for set in [None, Some(&indices[..])] {
            let mut table = ImageTable::new(wallet.poly(), set);
            while !table.build() {}
            let indexed = table.target().unwrap();
            for share in &wallet.shares {
                let card = encode(share, &wallet.shared_key).unwrap();
                let mut entries = card.words.map(Some);
                entries[5] = Some((card.words[5] + 1) % 2048);
                assert_eq!(
                    decode(&entries, Some(&indexed), None),
                    decode(&entries, Some(&polynomial), None)
                );
                assert!(decode(&entries, Some(&indexed), None)
                    .unwrap()
                    .canonical
                    .is_some());
                // Two wrong words: past unique decoding, the repair finds the same card.
                entries[9] = Some((card.words[9] + 1) % 2048);
                let first = |target: &Target| {
                    let mut job = RepairJob::new(&entries, 2).unwrap();
                    loop {
                        if let Some(outcome) = job.step(target, 40) {
                            break outcome;
                        }
                    }
                };
                let outcome = first(&indexed);
                assert_eq!(outcome, first(&polynomial));
                assert!(matches!(outcome, RepairOutcome::Found(found) if found.share == *share));
            }
            let other = Wallet::new(3, 1, &mut rng);
            let card = encode(&other.shares[0], &other.shared_key).unwrap();
            assert_eq!(
                decode(&card.words.map(Some), Some(&indexed), None),
                Err(DecodeError::NotInTarget)
            );
        }
    }

    /// A card whose index the set leaves out (its device's share was deleted from the wallet)
    /// still decodes against the listed F: the set filters repairs and never vetoes a unique
    /// decode. Another wallet's card at an unlisted index still fails.
    #[test]
    fn an_unlisted_index_is_checked_against_f() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let (dropped, kept) = wallet.shares.split_first().unwrap();
        let set: Vec<u8> = kept.iter().map(|s| byte(s.index)).collect();
        assert!(!set.contains(&byte(dropped.index)));
        let mut table = ImageTable::new(wallet.poly(), Some(&set));
        while !table.build() {}
        let indexed = table.target().unwrap();
        assert!(!indexed.allows(byte(dropped.index)));
        let polynomial = Target::Polynomial(wallet.poly());

        let card = encode(dropped, &wallet.shared_key).unwrap();
        let clean = card.words.map(Some);
        let mut one_wrong = clean;
        one_wrong[5] = Some((card.words[5] + 1) % 2048);
        let mut unreadable = clean;
        for j in [1, 40, 63] {
            unreadable[j] = None;
        }
        for entries in [clean, one_wrong, unreadable] {
            let decoded = decode(&entries, Some(&indexed), None).unwrap();
            assert_eq!(decoded.share, *dropped);
            assert!(decoded.verified);
            assert_eq!(decoded.masks.wallet, Some(true));
            assert_eq!(Ok(decoded), decode(&entries, Some(&polynomial), None));
        }

        // Past unique decoding the set does filter: the repair skips the unlisted index.
        let mut two_wrong = one_wrong;
        two_wrong[9] = Some((card.words[9] + 1) % 2048);
        let mut job = RepairJob::new(&two_wrong, 2).unwrap();
        let outcome = loop {
            if let Some(outcome) = job.step(&indexed, 64) {
                break outcome;
            }
        };
        assert_eq!(outcome, RepairOutcome::Exhausted);

        let other = Wallet::new(2, 1, &mut rng);
        let foreign = encode(&other.shares[0], &other.shared_key).unwrap();
        // Index 1, which only the dropped share had: the check evaluates F there.
        assert_eq!(byte(other.shares[0].index), byte(dropped.index));
        let mut empty = ImageTable::new(wallet.poly(), Some(&[]));
        while !empty.build() {}
        for target in [indexed, empty.target().unwrap()] {
            assert_eq!(
                decode(&foreign.words.map(Some), Some(&target), None),
                Err(DecodeError::NotInTarget)
            );
        }
    }

    #[test]
    fn repair_finds_up_to_three_wrong_words() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[1];
        let card = encode(share, &wallet.shared_key).unwrap();
        let target = wallet.record(share);
        for e in 1..=PARITY {
            let boxes = random_words(e, &mut rng);
            let mut entries = card.words.map(Some);
            corrupt(&mut entries, &boxes, &mut rng);
            let mut stats = RepairStats::default();
            let decoded =
                found(repair_search(&entries, e, &target, &mut stats, &mut |_| false).unwrap());
            assert_eq!(decoded.share, *share);
            assert_eq!(decoded.codeword.corrected, mask(&boxes));
            assert!(stats.subsets <= [1, 64, 2016, 41664][e]);
        }
    }

    #[test]
    fn repair_worst_case_tries_every_set() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(3, 3, &mut rng);
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        // Lexicographic order reaches the last three words last.
        corrupt(&mut entries, &[61, 62, 63], &mut rng);
        // No spare syndrome is left, so only the index filters. The share record allows one
        // index (about 1 set in 256 passes), the wallet's set {1, 2, 3} three, and every set
        // that passes fits.
        let mut table = ImageTable::new(wallet.poly(), Some(&[1, 2, 3]));
        while !table.build() {}
        for (target, most) in [(wallet.record(share), 300), (table.target().unwrap(), 700)] {
            let mut stats = RepairStats::default();
            let decoded =
                found(repair_search(&entries, 3, &target, &mut stats, &mut |_| false).unwrap());
            assert_eq!(decoded.share, *share);
            assert_eq!(stats.subsets, 41664);
            assert_eq!((stats.fits, stats.images), (stats.rebuilds, stats.rebuilds));
            assert!(stats.rebuilds < most, "{stats:?}");
            assert!(stats.rebuilds > most / 10, "{stats:?}");
            if let Target::Indexed(_) = target {
                assert!(decoded.canonical.unwrap().is_canonical());
            }
        }

        // A fourth wrong word is past the search.
        corrupt(&mut entries, &[0], &mut rng);
        let mut stats = RepairStats::default();
        assert_eq!(
            repair_search(&entries, 3, &wallet.record(share), &mut stats, &mut |_| {
                false
            }),
            Ok(RepairOutcome::Exhausted)
        );
        assert_eq!(stats.subsets, 41664);
    }

    #[test]
    fn repair_with_unreadable_boxes() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();
        let target = Target::Polynomial(wallet.poly());
        for (e, f) in [(2, 1), (1, 2), (0, 3)] {
            let boxes = random_words(e + f, &mut rng);
            let mut entries = card.words.map(Some);
            for &j in &boxes[..f] {
                entries[j] = None;
            }
            corrupt(&mut entries, &boxes[f..], &mut rng);
            let mut stats = RepairStats::default();
            let decoded =
                found(repair_search(&entries, e, &target, &mut stats, &mut |_| false).unwrap());
            assert_eq!(decoded.share, *share);
            assert_eq!(decoded.codeword.erased, mask(&boxes[..f]));
            assert_eq!(decoded.codeword.corrected, mask(&boxes[f..]));
            assert!(decoded.canonical.unwrap().is_canonical());
            // Only sets of readable boxes are tried.
            assert!(stats.subsets as f64 <= crate::reveal_card::model::choose(WORDS - f, e));
        }
        let mut entries = card.words.map(Some);
        entries[0] = None;
        let mut stats = RepairStats::default();
        assert_eq!(
            repair_search(&entries, 3, &target, &mut stats, &mut |_| false),
            Err(DecodeError::TooManyErasures)
        );
    }

    #[test]
    fn repair_never_accepts_another_share() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let card = encode(&wallet.shares[0], &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        corrupt(&mut entries, &random_words(2, &mut rng), &mut rng);
        let mut stats = RepairStats::default();
        assert_eq!(
            repair_search(
                &entries,
                2,
                &wallet.record(&wallet.shares[1]),
                &mut stats,
                &mut |_| { false }
            ),
            Ok(RepairOutcome::Exhausted)
        );
        assert_eq!(stats.subsets, 2016);
    }

    #[test]
    fn stop_ends_the_repair() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        corrupt(&mut entries, &[62, 63], &mut rng);
        let mut stats = RepairStats::default();
        assert_eq!(
            repair_search(&entries, 2, &wallet.record(share), &mut stats, &mut |s| s
                .subsets
                >= 10),
            Ok(RepairOutcome::Stopped)
        );
        assert_eq!(stats.subsets, 10);
    }

    /// A share of a random `threshold`-of-n wallet at `index`, without making the other shares.
    fn share_at(threshold: usize, index: u32, rng: &mut impl RngCore) -> (SecretShare, SharedKey) {
        let poly: Vec<Scalar<Secret, Zero>> = (0..threshold)
            .map(|_| Scalar::random(rng).mark_zero())
            .collect();
        let index = share_index(index as u8).unwrap();
        let share = SecretShare {
            index,
            share: poly::scalar::eval(&poly, index),
        };
        let point_poly = poly::scalar::to_point_poly(&poly);
        (share, SharedKey::from_poly(point_poly).non_zero().unwrap())
    }

    #[test]
    fn round_trips() {
        let mut rng = rand::thread_rng();
        for (t, i) in [
            (1, 1),
            (2, 1),
            (2, 255),
            (3, 1),
            (3, 255),
            (255, 1),
            (255, 255),
        ] {
            let (share, key) = share_at(t, i, &mut rng);
            let card = encode(&share, &key).unwrap();
            assert_eq!(distinct(&card.words), 1);
            let mut sorted = card.words;
            sorted.sort();
            assert!(sorted.windows(2).all(|w| w[0] != w[1]), "64 distinct words");
            let entries = card.words.map(Some);
            let record = Target::Share {
                image: share.share_image(),
                threshold: t,
            };
            let decoded = decode(&entries, Some(&record), None).unwrap();
            assert_eq!(decoded.share, share);
            assert_eq!((decoded.index, decoded.threshold), (i as u8, Some(t as u8)));
            assert_eq!(decoded.codeword.values(), card.values);
            assert_eq!(decoded.masks.ctr, Some(card.counter));
            if t <= 3 {
                let poly = Target::Polynomial(key.point_polynomial());
                let decoded = decode(&entries, Some(&poly), None).unwrap();
                assert_eq!(decoded.share, share);
                assert!(decoded.canonical.unwrap().is_canonical());
            }
            // Every single wrong word and every single erasure (2e + f ≤ 2), then patterns that
            // use the whole redundancy (2e + f = 3) at random.
            for j in 0..WORDS {
                let mut wrong = entries;
                corrupt(&mut wrong, &[j], &mut rng);
                let decoded = decode(&wrong, Some(&record), None).unwrap();
                assert_eq!((decoded.share, decoded.codeword.corrected), (share, 1 << j));
                let mut erased = entries;
                erased[j] = None;
                assert_eq!(decode(&erased, Some(&record), None).unwrap().share, share);
            }
            for _ in 0..10 {
                let words = random_words(3, &mut rng);
                let mut erased = entries;
                words.iter().for_each(|&j| erased[j] = None);
                assert_eq!(decode(&erased, Some(&record), None).unwrap().share, share);
                let mut mixed = entries;
                mixed[words[0]] = None;
                corrupt(&mut mixed, &words[1..2], &mut rng);
                assert_eq!(decode(&mixed, Some(&record), None).unwrap().share, share);
            }
        }
    }

    #[test]
    fn the_encode_job_matches_encode() {
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let (share, key) = share_at(2, rng.gen_range(1..=255), &mut rng);
            let mut job = EncodeJob::new(&share, &key).unwrap();
            let mut steps = 1;
            let card = loop {
                match job.step() {
                    Some(card) => break card.unwrap(),
                    None => steps += 1,
                }
            };
            // Every share takes the same 5 steps through the fixed counters.
            assert_eq!(steps, FIXED_COUNTERS.div_ceil(COUNTERS_PER_STEP) as usize);
            assert_eq!(card, encode(&share, &key).unwrap());
            assert_eq!(
                job.step().unwrap().unwrap(),
                card,
                "a further step repeats it"
            );
            assert_eq!(encode_checked(&share, &key).unwrap(), card);
        }
        let zero = SecretShare {
            index: share_index(1).unwrap(),
            share: Scalar::zero(),
        };
        let (_, key) = share_at(2, 1, &mut rng);
        assert_eq!(
            EncodeJob::new(&zero, &key).err(),
            Some(EncodeError::ZeroScalar)
        );
    }

    /// Encoding holds nothing big on the stack: the Lagrange rows are boxed.
    #[test]
    fn encode_frames_stay_small() {
        std::println!(
            "EncodeJob {} B, Card {} B, RepairJob {} B",
            core::mem::size_of::<EncodeJob>(),
            core::mem::size_of::<Card>(),
            core::mem::size_of::<RepairJob>()
        );
        assert!(core::mem::size_of::<EncodeJob>() < 1024);
        assert!(core::mem::size_of::<Card>() < 512);
        assert!(core::mem::size_of::<RepairJob>() < 1024);
        assert!(core::mem::size_of::<[u16; WORDS]>() + core::mem::size_of::<[u16; MASKS]>() < 256);
    }

    #[test]
    fn every_unique_decoding_pattern_is_corrected() {
        let mut rng = rand::thread_rng();
        let (share, key) = share_at(2, 7, &mut rng);
        let card = encode(&share, &key).unwrap();
        let record = Target::Share {
            image: share.share_image(),
            threshold: 2,
        };
        let check = |entries: &Entries, wrong: WordMask, erased: WordMask| {
            let decoded = decode(entries, Some(&record), None).unwrap();
            assert_eq!(decoded.share, share);
            assert_eq!(
                (decoded.codeword.corrected, decoded.codeword.erased),
                (wrong, erased)
            );
        };
        // Every pattern of up to two erasures, every wrong word with an erasure (2e + f = 3),
        // every single wrong word (above), and three erasures at the ends and at random (all
        // of them in `every_triple_erasure_is_corrected`).
        for a in 0..WORDS {
            for b in a..WORDS {
                let mut entries = card.words.map(Some);
                entries[a] = None;
                entries[b] = None;
                check(&entries, 0, mask(&[a, b]));
            }
        }
        for erased in 0..WORDS {
            for wrong in (0..WORDS).filter(|&w| w != erased) {
                let mut entries = card.words.map(Some);
                entries[erased] = None;
                corrupt(&mut entries, &[wrong], &mut rng);
                check(&entries, mask(&[wrong]), mask(&[erased]));
            }
        }
        let mut cases: Vec<Vec<usize>> = vec![vec![0, 1, 2], vec![61, 62, 63], vec![0, 31, 63]];
        cases.extend((0..100).map(|_| random_words(3, &mut rng)));
        for words in &cases {
            let mut entries = card.words.map(Some);
            for &j in words {
                entries[j] = None;
            }
            check(&entries, 0, mask(words));
            // One wrong word and one erasure: 2e + f = 3.
            let mut entries = card.words.map(Some);
            entries[words[0]] = None;
            corrupt(&mut entries, &words[1..2], &mut rng);
            check(&entries, mask(&words[1..2]), mask(&words[..1]));
        }
    }

    /// All C(64, 3) = 41,664 patterns of three erasures (PK.11). Slow in debug; run with
    /// `cargo test -p frost_backup --release --lib every_triple_erasure -- --ignored`.
    #[test]
    #[ignore = "slow: all 41,664 triple erasures, run in release"]
    fn every_triple_erasure_is_corrected() {
        let mut rng = rand::thread_rng();
        let (share, key) = share_at(2, 7, &mut rng);
        let card = encode(&share, &key).unwrap();
        let record = Target::Share {
            image: share.share_image(),
            threshold: 2,
        };
        let mut patterns = 0;
        for a in 0..WORDS {
            for b in a + 1..WORDS {
                for c in b + 1..WORDS {
                    let mut entries = card.words.map(Some);
                    entries[a] = None;
                    entries[b] = None;
                    entries[c] = None;
                    let decoded = decode(&entries, Some(&record), None).unwrap();
                    assert_eq!(decoded.share, share);
                    assert_eq!(
                        (decoded.codeword.corrected, decoded.codeword.erased),
                        (0, mask(&[a, b, c]))
                    );
                    patterns += 1;
                }
            }
        }
        assert_eq!(patterns, 41_664);
    }

    /// Runs a restore's decode on the host: unique decoding, then the repair stages
    /// [`repair_range`] allows against the target.
    fn restore(entries: &Entries, target: &Target) -> Option<Decoded> {
        if let Ok(decoded) = decode(entries, Some(target), None) {
            return Some(decoded);
        }
        let unreadable = erasures(entries).count_ones() as usize;
        for wrong in repair_range(unreadable) {
            let mut job = RepairJob::new(entries, wrong).ok()?;
            loop {
                match job.step(target, 64) {
                    None => continue,
                    Some(RepairOutcome::Found(decoded)) => return Some(decoded),
                    Some(_) => break,
                }
            }
        }
        None
    }

    #[test]
    fn two_wrong_words_are_repaired_at_the_worst_positions() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[1];
        let card = encode(share, &wallet.shared_key).unwrap();
        let poly = Target::Polynomial(wallet.poly());
        for target in [wallet.record(share), poly] {
            for (wrong, erased) in [
                (vec![62, 63], vec![]),
                (vec![0, 63], vec![]),
                (vec![62, 63], vec![0]),
                (vec![0, 63], vec![62]),
                (vec![61, 62], vec![63]),
            ] {
                let mut entries = card.words.map(Some);
                corrupt(&mut entries, &wrong, &mut rng);
                for &j in &erased {
                    entries[j] = None;
                }
                let decoded = restore(&entries, &target).expect("repaired");
                assert_eq!(decoded.share, *share);
                assert_eq!(decoded.codeword.corrected, mask(&wrong));
                assert_eq!(decoded.codeword.erased, mask(&erased));
            }
        }
        // Three erasures fill without repair; the stages after them are empty.
        let mut entries = card.words.map(Some);
        for j in [0, 32, 63] {
            entries[j] = None;
        }
        assert_eq!(
            restore(&entries, &wallet.record(share)).unwrap().share,
            *share
        );
    }

    #[test]
    fn three_wrong_words_are_past_the_cap() {
        assert_eq!(MAX_REPAIR_WRONG, 2);
        assert_eq!(repair_range(0), 2..=2);
        assert_eq!(repair_range(1), 2..=2);
        assert_eq!(repair_range(2), 1..=1);
        assert!(repair_range(3).is_empty());
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        corrupt(&mut entries, &[5, 40, 63], &mut rng);
        assert_eq!(
            RepairJob::new(&entries, 3).err(),
            Some(DecodeError::TooManyErasures)
        );
        assert!(restore(&entries, &wallet.record(share)).is_none());
        // The search itself still finds them, with the cap lifted.
        let mut job = RepairJob::uncapped(&entries, 3).unwrap();
        let outcome = loop {
            if let Some(outcome) = job.step(&wallet.record(share), 1024) {
                break outcome;
            }
        };
        assert_eq!(found(outcome).share, *share);
    }

    /// Four wrong words are past the code's distance from the card, so no restore returns the
    /// share; and no other share ever comes back, as s·G must be in the target.
    #[test]
    fn four_wrong_words_never_give_a_wrong_share() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[2];
        let card = encode(share, &wallet.shared_key).unwrap();
        let poly = Target::Polynomial(wallet.poly());
        for _ in 0..20 {
            let mut entries = card.words.map(Some);
            corrupt(&mut entries, &random_words(4, &mut rng), &mut rng);
            assert!(restore(&entries, &wallet.record(share)).is_none());
            assert!(restore(&entries, &poly).is_none());
        }
        // The worst lie: four words moved onto another codeword with the same index.
        for _ in 0..5 {
            let words: [usize; 4] = random_words(4, &mut rng).try_into().unwrap();
            let delta = weight_four(&words);
            let entries: Entries = core::array::from_fn(|j| Some(card.words[j] ^ delta[j]));
            assert!(restore(&entries, &wallet.record(share)).is_none());
            assert!(restore(&entries, &poly).is_none());
            // Without a target it decodes, cleanly, to another scalar: why repair and the
            // restored share need a target.
            let other = decode(&entries, None, None);
            assert!(other.map_or(true, |decoded| decoded.share.share != share.share));
        }
    }

    /// Steps a job to its outcome.
    fn run(mut job: RepairJob, target: &Target) -> (RepairOutcome, RepairStats) {
        loop {
            if let Some(outcome) = job.step(target, 256) {
                if !matches!(outcome, RepairOutcome::Found(_)) || job.stats().images > 0 {
                    return (outcome, *job.stats());
                }
            }
        }
    }

    /// Review B15 secrecy S-1: the prefilter's 3×3 inverse is right for every shape of set
    /// (1 to 3 words, padded with the identity, and a stand-in's identity), and costs the same
    /// table work for each, so a stand-in check costs what a real one does. Before, elimination
    /// with the zero-shortcut tables made the identity much cheaper.
    #[test]
    #[allow(clippy::needless_range_loop)]
    fn the_prefilter_inverse_costs_the_same_for_every_set() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let card = encode(&wallet.shares[0], &wallet.shared_key).unwrap();
        let prefilter = Prefilter::new::<TableFree>(&card.words.map(Some));
        let mut costs = alloc::collections::BTreeSet::new();
        for size in 0..=PARITY {
            for _ in 0..50 {
                let words = random_words(size, &mut rng);
                let q = mask(&words);
                let mut work = Work::default();
                let index = prefilter.check::<TableFree>(q, &mut work);
                costs.insert((work.table_ops, work.check_muls));
                // A codeword's syndromes are 0, so every set fits it, at the card's index.
                assert_eq!(index, Some(layout::index_of_h0(prefilter.split_received)));
                let entry = |m: usize, i: usize| match (m < size, i < size) {
                    (true, true) => prefilter.checks[words[i]][m],
                    _ => (m == i) as u16,
                };
                let inverse = invert(entry, &mut 0);
                for r in 0..PARITY {
                    for c in 0..PARITY {
                        let product =
                            (0..PARITY).fold(0, |p, k| p ^ gf::mul(entry(r, k), inverse[k][c]));
                        assert_eq!(product, (r == c) as u16, "{size} words: {words:?}");
                    }
                }
            }
        }
        assert_eq!(costs.len(), 1, "{costs:?}");
    }

    #[test]
    fn the_prefilter_agrees_with_the_full_rebuild() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        for round in 0..6 {
            let share = &wallet.shares[round % 3];
            let card = encode(share, &wallet.shared_key).unwrap();
            let target = if round % 2 == 0 {
                wallet.record(share)
            } else {
                Target::Polynomial(wallet.poly())
            };
            // (wrong words written, unreadable words, the size of set searched)
            for (written, unreadable, wrong) in [
                (0, 0, 0),
                (1, 0, 1),
                (1, 1, 1),
                (2, 0, 2),
                (2, 1, 2),
                (1, 2, 1),
                (3, 0, 2),
                (2, 0, 1),
                (4, 0, 2),
            ] {
                let words = random_words(written + unreadable, &mut rng);
                let mut entries = card.words.map(Some);
                corrupt(&mut entries, &words[..written], &mut rng);
                for &j in &words[written..] {
                    entries[j] = None;
                }
                let filtered = run(RepairJob::uncapped(&entries, wrong).unwrap(), &target);
                let oracle = run(RepairJob::oracle(&entries, wrong).unwrap(), &target);
                assert_eq!(filtered.0, oracle.0, "{written} {unreadable} {wrong}");
                assert_eq!(filtered.1.subsets, oracle.1.subsets);
                assert!(filtered.1.rebuilds <= oracle.1.rebuilds);
                if written <= wrong {
                    assert_eq!(found(filtered.0).share, *share);
                }
            }
        }
        // Three wrong words, with the cap lifted: the prefilter has only the index.
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();
        let mut entries = card.words.map(Some);
        corrupt(&mut entries, &[2, 9, 17], &mut rng);
        let target = wallet.record(share);
        let filtered = run(RepairJob::uncapped(&entries, 3).unwrap(), &target);
        let oracle = run(RepairJob::oracle(&entries, 3).unwrap(), &target);
        assert_eq!(filtered.0, oracle.0);
        assert_eq!(found(filtered.0).share, *share);
    }

    /// [`TableFree`], counting its multiplications.
    enum Counting {}

    std::thread_local! {
        static MULS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    }

    impl Field for Counting {
        fn mul(a: u16, b: u16) -> u16 {
            MULS.with(|m| m.set(m.get() + 1));
            TableFree::mul(a, b)
        }

        fn add_scaled(basis: &mut Basis, i: usize, y: u16, out: &mut [u16], column: &mut [u16]) {
            column.fill(0);
            basis.add_scaled(i, 1, column);
            for (o, &c) in out.iter_mut().zip(column.iter()) {
                *o ^= Self::mul(y, c);
            }
        }
    }

    fn muls<T>(f: impl FnOnce() -> T) -> (T, u64) {
        MULS.with(|m| m.set(0));
        let out = f();
        (out, MULS.with(|m| m.get()))
    }

    /// The operation counts a device bench multiplies by unit times. Table-free
    /// multiplications on secret values dominate encoding and the prefilter; a rebuild is
    /// about 61² of them plus as many table operations, and s·G is a scalar multiplication.
    ///
    /// ESP32-C3 estimate, from L1's measured 7.0 ms encode (2,976 table-free multiplications,
    /// 16 counters) and 2.0 ms rebuild at 29 words (≈ 2.2 µs a multiplication with overheads),
    /// and 38 ms per s·G: an encode ≈ 59,552 × 2.2 µs ≈ 0.13 s (5 steps of ≈ 26 ms, plus the
    /// Lagrange rows from the tables); a rebuild ≈ 8.4 ms; a prefilter candidate ≈ 30 µs. So
    /// the worst repairs at the cap with a target that names the index: e = 2, f = 0 ≈ 2,016
    /// candidates ≈ 0.06 s plus about one rebuild and s·G ≈ 0.11 s; e = 2, f = 1 ≈ 0.06 s
    /// plus about 8 × (rebuild + s·G) ≈ 0.43 s; the worst unique decode (65 rebuilds) ≈ 0.55 s.
    /// With F and no index set every index but 0 passes, so e = 2, f = 1 costs about 1,950
    /// rebuilds and s·G's (spec PK.8 amendment A6).
    #[test]
    fn worst_case_operation_counts() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[0];
        let card = encode(share, &wallet.shared_key).unwrap();

        // One encode: 32 base words of the threshold word and the 24 share values, then 36
        // masks per device word for each of the 50 fixed counters and once more for the card.
        let (encoded, encode_muls) = muls(|| encode_with::<Counting>(share, &wallet.shared_key));
        assert_eq!(encoded.unwrap(), card);
        assert_eq!(encode_muls, 32 * 25 + (FIXED_COUNTERS as u64 + 1) * 32 * 36);
        assert_eq!(encode_muls, 59_552);

        // The worst unique decode: one wrong word, in the last place.
        let mut entries = card.words.map(Some);
        corrupt(&mut entries, &[63], &mut rng);
        let (decoded, unique_muls) = muls(|| decode_unique_with::<Counting>(&entries));
        assert_eq!(decoded.unwrap().corrected, 1 << 63);

        let target = wallet.record(share);
        let mut rows = Vec::new();
        for (wrong, erased, candidates) in [
            (&[62usize, 63][..], &[][..], 2016u64),
            (&[62, 63], &[0], 1953),
        ] {
            let mut entries = card.words.map(Some);
            corrupt(&mut entries, wrong, &mut rng);
            for &j in erased {
                entries[j] = None;
            }
            let mut stats = RepairStats::default();
            let (outcome, search_muls) = muls(|| {
                repair_search_with::<Counting>(&entries, 2, &target, &mut stats, &mut |_| false)
            });
            assert_eq!(found(outcome.unwrap()).share, *share);
            // The wrong words come last, so every set is tried.
            assert_eq!(stats.subsets, candidates);
            assert!(stats.images <= stats.rebuilds);
            match erased.len() {
                // One spare syndrome (1 in 2048) then the index: almost only the hit.
                0 => assert!(stats.rebuilds <= 3, "{stats:?}"),
                // No spare syndrome: the index alone passes about 7.6.
                _ => assert!(stats.rebuilds <= 40, "{stats:?}"),
            }
            rows.push((erased.len(), stats, search_muls));
        }
        std::println!("encode: {encode_muls} table-free multiplications");
        std::println!("worst unique decode: {unique_muls} table-free multiplications");
        for (f, stats, search_muls) in rows {
            std::println!(
                "repair e = 2, f = {f}: {} candidates, {} rebuilds, {} s·G, {search_muls} \
                 table-free multiplications",
                stats.subsets,
                stats.rebuilds,
                stats.images
            );
        }
    }

    /// The entries of a card with `wrong` words replaced and `erased` words unreadable, at
    /// random places.
    fn damaged(card: &Card, wrong: usize, erased: usize, rng: &mut impl Rng) -> Entries {
        let mut entries = card.words.map(Some);
        let places = random_words(wrong + erased, rng);
        corrupt(&mut entries, &places[..wrong], rng);
        for &j in &places[wrong..] {
            entries[j] = None;
        }
        entries
    }

    /// A [`RestoreSearch`] run until it's done, then finished.
    fn search_to_the_end(
        entries: &Entries,
        poly: Option<&Poly>,
        indices: Option<&[u8]>,
    ) -> (SearchEnd, usize) {
        let mut search = RestoreSearch::new(entries, poly.map(|_| IndexSet::new(indices)));
        let mut slices = 0;
        while !search.is_done() {
            search.slice();
            slices += 1;
            assert!(slices < 5_000, "the search ends");
        }
        (search.finish(poly), slices)
    }

    /// What the device's restore made of entries before owner R2-1: [`decode`], then with a
    /// target a [`RepairJob`] per number of wrong words. `Some((share, unique))` for a share
    /// the masks passed, `Err` for "another wallet's card".
    fn decode_then_repair(
        entries: &Entries,
        target: Option<&Target>,
    ) -> Result<Option<(SecretShare, bool)>, ()> {
        match decode(entries, target, None) {
            Err(DecodeError::NotInTarget) => Err(()),
            Ok(d) if d.masks.self_ok && d.masks.wallet == Some(false) => Err(()),
            Ok(d) => Ok(d.masks.passed().then_some((d.share, true))),
            Err(DecodeError::TooManyErasures | DecodeError::WordRange) => Ok(None),
            Err(_) => {
                let Some(target) = target else {
                    return Ok(None);
                };
                let f = erasures(entries).count_ones() as usize;
                for wrong in repair_range(f) {
                    let mut job = RepairJob::new(entries, wrong).unwrap();
                    loop {
                        match job.step(target, 64) {
                            None => continue,
                            Some(RepairOutcome::Found(d)) => {
                                return Ok(d.masks.passed().then_some((d.share, false)))
                            }
                            Some(_) => break,
                        }
                    }
                }
                Ok(None)
            }
        }
    }

    /// The same, from a [`RestoreSearch`].
    fn searched(end: SearchEnd) -> Result<Option<(SecretShare, bool)>, ()> {
        match end {
            SearchEnd::Refused(_) | SearchEnd::NotFound => Ok(None),
            SearchEnd::Read(SearchStage::Unique, Err(DecodeError::NotInTarget)) => Err(()),
            SearchEnd::Read(SearchStage::Unique, Ok(d))
                if d.masks.self_ok && d.masks.wallet == Some(false) =>
            {
                Err(())
            }
            SearchEnd::Read(stage, Ok(d)) => Ok(d
                .masks
                .passed()
                .then_some((d.share, stage == SearchStage::Unique))),
            SearchEnd::Read(_, Err(_)) => Ok(None),
        }
    }

    /// Owner R2-1: the constant-work search finds what the decode and repair it replaces found,
    /// in the same stage, for every mix of wrong and unreadable words up to the cap and past
    /// it, against F with the card's index set, another set, or none, and without F; and for
    /// another wallet's card.
    #[test]
    fn the_restore_search_agrees_with_decode_and_repair() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let other = Wallet::new(2, 3, &mut rng);
        let share = &wallet.shares[1];
        let card = encode(share, &wallet.shared_key).unwrap();
        let other_card = encode(&other.shares[0], &other.shared_key).unwrap();
        let own = [byte(share.index)];
        let elsewhere = [byte(wallet.shares[0].index)];
        for (wrong, erased) in [
            (0, 0),
            (1, 0),
            (0, 1),
            (1, 1),
            (0, 2),
            (0, 3),
            (2, 0),
            (2, 1),
            (1, 2),
            (3, 0),
            (0, 4),
        ] {
            let entries = damaged(&card, wrong, erased, &mut rng);
            for indices in [Some(&own[..]), Some(&elsewhere[..])] {
                let mut table = ImageTable::new(wallet.poly(), indices);
                while !table.build() {}
                let old = decode_then_repair(&entries, table.target().as_ref());
                let (end, _) = search_to_the_end(&entries, Some(wallet.poly()), indices);
                assert_eq!(searched(end), old, "{wrong} wrong, {erased} unreadable");
            }
            let old = decode_then_repair(&entries, None);
            let (end, _) = search_to_the_end(&entries, None, None);
            assert_eq!(
                searched(end),
                old,
                "{wrong} wrong, {erased} unreadable, no F"
            );
            let expect_share = wrong <= 2 && wrong + erased <= PARITY && 2 * wrong + erased <= 3;
            if expect_share {
                assert_eq!(old, Ok(Some((*share, true))));
            }
        }
        // Another wallet's card, read uniquely, and with too many wrong words for that.
        for wrong in [0, 1, 2] {
            let entries = damaged(&other_card, wrong, 0, &mut rng);
            let mut table = ImageTable::new(wallet.poly(), None);
            while !table.build() {}
            let old = decode_then_repair(&entries, table.target().as_ref());
            let (end, _) = search_to_the_end(&entries, Some(wallet.poly()), None);
            assert_eq!(searched(end), old, "another wallet's card, {wrong} wrong");
            assert_eq!(old.is_err(), wrong < 2);
        }
    }

    /// Owner R2-1: every slice of a [`RestoreSearch`] does the same work whatever the entries,
    /// before the search is done and after, and so does its finish, whether it found a card or
    /// not. Too many unreadable words and words out of range work on stand-ins.
    #[test]
    fn the_restore_search_does_the_same_work_every_slice() {
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let card = encode(&wallet.shares[0], &wallet.shared_key).unwrap();
        let mut out_of_range = card.words.map(Some);
        out_of_range[9] = Some(4000);
        let inputs = [
            card.words.map(Some),
            damaged(&card, 1, 0, &mut rng),
            damaged(&card, 2, 0, &mut rng),
            damaged(&card, 2, 1, &mut rng),
            damaged(&card, 0, 3, &mut rng),
            damaged(&card, 0, 5, &mut rng),
            damaged(&card, 7, 0, &mut rng),
            out_of_range,
        ];
        let mut traces = alloc::collections::BTreeSet::new();
        for entries in inputs {
            for poly in [Some(wallet.poly()), None] {
                let mut search = RestoreSearch::new(&entries, poly.map(|_| IndexSet::new(None)));
                let mut slices = vec![];
                for _ in 0..40 {
                    let before = *search.work();
                    search.slice();
                    slices.push(search.work().since(&before));
                }
                let before = *search.work();
                let _ = search.finish(poly);
                let finish = search.work().since(&before);
                let before = *search.work();
                search.slice();
                let after = search.work().since(&before);
                assert!(slices.iter().all(|w| *w == slices[0]));
                assert_eq!(after, slices[0]);
                traces.insert((poly.is_some(), format!("{:?} {finish:?}", slices[0])));
            }
        }
        assert_eq!(traces.len(), 2, "one trace with F, one without: {traces:?}");
    }

    /// Host timings of the constant-work search's slices and finish, for the report (owner
    /// R2-1). `cargo test -p frost_backup --release --lib -- --ignored restore_search_timing
    /// --nocapture`.
    #[test]
    #[ignore]
    fn restore_search_timing() {
        extern crate std;
        use std::time::Instant;
        let mut rng = rand::thread_rng();
        let wallet = Wallet::new(2, 3, &mut rng);
        let card = encode(&wallet.shares[0], &wallet.shared_key).unwrap();
        let cases = [
            ("clean", card.words.map(Some)),
            ("1 wrong", damaged(&card, 1, 0, &mut rng)),
            ("2 wrong", damaged(&card, 2, 0, &mut rng)),
            ("2 wrong 1 unreadable", damaged(&card, 2, 1, &mut rng)),
            ("7 wrong", damaged(&card, 7, 0, &mut rng)),
        ];
        let set = [1u8, 2, 3];
        // Twice, the first round warming up the caches.
        for (name, entries) in cases.iter().chain(&cases) {
            let mut search = RestoreSearch::new(entries, Some(IndexSet::new(Some(&set))));
            let mut times = vec![];
            let mut done_at = None;
            for k in 0..300 {
                let started = Instant::now();
                search.slice();
                times.push(started.elapsed().as_micros());
                if done_at.is_none() && search.is_done() {
                    done_at = Some(k + 1);
                }
            }
            let started = Instant::now();
            let end = search.finish(Some(wallet.poly()));
            let finish = started.elapsed().as_micros();
            times.sort();
            let mean = times.iter().sum::<u128>() / times.len() as u128;
            std::println!(
                "{name}: slice mean {mean}us, min {}us, median {}us, max {}us; finish {finish}us; \
                 search done after {done_at:?} slices; found {}",
                times[0],
                times[times.len() / 2],
                times[times.len() - 1],
                matches!(end, SearchEnd::Read(_, Ok(_)))
            );
        }
        std::println!("work of one slice: {:?}", {
            let mut search = RestoreSearch::new(&card.words.map(Some), None);
            search.slice();
            *search.work()
        });
    }
}
