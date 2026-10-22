//! Silent payment sending (BIP352) and the BIP374 DLEQ proofs that let every party check a share
//! of the ECDH before anything is signed with it.

mod dleq;

pub use dleq::{dleq_prove, dleq_verify};

use schnorr_fun::fun::hash::Tag;
use sha2::{Digest, Sha256};

fn tagged(tag: &str) -> Sha256 {
    Sha256::default().tag(tag.as_bytes())
}
