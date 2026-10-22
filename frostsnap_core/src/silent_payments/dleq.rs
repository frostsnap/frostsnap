use schnorr_fun::fun::{g, marker::*, s, Point, Scalar, G};
use sha2::Digest;

use super::tagged;

fn xor_32(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (out, (a, b)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *out = a ^ b;
    }
    out
}

/// BIP374 challenge `e`. `m` is the optional message, empty when absent.
fn dleq_challenge(
    a: Point,
    b: Point,
    c: Point,
    r1: Point,
    r2: Point,
    m: Option<&[u8; 32]>,
) -> Scalar<Public, Zero> {
    let mut hash = tagged("BIP0374/challenge")
        .chain_update(a.to_bytes())
        .chain_update(b.to_bytes())
        .chain_update(c.to_bytes())
        .chain_update(G.normalize().to_bytes())
        .chain_update(r1.to_bytes())
        .chain_update(r2.to_bytes());
    if let Some(m) = m {
        hash = hash.chain_update(m);
    }
    Scalar::from_bytes_mod_order(hash.finalize().into())
}

/// BIP374 `GenerateProof` with the generator fixed to `G`: returns a 64-byte proof `e || s` that
/// `a * G` and `c = a * b` share the secret `a`, along with `c`. `r` is fresh auxiliary randomness
/// and `m` an optional 32-byte message.
///
/// Unlike the spec, this does not verify the proof before returning it, callers must run
/// [`dleq_verify`] if they do not trust the prover.
pub fn dleq_prove(
    a: Scalar,
    b: Point,
    r: &[u8; 32],
    m: Option<&[u8; 32]>,
) -> Option<([u8; 64], Point)> {
    let a_point = g!(a * G).normalize().non_zero()?;
    let c = g!(a * b).normalize().non_zero()?;

    let t = xor_32(
        a.to_bytes(),
        tagged("BIP0374/aux").chain_update(r).finalize().into(),
    );
    let mut rand = tagged("BIP0374/nonce")
        .chain_update(t)
        .chain_update(a_point.to_bytes())
        .chain_update(c.to_bytes());
    if let Some(m) = m {
        rand = rand.chain_update(m);
    }
    let k = Scalar::<Secret, Zero>::from_bytes_mod_order(rand.finalize().into()).non_zero()?;

    let r1 = g!(k * G).normalize().non_zero()?;
    let r2 = g!(k * b).normalize().non_zero()?;
    let e = dleq_challenge(a_point, b, c, r1, r2, m);
    let s = s!(k + e * a);

    let mut proof = [0u8; 64];
    proof[..32].copy_from_slice(&e.to_bytes());
    proof[32..].copy_from_slice(&s.to_bytes());

    Some((proof, c))
}

/// BIP374 `VerifyProof` with the generator fixed to `G`: checks that `a` and `c` are `x * G` and
/// `x * b` for one `x`.
pub fn dleq_verify(a: Point, b: Point, c: Point, proof: &[u8; 64], m: Option<&[u8; 32]>) -> bool {
    let (Some(e), Some(s)) = (
        Scalar::<Public, Zero>::from_slice(&proof[..32]),
        Scalar::<Public, Zero>::from_slice(&proof[32..]),
    ) else {
        return false;
    };
    let Some(r1) = g!(s * G - e * a).normalize().non_zero() else {
        return false;
    };
    let Some(r2) = g!(s * b - e * c).normalize().non_zero() else {
        return false;
    };
    e == dleq_challenge(a, b, c, r1, r2, m)
}
