//! Layout C′ (packed): where every bit of the share sits on the card.
//!
//! The card is the RS[64,61] codeword of a polynomial f of degree at most 60 over GF(2^11): word
//! w (code index j = w − 1) is f(w). C′ describes f by its **values** at the 61 points of
//! [`BASIS`], one value per slot:
//!
//! | Slots | Point | Holds |
//! |---|---|---|
//! | 0..5 | hidden x = 2019..2023 | slot 2 is the threshold word (0b000 ‖ t8) XOR μ; slots 0, 1, 3, 4 are masks m1..m4 |
//! | 5..37 | words 33..64 | masks m5..m36: the phone half P |
//! | 37..61 | hidden x = 2024..2047 | h0..h23 = u8(i) ‖ ser32(s), 11 bits each, most significant first |
//!
//! Every word of the device half (words 1-32) is computed from the 61 values; the phone half is
//! the 32 random values P themselves. The 29 hidden points are never a word: every word and
//! record index is a u8 and word j sits at j + 1 ≤ 256 < [`RANDOM_HIDDEN_FIRST`], and every function
//! that evaluates a word asserts j < 64.
//!
//! The proofs commit to 311 bits: the 55 bits of the 5 hidden random values, then the 256 scalar
//! bits. The 8 index bits and the 352 bits of P are public: the phone folds them itself.
//! Committed bit `v` numbers them in slot order, then bit order.

use super::{
    codec::{Field, TableFree},
    proof::{Ops, Tagged},
};
use crate::reveal_card::gf;
use schnorr_fun::frost::ShareIndex;

/// Words on the card.
pub const WORDS: usize = 64;
/// Values that define the card, and the coefficients of its codeword: the code's dimension k.
/// Any 61 correct words rebuild the card.
pub const COEFFS: usize = 61;
/// Spare words: unique decoding fixes `e` wrong words and `f` erasures when `2e + f <= 3`.
pub const PARITY: usize = WORDS - COEFFS;
/// Bits per word and per value.
pub const WORD_BITS: usize = 11;
const WORD_MASK: u16 = (1 << WORD_BITS) - 1;

/// The card format (the wire's and the records' format_id): 3 is C′, the packed layout.
pub const FORMAT_ID: u8 = 3;

/// Words per half.
pub const HALF: usize = WORDS / 2;
/// The device half: words 1-32, shown only on the device. Every one is computed.
pub const DEVICE_HALF: core::ops::Range<usize> = 0..HALF;
/// The phone half: words 33-64, shown only in the app. They're the random values P.
pub const PHONE_HALF: core::ops::Range<usize> = HALF..WORDS;
/// A set of word positions: bit `j` is word `j + 1`.
pub type WordMask = u64;
const _: () = assert!(WORDS <= WordMask::BITS as usize);

/// The first random hidden point, and the lowest hidden point. The hidden points are
/// 2019..=2047: the 5 hidden random values at 2019..=2023, then the 24 share values h0..h23 at
/// [`HIDDEN_FIRST`]..=2047.
pub const RANDOM_HIDDEN_FIRST: u16 = 2019;
/// The first secret hidden point, h0's (spec PK.1).
pub const HIDDEN_FIRST: u16 = 2024;
/// Hidden points.
pub const HIDDEN: usize = 29;
// No u8 index, no j + 1 and no off-by-one reaches a hidden point (word j sits at j + 1 ≤ 256),
// and every hidden point is a field element.
const _: () = assert!(RANDOM_HIDDEN_FIRST > 256);
const _: () = assert!(RANDOM_HIDDEN_FIRST + RANDOM_HIDDEN as u16 == HIDDEN_FIRST);
const _: () = assert!(RANDOM_HIDDEN_FIRST as usize + HIDDEN - 1 <= WORD_MASK as usize);

/// Hidden random values, in slots 0..5 at hidden points 0..5 (x = 2019..2023).
pub const RANDOM_HIDDEN: usize = 5;
/// The slot of the threshold word, (0b000 ‖ t8) XOR μ, at x = 2021.
pub const THRESHOLD_SLOT: usize = 2;
/// The first phone slot: slots 5..37 are words 33..64.
pub const FIRST_PHONE_SLOT: usize = RANDOM_HIDDEN;
/// The first share slot: slots 37..61 are h0..h23, at hidden points 5..29 (x = 2024..2047).
pub const FIRST_SHARE_SLOT: usize = FIRST_PHONE_SLOT + HALF;
/// Share values h0..h23: the 264 bits u8(i) ‖ ser32(s), exactly 24 × 11.
pub const SHARE_VALUES: usize = 24;
const _: () = assert!(FIRST_SHARE_SLOT == 37 && FIRST_SHARE_SLOT + SHARE_VALUES == COEFFS);
const _: () = assert!(RANDOM_HIDDEN + SHARE_VALUES == HIDDEN);
const _: () = assert!(THRESHOLD_SLOT < RANDOM_HIDDEN);

/// Masks: m1..m4 in the hidden random slots other than the threshold's, m5..m36 in the phone
/// slots.
pub const MASKS: usize = 36;
/// Self-checking masks m1..m24, which any restore can recompute from the card alone.
pub const SELF_MASKS: usize = 24;
/// Wallet-bound masks m25..m36, which hash the wallet polynomial F too.
pub const WALLET_MASKS: usize = MASKS - SELF_MASKS;
/// The slot of each mask: m_{n+1} is in slot `MASK_SLOTS[n]`.
pub const MASK_SLOTS: [usize; MASKS] = mask_slots();
/// The slots of the self-checking masks m1..m24: 0, 1, 3, 4 and 5..25 (x = 2019, 2020, 2022,
/// 2023 and words 33..52).
pub const SELF_MASK_SLOTS: [usize; SELF_MASKS] = {
    let mut slots = [0; SELF_MASKS];
    let mut n = 0;
    while n < SELF_MASKS {
        slots[n] = MASK_SLOTS[n];
        n += 1;
    }
    slots
};
/// The slots of the wallet-bound masks m25..m36: words 53..64.
pub const WALLET_MASK_SLOTS: core::ops::Range<usize> = FIRST_PHONE_SLOT + 20..FIRST_SHARE_SLOT;
const _: () = assert!(WALLET_MASK_SLOTS.end - WALLET_MASK_SLOTS.start == WALLET_MASKS);

const fn mask_slots() -> [usize; MASKS] {
    let mut slots = [0; MASKS];
    let mut n = 0;
    let mut slot = 0;
    while slot < FIRST_SHARE_SLOT {
        if slot != THRESHOLD_SLOT {
            slots[n] = slot;
            n += 1;
        }
        slot += 1;
    }
    assert!(n == MASKS);
    slots
}

/// The point of every slot: the 5 hidden random points, words 33..64, then h0..h23's points.
pub const BASIS: [u16; COEFFS] = {
    let mut basis = [0u16; COEFFS];
    let mut slot = 0;
    while slot < COEFFS {
        basis[slot] = if slot < FIRST_PHONE_SLOT {
            RANDOM_HIDDEN_FIRST + slot as u16
        } else if slot < FIRST_SHARE_SLOT {
            (HALF + slot - FIRST_PHONE_SLOT + 1) as u16
        } else {
            HIDDEN_FIRST + (slot - FIRST_SHARE_SLOT) as u16
        };
        slot += 1;
    }
    // Distinct, so the 61 values fix f.
    let mut a = 0;
    while a < COEFFS {
        let mut b = a + 1;
        while b < COEFFS {
            assert!(basis[a] != basis[b]);
            b += 1;
        }
        a += 1;
    }
    basis
};

/// The point of word `j + 1`. Every word evaluation goes through it, so no j ≥ 64 is ever
/// evaluated.
pub const fn word_point(j: usize) -> u16 {
    assert!(j < WORDS, "a card has 64 words");
    (j + 1) as u16
}

/// Hidden point `m`: x = 2019 + m. The random values are m = 0..5 and h_n is m = 5 + n.
pub const fn hidden_point(m: usize) -> u16 {
    assert!(m < HIDDEN, "there are 29 hidden points");
    RANDOM_HIDDEN_FIRST + m as u16
}

/// The hidden point of a slot, or `None` for a phone slot.
pub const fn hidden_index(slot: usize) -> Option<usize> {
    assert!(slot < COEFFS);
    if slot < FIRST_PHONE_SLOT {
        Some(slot)
    } else if slot < FIRST_SHARE_SLOT {
        None
    } else {
        Some(RANDOM_HIDDEN + slot - FIRST_SHARE_SLOT)
    }
}

// DRAFT: frozen before the first release (spec PK-O3).
/// The tag of the layout hash L.
pub const LAYOUT_TAG: &[u8] = b"frostsnap/card/c1/layout";
/// The layout descriptor. It names every point and slot, so any change to the layout changes L,
/// and with it the seal and every mask. A test rebuilds its fields from the constants above.
pub const FMT_C: &[u8] = b"frostsnap-card|packed-c1|gf2^11|poly=0x805|n=64|k=61|x=word|rand=2019..2023,33..64|thr=2021|hidden=2024..2047|h=idx8+scalar256-be|self=2019,2020,2022,2023,33..52|wallet=53..64|D=words1-32|phone=words33-64";
const _: () = assert!(FMT_C.len() <= u8::MAX as usize);

/// L = TH("frostsnap/card/c1/layout", u8 len(FMT_C) ‖ FMT_C).
pub fn layout_hash() -> [u8; 32] {
    layout_hash_of(FMT_C)
}

/// L for another descriptor, for the test that masks are bound to the layout.
pub(crate) fn layout_hash_of(descriptor: &[u8]) -> [u8; 32] {
    let len = u8::try_from(descriptor.len()).expect("a descriptor fits a length byte");
    Tagged::new(LAYOUT_TAG)
        .add(&[len])
        .add(descriptor)
        .finish(&mut Ops::default())
}

/// Committed free bits: the 55 bits of the hidden random values, which no public equation
/// covers.
pub const FREE: usize = RANDOM_HIDDEN * WORD_BITS;
/// Low bits of h0 that hold scalar bits (255, 254, 253).
const SPLIT_SCALAR_BITS: usize = 3;
/// Public bits in the share values: the 8 index bits, h0 bits 3..10.
pub const PUBLIC: usize = WORD_BITS - SPLIT_SCALAR_BITS;
/// Public bits of the phone half P.
pub const PHONE_BITS: usize = HALF * WORD_BITS;
/// Bits the proofs commit to: 55 free bits, then 256 scalar bits.
pub const COMMITTED: usize = COEFFS * WORD_BITS - PUBLIC - PHONE_BITS;
const _: () = assert!(COMMITTED == FREE + 256);
/// The committed bit holding scalar bit 0, whose blinding is solved to make the link exact.
pub const V_LINK: usize = committed_of_scalar_bit(0);
const _: () = assert!(V_LINK == 300);

/// The most committed bits that feed one output bit of one device word.
pub const MAX_SET: usize = 184;
/// Carry bits per output bit: an honest carry is at most `MAX_SET / 2`.
pub const CARRY_BITS: usize = 7;
/// Carry commitments per opened word.
pub const CARRIES_PER_BOX: usize = WORD_BITS * CARRY_BITS;
// An honest carry is at most ⌊184/2⌋ = 92: 7 bits, not 6.
const _: () = assert!(MAX_SET / 2 < 1 << CARRY_BITS && MAX_SET / 2 >= 1 << (CARRY_BITS - 1));

/// The value bit `(slot, g)` (bit `g` of the value in `slot`) of committed bit `v`.
pub const fn value_bit(v: usize) -> (usize, usize) {
    assert!(v < COMMITTED);
    if v < FREE {
        (v / WORD_BITS, v % WORD_BITS)
    } else if v < FREE + SPLIT_SCALAR_BITS {
        (FIRST_SHARE_SLOT, v - FREE)
    } else {
        let w = v - FREE - SPLIT_SCALAR_BITS;
        (FIRST_SHARE_SLOT + 1 + w / WORD_BITS, w % WORD_BITS)
    }
}

/// The committed bit at value bit `(slot, g)`, or `None` for a phone slot or an index bit.
pub const fn committed_index(slot: usize, g: usize) -> Option<usize> {
    assert!(slot < COEFFS && g < WORD_BITS);
    if slot < FIRST_PHONE_SLOT {
        Some(slot * WORD_BITS + g)
    } else if slot < FIRST_SHARE_SLOT {
        None
    } else if slot == FIRST_SHARE_SLOT {
        if g < SPLIT_SCALAR_BITS {
            Some(FREE + g)
        } else {
            None
        }
    } else {
        Some(FREE + SPLIT_SCALAR_BITS + (slot - FIRST_SHARE_SLOT - 1) * WORD_BITS + g)
    }
}

/// The value bit holding scalar bit `t` (the bit worth 2^t): bit t mod 11 of h_{23 − ⌊t/11⌋}.
pub const fn scalar_bit_position(t: usize) -> (usize, usize) {
    assert!(t < 256);
    (COEFFS - 1 - t / WORD_BITS, t % WORD_BITS)
}

/// The committed bit holding scalar bit `t`.
pub const fn committed_of_scalar_bit(t: usize) -> usize {
    let (slot, g) = scalar_bit_position(t);
    match committed_index(slot, g) {
        Some(v) => v,
        None => panic!("scalar bits are committed"),
    }
}

/// The scalar bit `t` that committed bit `v` holds, or `None` for a free bit.
pub const fn scalar_bit(v: usize) -> Option<usize> {
    if v < FREE {
        return None;
    }
    let (slot, g) = value_bit(v);
    Some((COEFFS - 1 - slot) * WORD_BITS + g)
}

/// Why metadata can't go on a card. Values are refused, never truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetaError {
    /// The index is above 255 or the threshold is 0 or above 255.
    MetadataRange,
    /// Until threshold-1 cards are decided, only index 1 of a threshold-1 wallet gets one.
    ThresholdOne,
}

/// A card's metadata. The index is public and proof-bound; the threshold is scrambled into the
/// threshold word and is only warned about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    /// The wallet's threshold, or 0 for not recorded (decoders only: no encoder writes 0).
    pub threshold: u8,
    /// The share index.
    pub index: u8,
}

impl Meta {
    /// The metadata an encoder writes for share `index` of a `threshold`-of-n wallet.
    pub fn new(index: ShareIndex, threshold: usize) -> Result<Self, MetaError> {
        let index = u32::try_from(index).map_err(|_| MetaError::MetadataRange)?;
        let index = u8::try_from(index).map_err(|_| MetaError::MetadataRange)?;
        let threshold = u8::try_from(threshold).map_err(|_| MetaError::MetadataRange)?;
        if threshold == 0 {
            return Err(MetaError::MetadataRange);
        }
        if threshold == 1 && index != 1 {
            return Err(MetaError::ThresholdOne);
        }
        Ok(Self { threshold, index })
    }
}

/// Writes Dh = u8(index) ‖ ser32(scalar) into h0..h23 (slots 37..61), 11 bits each, most
/// significant first. The 264 bits fill the 24 values exactly.
pub fn pack(index: u8, scalar: &[u8; 32], values: &mut [u16; COEFFS]) {
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = FIRST_SHARE_SLOT;
    for &byte in core::iter::once(&index).chain(scalar) {
        acc = (acc << 8) | byte as u32;
        bits += 8;
        if bits >= WORD_BITS {
            bits -= WORD_BITS;
            values[out] = (acc >> bits) as u16 & WORD_MASK;
            out += 1;
            acc &= (1 << bits) - 1;
        }
    }
    debug_assert_eq!((out, bits), (COEFFS, 0));
}

/// Reads the index and the scalar's 32 bytes back out of h0..h23.
pub fn unpack(values: &[u16; COEFFS]) -> (u8, [u8; 32]) {
    let mut bytes = [0u8; 33];
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = 0;
    for &word in &values[FIRST_SHARE_SLOT..] {
        acc = (acc << WORD_BITS) | (word & WORD_MASK) as u32;
        bits += WORD_BITS;
        while bits >= 8 {
            bits -= 8;
            bytes[out] = (acc >> bits) as u8;
            out += 1;
        }
        acc &= (1 << bits) - 1;
    }
    debug_assert_eq!(out, 1 + 32);
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&bytes[1..]);
    (bytes[0], scalar)
}

/// The index: the top 8 bits of h0.
pub const fn index_of(values: &[u16; COEFFS]) -> u8 {
    index_of_h0(values[FIRST_SHARE_SLOT])
}

/// The index in h0, its top 8 bits.
pub const fn index_of_h0(h0: u16) -> u8 {
    ((h0 & WORD_MASK) >> SPLIT_SCALAR_BITS) as u8
}

const BIT_WORDS: usize = COMMITTED.div_ceil(32);

/// A set of committed bits, or the values of all of them: bit `v` is committed bit `v`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bits(pub [u32; BIT_WORDS]);

impl Bits {
    pub fn get(&self, v: usize) -> bool {
        (self.0[v / 32] >> (v % 32)) & 1 == 1
    }

    /// Sets bit `v` to `bit` without branching on it.
    pub fn set(&mut self, v: usize, bit: bool) {
        let word = &mut self.0[v / 32];
        *word = (*word & !(1 << (v % 32))) | ((bit as u32) << (v % 32));
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|w| w.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    /// How many bits are set in both, without branching on either.
    pub fn count_common(&self, other: &Bits) -> u32 {
        self.0
            .iter()
            .zip(&other.0)
            .map(|(a, b)| (a & b).count_ones())
            .sum()
    }

    /// The bits that are set, lowest first. Branches on them, so use it on public sets only.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..COMMITTED).filter(|&v| self.get(v))
    }
}

/// The values of the committed bits of a card, x_v.
pub fn committed_bits(values: &[u16; COEFFS]) -> Bits {
    let mut bits = Bits::default();
    for v in 0..COMMITTED {
        let (slot, g) = value_bit(v);
        bits.set(v, (values[slot] >> g) & 1 == 1);
    }
    bits
}

/// The 61 values from the committed bits, the index and the phone half, without branching on
/// any of them.
pub fn values_from_bits(bits: &Bits, index: u8, phone: &[u16; HALF]) -> [u16; COEFFS] {
    let mut values = [0u16; COEFFS];
    for v in 0..COMMITTED {
        let (slot, g) = value_bit(v);
        values[slot] |= (bits.get(v) as u16) << g;
    }
    values[FIRST_PHONE_SLOT..FIRST_SHARE_SLOT].copy_from_slice(phone);
    values[FIRST_SHARE_SLOT] |= (index as u16) << SPLIT_SCALAR_BITS;
    values
}

/// The 64 words from the committed bits, the index and the phone half: the device half through
/// the Lagrange rows, table-free on the values, and the phone half as given. No branch or table
/// read depends on a value.
pub fn words_from_bits(bits: &Bits, index: u8, phone: &[u16; HALF]) -> [u16; WORDS] {
    let mut values = values_from_bits(bits, index, phone);
    let weights = basis_weights();
    let mut words = [0u16; WORDS];
    for (j, word) in words[DEVICE_HALF].iter_mut().enumerate() {
        *word = combine::<TableFree>(&lagrange_row_with(&weights, word_point(j)), &values);
    }
    words[PHONE_HALF].copy_from_slice(phone);
    super::codec::wipe(&mut values, [0; COEFFS]);
    words
}

/// Σ_k row_k·values_k: a word from its Lagrange row.
pub(crate) fn combine<F: Field>(row: &[u16; COEFFS], values: &[u16; COEFFS]) -> u16 {
    row.iter()
        .zip(values)
        .fold(0, |word, (&l, &value)| word ^ F::mul(l, value))
}

/// The barycentric weights of [`BASIS`], w_k = 1/Π_{m≠k}(x_k − x_m). The points are public, so
/// the tables compute them.
pub fn basis_weights() -> [u16; COEFFS] {
    core::array::from_fn(|k| {
        let product = BASIS
            .iter()
            .enumerate()
            .filter(|&(m, _)| m != k)
            .fold(1, |acc, (_, &x)| gf::mul(acc, BASIS[k] ^ x));
        gf::inv(product)
    })
}

/// ℓ_k(x) for every slot k: f(x) = Σ_k ℓ_k(x)·value_k. Public: it's computed with the tables
/// by the product formula, never from a const table. At a point of [`BASIS`] it's that slot's
/// indicator row.
pub fn lagrange_row(x: u16) -> [u16; COEFFS] {
    lagrange_row_with(&basis_weights(), x)
}

/// [`lagrange_row`] with the weights already computed.
pub fn lagrange_row_with(weights: &[u16; COEFFS], x: u16) -> [u16; COEFFS] {
    assert!(x <= WORD_MASK, "a field element");
    if let Some(slot) = BASIS.iter().position(|&point| point == x) {
        return core::array::from_fn(|k| (k == slot) as u16);
    }
    // ℓ_k(x) = w_k·M(x)/(x − x_k), with M(x) = Π_m (x − x_m).
    let master = BASIS.iter().fold(1, |acc, &point| gf::mul(acc, x ^ point));
    core::array::from_fn(|k| gf::mul(gf::mul(master, weights[k]), gf::inv(x ^ BASIS[k])))
}

/// The bits that feed one device word.
///
/// Bit β of the word is the XOR of the committed bits in `committed[β]` (T_jβ) and the bit β of
/// [`public_word`](Self::public_word), the part of the word the index and P give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxSets {
    pub committed: [Bits; WORD_BITS],
    /// ℓ_k(x_j) for every slot.
    pub lagrange: [u16; COEFFS],
}

impl BoxSets {
    /// The sets for device word `j + 1`, j < 32: bit β of ℓ_k(x_j) ⊗ x^g decides whether value
    /// bit `(k, g)` feeds output bit β. Phone words have no sets: they are values.
    pub fn new(j: usize) -> Self {
        assert!(j < HALF, "only the device half's words are computed");
        let lagrange = lagrange_row(word_point(j));
        let mut committed = [Bits::default(); WORD_BITS];
        for v in 0..COMMITTED {
            let (slot, g) = value_bit(v);
            // Public data, so the table arithmetic is fine here.
            let column = gf::mul(lagrange[slot], 1 << g);
            for (beta, set) in committed.iter_mut().enumerate() {
                if (column >> beta) & 1 == 1 {
                    set.set(v, true);
                }
            }
        }
        Self {
            committed,
            lagrange,
        }
    }

    /// The committed bits' part of the word: bit β is the parity of the committed bits in
    /// T_jβ. It doesn't branch on the bits.
    pub fn word(&self, bits: &Bits) -> u16 {
        (0..WORD_BITS).fold(0, |word, beta| {
            let ones = self.committed[beta].count_common(bits);
            word | (((ones & 1) as u16) << beta)
        })
    }

    /// The public part of the word: ⊕_{k=5..37} ℓ_k·P_k ⊕ ℓ_37·(index << 3). The word is
    /// [`word`](Self::word) XOR this. P is multiplied table-free. Every P value must be below
    /// 2048.
    pub fn public_word(&self, index: u8, phone: &[u16; HALF]) -> u16 {
        let share = TableFree::mul(
            self.lagrange[FIRST_SHARE_SLOT],
            (index as u16) << SPLIT_SCALAR_BITS,
        );
        phone.iter().enumerate().fold(share, |word, (k, &p)| {
            word ^ TableFree::mul(self.lagrange[FIRST_PHONE_SLOT + k], p)
        })
    }
}

/// Bytes of a packed half: 32 words of 11 bits.
pub const HALF_BYTES: usize = HALF * WORD_BITS / 8;

/// 32 words as 44 bytes, 11 bits each, most significant first. Each word must be below 2048.
pub fn pack_half(words: &[u16; HALF]) -> [u8; HALF_BYTES] {
    let mut bytes = [0u8; HALF_BYTES];
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = 0;
    for &word in words {
        acc = (acc << WORD_BITS) | (word & WORD_MASK) as u32;
        bits += WORD_BITS;
        while bits >= 8 {
            bits -= 8;
            bytes[out] = (acc >> bits) as u8;
            out += 1;
        }
        acc &= (1 << bits) - 1;
    }
    debug_assert_eq!((out, bits), (HALF_BYTES, 0));
    bytes
}

/// 44 bytes back into 32 words.
pub fn unpack_half(bytes: &[u8; HALF_BYTES]) -> [u16; HALF] {
    let mut words = [0u16; HALF];
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = 0;
    for &byte in bytes {
        acc = (acc << 8) | byte as u32;
        bits += 8;
        if bits >= WORD_BITS {
            bits -= WORD_BITS;
            words[out] = (acc >> bits) as u16 & WORD_MASK;
            out += 1;
            acc &= (1 << bits) - 1;
        }
    }
    debug_assert_eq!((out, bits), (HALF, 0));
    words
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::reveal_card::card::{eval, interpolate};
    use alloc::{string::String, vec::Vec};
    use rand::{seq::SliceRandom, Rng};
    use schnorr_fun::fun::prelude::*;

    fn index(i: u32) -> ShareIndex {
        Scalar::<Public, Zero>::from(i).non_zero().unwrap()
    }

    fn random_values(rng: &mut impl Rng) -> [u16; COEFFS] {
        core::array::from_fn(|_| rng.gen_range(0..2048))
    }

    fn phone_of(values: &[u16; COEFFS]) -> [u16; HALF] {
        values[FIRST_PHONE_SLOT..FIRST_SHARE_SLOT]
            .try_into()
            .unwrap()
    }

    /// f's 64 words from its values, through an interpolation of the 61 slot points.
    fn words_of(values: &[u16; COEFFS]) -> [u16; WORDS] {
        let coeffs = interpolate(&BASIS, values);
        core::array::from_fn(|j| eval(&coeffs, (j + 1) as u16))
    }

    #[test]
    fn committed_bits_cover_every_value_bit_once() {
        assert_eq!((COMMITTED, FREE, PUBLIC, PHONE_BITS), (311, 55, 8, 352));
        let (mut public, mut phone) = (0, 0);
        let mut v_next = 0;
        for slot in 0..COEFFS {
            for g in 0..WORD_BITS {
                match committed_index(slot, g) {
                    Some(v) => {
                        assert_eq!(v, v_next, "canonical order is slot, then bit");
                        assert_eq!(value_bit(v), (slot, g));
                        v_next += 1;
                    }
                    None if (FIRST_PHONE_SLOT..FIRST_SHARE_SLOT).contains(&slot) => phone += 1,
                    None => {
                        assert_eq!(slot, FIRST_SHARE_SLOT);
                        assert!(g >= 3);
                        public += 1;
                    }
                }
            }
        }
        assert_eq!((v_next, public, phone), (COMMITTED, PUBLIC, PHONE_BITS));
        // The free bits are the 5 hidden random values, in slot order.
        for v in 0..FREE {
            assert_eq!(value_bit(v), (v / 11, v % 11));
        }
    }

    #[test]
    fn scalar_bits_sit_where_the_spec_says() {
        assert_eq!(V_LINK, 300);
        assert_eq!(scalar_bit_position(0), (60, 0));
        assert_eq!(scalar_bit_position(252), (38, 10));
        assert_eq!(scalar_bit_position(253), (37, 0));
        assert_eq!(scalar_bit_position(255), (37, 2));
        assert_eq!([253, 254, 255].map(committed_of_scalar_bit), [55, 56, 57]);
        assert_eq!(committed_of_scalar_bit(252), 58 + 10);
        for v in 0..COMMITTED {
            match scalar_bit(v) {
                None => assert!(v < FREE),
                Some(t) => assert_eq!(committed_of_scalar_bit(t), v),
            }
        }
        assert_eq!((FREE..COMMITTED).filter_map(scalar_bit).count(), 256);
    }

    #[test]
    fn metadata_round_trips() {
        let meta = Meta::new(index(3), 2).unwrap();
        assert_eq!(
            meta,
            Meta {
                threshold: 2,
                index: 3
            }
        );
        let meta = Meta::new(index(255), 255).unwrap();
        assert_eq!((meta.threshold, meta.index), (255, 255));
        // The index goes in the clear at the top of h0.
        let mut values = [0u16; COEFFS];
        pack(meta.index, &[0xff; 32], &mut values);
        assert_eq!(index_of(&values), 255);
    }

    #[test]
    fn metadata_out_of_range_is_refused() {
        assert_eq!(Meta::new(index(256), 2), Err(MetaError::MetadataRange));
        assert_eq!(Meta::new(index(1), 256), Err(MetaError::MetadataRange));
        assert_eq!(Meta::new(index(1), 0), Err(MetaError::MetadataRange));
        assert_eq!(Meta::new(index(2), 1), Err(MetaError::ThresholdOne));
        assert!(Meta::new(index(1), 1).is_ok());
        assert!(Meta::new(index(1), 255).is_ok());
        assert!(Meta::new(index(255), 2).is_ok());
    }

    #[test]
    fn packing_round_trips_and_puts_bits_in_place() {
        let mut rng = rand::thread_rng();
        for _ in 0..50 {
            let index: u8 = rng.gen();
            let scalar: [u8; 32] = rng.gen();
            let mut values = random_values(&mut rng);
            let before = values;
            pack(index, &scalar, &mut values);
            assert!(values.iter().all(|&c| c <= WORD_MASK));
            assert_eq!(
                values[..FIRST_SHARE_SLOT],
                before[..FIRST_SHARE_SLOT],
                "only h0..h23"
            );
            assert_eq!(unpack(&values), (index, scalar));
            assert_eq!(index_of(&values), index);
            for t in 0..256 {
                let (slot, g) = scalar_bit_position(t);
                let byte = scalar[31 - t / 8];
                assert_eq!(
                    (values[slot] >> g) & 1,
                    (byte >> (t % 8)) as u16 & 1,
                    "bit {t}"
                );
            }
        }
    }

    #[test]
    fn bits_round_trip() {
        let mut rng = rand::thread_rng();
        for _ in 0..20 {
            let values = random_values(&mut rng);
            let bits = committed_bits(&values);
            let (index, phone) = (index_of(&values), phone_of(&values));
            assert_eq!(values_from_bits(&bits, index, &phone), values);
            assert_eq!(words_from_bits(&bits, index, &phone), words_of(&values));
        }
        let mut b = Bits::default();
        b.set(310, true);
        b.set(3, true);
        b.set(3, false);
        assert_eq!(b.iter().collect::<Vec<_>>(), [310]);
    }

    #[test]
    fn lagrange_rows_interpolate() {
        let mut rng = rand::thread_rng();
        let values = random_values(&mut rng);
        let coeffs = interpolate(&BASIS, &values);
        let weights = basis_weights();
        for x in [
            1u16, 17, 32, 33, 64, 65, 255, 256, 1000, 2018, 2019, 2021, 2024, 2047,
        ] {
            let row = lagrange_row_with(&weights, x);
            assert_eq!(
                combine::<TableFree>(&row, &values),
                eval(&coeffs, x),
                "x = {x}"
            );
            assert_eq!(row, lagrange_row(x));
        }
        // At a basis point the row picks out its slot.
        for (slot, &x) in BASIS.iter().enumerate() {
            let row = lagrange_row(x);
            assert_eq!(row.iter().filter(|&&l| l != 0).count(), 1);
            assert_eq!(row[slot], 1);
        }
    }

    #[test]
    fn box_sets_match_evaluation() {
        let mut rng = rand::thread_rng();
        let sets: Vec<BoxSets> = DEVICE_HALF.map(BoxSets::new).collect();
        for _ in 0..20 {
            let values = random_values(&mut rng);
            let words = words_of(&values);
            let (bits, index, phone) = (
                committed_bits(&values),
                index_of(&values),
                phone_of(&values),
            );
            for (j, sets) in sets.iter().enumerate() {
                assert_eq!(
                    sets.word(&bits) ^ sets.public_word(index, &phone),
                    words[j],
                    "word {j}"
                );
            }
            // The phone half is P itself.
            assert_eq!(words[PHONE_HALF], phone);
        }
    }

    #[test]
    fn box_set_sizes_need_seven_carry_bits() {
        let sizes: Vec<usize> = DEVICE_HALF
            .flat_map(|j| BoxSets::new(j).committed.map(|t| t.len()))
            .collect();
        let min = *sizes.iter().min().unwrap();
        let max = *sizes.iter().max().unwrap();
        let mean = sizes.iter().sum::<usize>() as f64 / sizes.len() as f64;
        assert_eq!((min, max), (138, MAX_SET));
        assert!((mean - 156.89).abs() < 0.01, "mean {mean}");
        assert!(max / 2 < 1 << CARRY_BITS);
        assert_eq!(CARRIES_PER_BOX, 77);
    }

    #[test]
    fn word_sets_are_dense() {
        // Every device word is computed: each output bit sees most of the share and the hidden
        // random values. Phone words have no sets (see `hidden_points_are_unreachable`).
        for j in DEVICE_HALF {
            let sets = BoxSets::new(j);
            for t in &sets.committed {
                assert!(t.len() >= 128, "word {j}: |T| = {}", t.len());
                assert!(t.iter().any(|v| v < FREE), "word {j} sees the free bits");
            }
            assert!(sets.lagrange.iter().all(|&l| l != 0), "word {j}");
        }
    }

    #[test]
    fn hidden_points_are_unreachable() {
        const { assert!(RANDOM_HIDDEN_FIRST > 256) };
        assert_eq!(
            (hidden_point(0), hidden_point(HIDDEN - 1)),
            (2019, 2047),
            "29 hidden points"
        );
        assert_eq!(
            (RANDOM_HIDDEN_FIRST, HIDDEN_FIRST),
            (2019, 2024),
            "first random and first secret hidden point (spec PK.1)"
        );
        assert_eq!(hidden_point(RANDOM_HIDDEN), HIDDEN_FIRST, "h0");
        // Every word index a u8 can carry: 0..64 name a word point outside the hidden points,
        // and 64..=255 are refused rather than evaluated.
        for j in 0..WORDS {
            assert!(!(2019..=2047).contains(&word_point(j)), "word {j}");
        }
        for j in WORDS..=u8::MAX as usize {
            assert!(
                std::panic::catch_unwind(|| word_point(j)).is_err(),
                "word index {j} must be refused"
            );
        }
        let mut sorted = BASIS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), COEFFS, "BASIS is distinct");
        for (slot, &x) in BASIS.iter().enumerate() {
            match hidden_index(slot) {
                Some(m) => assert_eq!(x, hidden_point(m)),
                None => assert!((33..=64).contains(&x)),
            }
        }
        let panics = |f: fn()| std::panic::catch_unwind(f).is_err();
        assert!(panics(|| {
            word_point(WORDS);
        }));
        assert!(panics(|| {
            hidden_point(HIDDEN);
        }));
        assert!(panics(|| {
            BoxSets::new(WORDS);
        }));
        assert!(panics(|| {
            BoxSets::new(HALF);
        }));
        assert!(panics(|| {
            BoxSets::new(usize::MAX);
        }));
    }

    /// A run of points as FMT_C writes it: "a..b" for three or more in a row, else a list.
    fn points(xs: impl IntoIterator<Item = u16>) -> String {
        let xs: Vec<u16> = xs.into_iter().collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < xs.len() {
            let mut end = i;
            while end + 1 < xs.len() && xs[end + 1] == xs[end] + 1 {
                end += 1;
            }
            if end - i >= 2 {
                out.push(alloc::format!("{}..{}", xs[i], xs[end]));
            } else {
                out.extend(xs[i..=end].iter().map(|x| alloc::format!("{x}")));
            }
            i = end + 1;
        }
        out.join(",")
    }

    #[test]
    fn fmt_c_matches_the_constants() {
        let fmt = core::str::from_utf8(FMT_C).unwrap();
        let field = |name: &str| {
            fmt.split('|')
                .find_map(|f| f.strip_prefix(&alloc::format!("{name}=")))
                .unwrap_or_else(|| panic!("no {name}="))
        };
        // The random values: the 5 hidden ones (the threshold word among them) and P.
        assert_eq!(
            field("rand"),
            points((0..FIRST_SHARE_SLOT).map(|s| BASIS[s]))
        );
        assert_eq!(field("thr"), points([BASIS[THRESHOLD_SLOT]]));
        assert_eq!(
            field("hidden"),
            points(BASIS[FIRST_SHARE_SLOT..].iter().copied())
        );
        assert_eq!(field("self"), points(SELF_MASK_SLOTS.map(|s| BASIS[s])));
        assert_eq!(field("wallet"), points(WALLET_MASK_SLOTS.map(|s| BASIS[s])));
        assert_eq!(field("n"), alloc::format!("{WORDS}"));
        assert_eq!(field("k"), alloc::format!("{COEFFS}"));
        assert_eq!(field("D"), alloc::format!("words1-{HALF}"));
        assert_eq!(field("phone"), alloc::format!("words{}-{WORDS}", HALF + 1));
        assert_eq!(field("poly"), "0x805");
        assert_eq!(field("x"), "word");
        assert_eq!(field("h"), "idx8+scalar256-be");
        // The masks are every random slot but the threshold's, self first.
        let mut all: Vec<usize> = SELF_MASK_SLOTS.to_vec();
        all.extend(WALLET_MASK_SLOTS);
        assert_eq!(all, MASK_SLOTS);
        assert_eq!(MASK_SLOTS.len() + 1, FIRST_SHARE_SLOT);
        assert!(!MASK_SLOTS.contains(&THRESHOLD_SLOT));
        assert_eq!(layout_hash(), layout_hash_of(FMT_C));
    }

    /// The 11 bits of every random value (slots 0..37) are the masking bits: the rows of the
    /// words' bits over those 407 bits.
    type Row = [u64; (FIRST_SHARE_SLOT * WORD_BITS).div_ceil(64)];
    const RANDOM_BITS: usize = FIRST_SHARE_SLOT * WORD_BITS;

    /// GF(2) rank of rows over the random bits.
    fn rank(mut rows: Vec<Row>) -> usize {
        let mut rank = 0;
        for bit in 0..RANDOM_BITS {
            let has = |row: &Row| (row[bit / 64] >> (bit % 64)) & 1 == 1;
            let Some(pivot) = (rank..rows.len()).find(|&r| has(&rows[r])) else {
                continue;
            };
            rows.swap(rank, pivot);
            let pivot = rows[rank];
            for (r, row) in rows.iter_mut().enumerate() {
                if r != rank && has(row) {
                    row.iter_mut().zip(&pivot).for_each(|(a, b)| *a ^= b);
                }
            }
            rank += 1;
        }
        rank
    }

    /// For each word, its 11 output bits as rows over the random bits: bit β of ℓ_k(x_j)·x^g
    /// for every random slot k and bit g.
    fn random_rows() -> Vec<[Row; WORD_BITS]> {
        let weights = basis_weights();
        (0..WORDS)
            .map(|j| {
                let row = lagrange_row_with(&weights, word_point(j));
                core::array::from_fn(|beta| {
                    let mut out = Row::default();
                    for (slot, &l) in row[..FIRST_SHARE_SLOT].iter().enumerate() {
                        for g in 0..WORD_BITS {
                            if (gf::mul(l, 1 << g) >> beta) & 1 == 1 {
                                let bit = slot * WORD_BITS + g;
                                out[bit / 64] |= 1 << (bit % 64);
                            }
                        }
                    }
                    out
                })
            })
            .collect()
    }

    fn rows_of(all: &[[Row; WORD_BITS]], words: &[usize]) -> Vec<Row> {
        words.iter().flat_map(|&j| all[j]).collect()
    }

    /// Any 37 words are masked: what they show is independent of the share.
    ///
    /// The argument: fix the 24 share values and the 37 words at positions J. That's 61 points
    /// of a polynomial of degree ≤ 60, so f is fixed, and with it the 37 random values. So the
    /// map from the 37 random values to the 37 words is one-to-one for every share, and as a
    /// GF(2)-linear map from the 407 random bits to the 407 word bits it has full rank. With the
    /// masks uniform (they are hash outputs), the words are uniform whatever the share is. 38
    /// words give 418 output bits for at most rank 407, so 11 GF(2) relations fix bits of the
    /// share.
    ///
    /// The test checks the rank directly: the two sets a screen sees (its own half plus 5 words
    /// of the other half), and random sets.
    #[test]
    fn any_37_words_are_masked() {
        let all = random_rows();
        let mut rng = rand::thread_rng();
        let device: Vec<usize> = DEVICE_HALF.collect();
        let phone: Vec<usize> = PHONE_HALF.collect();
        let mut cases: Vec<Vec<usize>> = Vec::new();
        // The device screen: words 1-32 and 5 of words 33-64; the phone: the other way round.
        for (own, other) in [(&device, &phone), (&phone, &device)] {
            cases.push(own.iter().chain(&other[..5]).copied().collect());
            cases.push(own.iter().chain(&other[27..]).copied().collect());
            for _ in 0..15 {
                let five = other.choose_multiple(&mut rng, 5).copied();
                cases.push(own.iter().copied().chain(five).collect());
            }
        }
        let every: Vec<usize> = (0..WORDS).collect();
        for _ in 0..10 {
            cases.push(every.choose_multiple(&mut rng, 37).copied().collect());
        }
        cases.push((0..37).collect());
        cases.push((WORDS - 37..WORDS).collect());
        for words in cases {
            assert_eq!(words.len(), 37);
            assert_eq!(rank(rows_of(&all, &words)), RANDOM_BITS, "{words:?}");
        }
        assert_eq!(RANDOM_BITS, 407);
    }

    /// Reduces `row` by an echelon basis of (pivot bit, row), each row reduced by those before.
    fn reduce(mut row: Row, basis: &[(usize, Row)]) -> Row {
        for (bit, b) in basis {
            if (row[bit / 64] >> (bit % 64)) & 1 == 1 {
                row.iter_mut().zip(b).for_each(|(x, y)| *x ^= y);
            }
        }
        row
    }

    /// Every set a screen sees, its own half plus any 5 words of the other: 2 × C(32, 5) =
    /// 402,752 sets. The interpolation argument covers them all; this checks each one. About
    /// 10 s in release.
    #[test]
    #[ignore = "slow: run in release with --ignored"]
    fn every_screen_set_is_masked() {
        let all = random_rows();
        for (own, other) in [(DEVICE_HALF, PHONE_HALF), (PHONE_HALF, DEVICE_HALF)] {
            let own: Vec<usize> = own.collect();
            let mut basis: Vec<(usize, Row)> = Vec::new();
            for row in rows_of(&all, &own) {
                let row = reduce(row, &basis);
                if let Some(bit) =
                    (0..RANDOM_BITS).find(|&bit| (row[bit / 64] >> (bit % 64)) & 1 == 1)
                {
                    basis.push((bit, row));
                }
            }
            let other: Vec<Vec<Row>> = other
                .map(|j| all[j].iter().map(|&row| reduce(row, &basis)).collect())
                .collect();
            let mut five = [0, 1, 2, 3, 4];
            let mut count = 0;
            loop {
                let rows = five
                    .iter()
                    .flat_map(|&i| other[i].iter().copied())
                    .collect();
                assert_eq!(basis.len() + rank(rows), RANDOM_BITS, "{five:?}");
                count += 1;
                // The next 5-subset of 0..32 in lexicographic order.
                let Some(i) = (0..5).rev().find(|&i| five[i] < HALF - 5 + i) else {
                    break;
                };
                five[i] += 1;
                for k in i + 1..5 {
                    five[k] = five[k - 1] + 1;
                }
            }
            assert_eq!(count, 201_376);
        }
    }

    #[test]
    fn any_38_words_leak() {
        let all = random_rows();
        let mut rng = rand::thread_rng();
        let every: Vec<usize> = (0..WORDS).collect();
        for _ in 0..5 {
            let words: Vec<usize> = every.choose_multiple(&mut rng, 38).copied().collect();
            let rows = rows_of(&all, &words);
            // 418 output bits, rank at most 407: 11 relations on the share's bits are left.
            assert_eq!(rows.len() - rank(rows), 38 * WORD_BITS - RANDOM_BITS);
            assert_eq!(38 * WORD_BITS - RANDOM_BITS, 11);
        }
    }

    #[test]
    fn halves_split_the_card() {
        assert_eq!((HALF, DEVICE_HALF, PHONE_HALF), (32, 0..32, 32..64));
        assert_eq!(DEVICE_HALF.len() + PHONE_HALF.len(), WORDS);
        assert_eq!(
            &BASIS[FIRST_PHONE_SLOT..FIRST_SHARE_SLOT],
            &(33..=64).collect::<Vec<u16>>()[..]
        );
    }

    #[test]
    fn phone_half_packs_into_44_bytes() {
        let mut rng = rand::thread_rng();
        assert_eq!(HALF_BYTES, 44);
        for _ in 0..20 {
            let half: [u16; HALF] = core::array::from_fn(|_| rng.gen_range(0..2048));
            let bytes = pack_half(&half);
            assert_eq!(unpack_half(&bytes), half);
        }
        // Most significant first: word 33 = 2047 fills the first 11 bits.
        let mut half = [0u16; HALF];
        half[0] = 0x7ff;
        half[31] = 1;
        let bytes = pack_half(&half);
        assert_eq!((bytes[0], bytes[1], bytes[43]), (0xff, 0xe0, 0x01));
    }
}
