//! Experimental: the reveal-check backup card.
//!
//! A card holds the 24 key words of today's backup plus `pad` random pad words as the
//! coefficients of a polynomial over GF(2^11), and is written as that polynomial's values
//! at `24 + pad + parity` points. Any `24 + pad` words rebuild the card, and a device can
//! open up to `pad` words without revealing anything about the key.
//!
//! This module has the code, the repair search restore would run against a lying device,
//! and the model that turns measured search speed into catch rates. It exists to benchmark
//! designs on a device and is not a stable format.

pub mod card;
pub mod gf;
pub mod model;
pub mod search;
