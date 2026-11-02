//! The card: an evaluation-form Reed–Solomon code over GF(2^11).
//!
//! The polynomial's coefficients are the pad words (lowest) then the 24 key words. Card
//! word `j` is the polynomial evaluated at [`point`]`(j)`.

use super::gf;
use alloc::vec::Vec;

/// Words that hold the 256-bit share and its 8-bit polynomial checksum.
pub const KEY_WORDS: usize = 24;

/// How many pad and parity words a card adds to the key words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Design {
    pub pad: usize,
    pub parity: usize,
}

impl Design {
    pub const fn new(pad: usize, parity: usize) -> Self {
        Self { pad, parity }
    }

    /// Words written on the card.
    pub const fn n(&self) -> usize {
        KEY_WORDS + self.pad + self.parity
    }

    /// Polynomial coefficients. Any `k()` card words rebuild the whole card.
    pub const fn k(&self) -> usize {
        KEY_WORDS + self.pad
    }
}

impl core::fmt::Display for Design {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}", self.pad, self.parity)
    }
}

/// The evaluation point of card word `j`: distinct and nonzero for every word.
pub const fn point(j: usize) -> u16 {
    (j + 1) as u16
}

/// Evaluates the polynomial with these coefficients (lowest first) at `x`.
pub fn eval(coeffs: &[u16], x: u16) -> u16 {
    coeffs.iter().rev().fold(0, |acc, &c| gf::mul(acc, x) ^ c)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub design: Design,
    /// Pad words then key words.
    pub coeffs: Vec<u16>,
    /// What is written on the card.
    pub words: Vec<u16>,
}

impl Card {
    pub fn new(design: Design, pad: &[u16], key: &[u16; KEY_WORDS]) -> Self {
        assert_eq!(pad.len(), design.pad);
        let coeffs: Vec<u16> = pad.iter().chain(key.iter()).copied().collect();
        let words = (0..design.n()).map(|j| eval(&coeffs, point(j))).collect();
        Self {
            design,
            coeffs,
            words,
        }
    }

    pub fn key(&self) -> &[u16] {
        &self.coeffs[self.design.pad..]
    }
}

/// Splits 24 key words into the 32-byte scalar and the 8-bit polynomial checksum. The key
/// words are the first 24 words of today's backup ([`crate::ShareBackup::to_word_indices`]).
pub fn unpack_key(key: &[u16; KEY_WORDS]) -> ([u8; 32], u8) {
    let mut bytes = [0u8; 33];
    let mut acc: u32 = 0;
    let mut bits = 0;
    let mut out = 0;
    for &word in key {
        acc = (acc << 11) | (word as u32 & 0x7ff);
        bits += 11;
        while bits >= 8 {
            bits -= 8;
            bytes[out] = (acc >> bits) as u8;
            out += 1;
        }
        acc &= (1 << bits) - 1;
    }
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&bytes[..32]);
    (scalar, bytes[32])
}

/// The Lagrange basis for one set of distinct evaluation points.
///
/// Allocations are kept between [`Basis::reset`] calls so the repair search can reuse one.
///
/// Its arithmetic has no branch on a value ([`gf::mul_ct`]): a restore's rebuild leaves out
/// words that depend on the user's typos, so the points are public but which ones are used
/// isn't, and a zero shortcut would make the work follow them (review B15 secrecy S-1).
#[derive(Clone, Debug, Default)]
pub struct Basis {
    xs: Vec<u16>,
    /// Coefficients of M(x) = Π (x + xs[j]), lowest first.
    master: Vec<u16>,
    quotient: Vec<u16>,
    /// Table operations done, so a test can check that they don't depend on the points.
    pub table_ops: u64,
}

impl Basis {
    pub fn new(xs: &[u16]) -> Self {
        let mut basis = Self::default();
        basis.reset(xs);
        basis
    }

    pub fn reset(&mut self, xs: &[u16]) {
        self.xs.clear();
        self.xs.extend_from_slice(xs);
        self.master.clear();
        self.master.push(1);
        for &x in xs {
            self.master.push(0);
            for i in (1..self.master.len()).rev() {
                self.master[i] = self.master[i - 1] ^ gf::mul_ct(x, self.master[i]);
            }
            self.master[0] = gf::mul_ct(x, self.master[0]);
            self.table_ops += self.master.len() as u64;
        }
        self.quotient.clear();
        self.quotient.resize(xs.len(), 0);
    }

    /// Adds `y·ℓ_j` to `out`, where `ℓ_j` is 1 at point `j` and 0 at the others.
    pub fn add_scaled(&mut self, j: usize, y: u16, out: &mut [u16]) {
        let k = self.xs.len();
        if y == 0 || k == 0 {
            return;
        }
        let a = self.xs[j];
        // M(x) / (x + a), by synthetic division from the top.
        let q = &mut self.quotient;
        q[k - 1] = self.master[k];
        for i in (1..k).rev() {
            q[i - 1] = self.master[i] ^ gf::mul_ct(a, q[i]);
        }
        let at_a = q.iter().rev().fold(0, |acc, &c| gf::mul_ct(acc, a) ^ c);
        let weight = gf::mul_ct(y, gf::inv_ct(at_a));
        for (o, &c) in out.iter_mut().zip(q.iter()) {
            *o ^= gf::mul_ct(weight, c);
        }
        self.table_ops += 3 * k as u64 + 1;
    }
}

/// The coefficients (lowest first) of the polynomial through these points.
pub fn interpolate(xs: &[u16], ys: &[u16]) -> Vec<u16> {
    assert_eq!(xs.len(), ys.len());
    let mut basis = Basis::new(xs);
    let mut out = vec![0u16; xs.len()];
    for (j, &y) in ys.iter().enumerate() {
        basis.add_scaled(j, y, &mut out);
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Fingerprint, ShareBackup};
    use rand::{seq::SliceRandom, Rng};
    use schnorr_fun::fun::prelude::*;

    fn random_key(rng: &mut impl Rng) -> [u16; KEY_WORDS] {
        core::array::from_fn(|_| rng.gen_range(0..2048))
    }

    #[test]
    fn key_words_match_todays_backup() {
        let mut rng = rand::thread_rng();
        let (backups, shared_key) = ShareBackup::generate_shares(
            Scalar::random(&mut rng),
            2,
            20,
            Fingerprint::NONE,
            &mut rng,
        );
        for backup in backups {
            let words = backup.to_word_indices();
            let key: [u16; KEY_WORDS] = words[..KEY_WORDS].try_into().unwrap();
            let (scalar, checksum) = unpack_key(&key);
            let share = backup.extract_secret(&shared_key).unwrap();
            assert_eq!(scalar, share.share.to_bytes());
            assert_eq!(checksum as u16, words[23] & 0xff);
        }
    }

    #[test]
    fn any_k_words_rebuild_the_card() {
        let mut rng = rand::thread_rng();
        for design in [Design::new(4, 4), Design::new(6, 2), Design::new(0, 1)] {
            let pad: Vec<u16> = (0..design.pad).map(|_| rng.gen_range(0..2048)).collect();
            let card = Card::new(design, &pad, &random_key(&mut rng));
            for _ in 0..10 {
                let mut positions: Vec<usize> = (0..design.n()).collect();
                positions.shuffle(&mut rng);
                positions.truncate(design.k());
                let xs: Vec<u16> = positions.iter().map(|&j| point(j)).collect();
                let ys: Vec<u16> = positions.iter().map(|&j| card.words[j]).collect();
                assert_eq!(interpolate(&xs, &ys), card.coeffs);
            }
        }
    }
}
