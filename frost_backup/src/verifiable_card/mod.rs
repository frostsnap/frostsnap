//! Experimental: the verifiable 64-word card, format C′ (the packed layout).
//!
//! A card holds a share as a polynomial f of degree at most 60 over GF(2^11), written as its
//! values at 64 words, so 3 words are parity. C′ describes f by 61 values rather than 61
//! coefficients: 37 random values derived from the share (5 at hidden points, the threshold word
//! among them, and the 32 words of the phone half), and 24 hidden values that hold the share
//! index and the scalar. No word ever shows a hidden point, and any 37 words reveal nothing
//! about the share. Words 1-32 are the device half, all computed from the values; words 33-64
//! are the phone half P, random values that the device sends to the phone. The device commits
//! to the 311 bits the proofs bind (the 55 bits of its hidden random values and the 256 scalar
//! bits), so the phone can check words it opens against the share image; the index and P are
//! public. The seal binds P by its hash hP, so the phone half needs no openings: the phone
//! checks every P it gets against the pinned seal, and every word the phone round opens is a
//! computed device word.
//!
//! [`layout`] has where every bit sits and the bit sets the proofs use; [`codec`] encodes,
//! decodes and repairs cards; [`proof`] has the seal, the bit proofs and the openings; [`jobs`]
//! cuts the seal and the start of an opening into device-sized steps; [`phone`] has the
//! phone's owned types for gathering and checking what the device sends; [`rng`] has the
//! domain-separated child streams the quizzes and the device round draw from. It is
//! experimental and not a stable format (spec PK-O3).

pub mod codec;
pub mod jobs;
pub mod layout;
#[cfg(feature = "offline-repair")]
pub mod offline;
pub mod phone;
pub mod proof;
pub mod rng;
