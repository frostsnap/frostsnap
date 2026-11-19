//! Randomness for the card's quizzes and device round (spec L1.4 rules 1–3 and 6).
//!
//! Every use draws from its own child stream: 32 bytes from a parent CSPRNG, hashed with the
//! use's tag and its context (the share image, the phase, the attempt, the word position, the
//! display id and the run), seed a ChaCha20Rng. Uniform choices use rejection sampling and
//! shuffles use Fisher–Yates. None of this is part of a hashed or transmitted format.

use super::{
    codec::wipe,
    proof::{Ops, Tagged},
};
use rand_chacha::ChaCha20Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};
use schnorr_fun::frost::ShareImage;

// DRAFT: frozen before the first release (spec PK-O3).
/// Use tags (L1.4 rule 1): TH("frostsnap/card/rng/<use>", 32 B from parent ‖ ctx).
pub mod tag {
    pub const SESSION: &[u8] = b"frostsnap/card/rng/session";
    pub const DR_E: &[u8] = b"frostsnap/card/rng/device-round-e";
    pub const DR_OPTIONS: &[u8] = b"frostsnap/card/rng/device-round-options";
    /// The option shuffle seed of a self-check quiz.
    pub const SELF_CHECK: &[u8] = b"frostsnap/card/rng/self-check";
}

/// The bytes of [`share_image_bytes`].
pub const SHARE_IMAGE_LEN: usize = 65;

/// The one encoding of a share image used by the card RNG: the index (32 B, big-endian
/// scalar) ‖ the compressed point (33 B).
pub fn share_image_bytes(img: &ShareImage) -> [u8; SHARE_IMAGE_LEN] {
    let mut bytes = [0u8; SHARE_IMAGE_LEN];
    bytes[..32].copy_from_slice(&img.index.to_bytes());
    bytes[32..].copy_from_slice(&img.image.to_bytes());
    bytes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    // 1-3 and 5 are reserved: an earlier draft's phases used them.
    DeviceRound = 4,
    SelfCheck = 6,
}

/// The bytes of [`RngCtx::to_bytes`].
pub const RNG_CTX_LEN: usize = 73;

/// What a child stream is for. Field use per phase: DeviceRound{pass = attempt, position = k};
/// SelfCheck{pass 0, position 0}.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RngCtx {
    /// [`share_image_bytes`]; zeros at restore.
    pub share_image: [u8; SHARE_IMAGE_LEN],
    pub phase: Phase,
    pub pass: u8,
    pub position: u8,
    pub display_id: u32,
    pub run: u8,
}

impl RngCtx {
    /// share_image (65) ‖ phase ‖ pass ‖ position ‖ BE32(display_id) ‖ run: 73 bytes.
    pub fn to_bytes(&self) -> [u8; RNG_CTX_LEN] {
        let mut bytes = [0u8; RNG_CTX_LEN];
        bytes[..65].copy_from_slice(&self.share_image);
        bytes[65] = self.phase as u8;
        bytes[66] = self.pass;
        bytes[67] = self.position;
        bytes[68..72].copy_from_slice(&self.display_id.to_be_bytes());
        bytes[72] = self.run;
        bytes
    }
}

/// ChaCha20Rng::from_seed(TH(tag, 32 bytes from `parent` ‖ ctx)).
pub fn child(tag: &[u8], parent: &mut (impl RngCore + CryptoRng), ctx: &RngCtx) -> ChaCha20Rng {
    let mut draw = [0u8; 32];
    parent.fill_bytes(&mut draw);
    let mut seed = Tagged::new(tag)
        .add(&draw)
        .add(&ctx.to_bytes())
        .finish(&mut Ops::default());
    let rng = ChaCha20Rng::from_seed(seed);
    wipe(&mut draw, [0; 32]);
    wipe(&mut seed, [0; 32]);
    rng
}

/// A uniform value below `n` (n ≥ 1), by rejection sampling: the top 2^32 mod n values of a u32
/// are drawn again. It always draws at least once, even for n = 1.
pub fn below(rng: &mut impl RngCore, n: u32) -> u32 {
    assert!(n >= 1, "below needs n ≥ 1");
    // 2^32 mod n, so the accepted values are a whole number of runs of n.
    let rejected = (u32::MAX - n + 1) % n;
    loop {
        let x = rng.next_u32();
        if x <= u32::MAX - rejected {
            return x % n;
        }
    }
}

/// Fisher–Yates with [`below`]: one draw for every element, the last one included, so the work
/// depends only on the length.
pub fn shuffle<T>(rng: &mut impl RngCore, xs: &mut [T]) {
    for i in (0..xs.len()).rev() {
        let j = below(rng, i as u32 + 1) as usize;
        xs.swap(i, j);
    }
}

/// K distinct values below `n`, in random order: a partial Fisher–Yates.
pub fn distinct<const K: usize>(rng: &mut impl RngCore, n: u8) -> [u8; K] {
    assert!(K <= n as usize, "distinct needs K ≤ n");
    let mut pool = [0u8; 256];
    for (i, value) in pool.iter_mut().enumerate().take(n as usize) {
        *value = i as u8;
    }
    let mut out = [0u8; K];
    for (i, value) in out.iter_mut().enumerate() {
        let j = i + below(rng, (n as usize - i) as u32) as usize;
        pool.swap(i, j);
        *value = pool[i];
    }
    wipe(&mut pool, [0; 256]);
    out
}
