//! Silent payment sending (BIP352) and the BIP374 DLEQ proofs that let every party check a share
//! of the ECDH before anything is signed with it.
//!
//! Everything here is a pure function over points and scalars, except [`prove_ecdh_share`], which
//! draws the proof's auxiliary randomness from the caller's RNG. Frostsnap's threshold ECDH works
//! on account key shares `x_j`: each party publishes `x_j * B_scan` with a DLEQ proof against its
//! share image, and [`combine_ecdh_shares`] turns any threshold of those into the same point the
//! sender would compute from the summed input secret.

mod address;
mod dleq;
mod template;

pub use dleq::{dleq_prove, dleq_verify};

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use bitcoin::{hashes::Hash, OutPoint};
use rand_core::RngCore;
use schnorr_fun::{
    frost::ShareIndex,
    fun::{g, hash::Tag, marker::*, poly, s, Point, Scalar, G},
};
use sha2::{Digest, Sha256};

use crate::tweak::TweakContext;

fn tagged(tag: &str) -> Sha256 {
    Sha256::default().tag(tag.as_bytes())
}

/// The lexicographically smallest outpoint, compared as its 36 serialized bytes, which is the
/// outpoint BIP352 hashes into `input_hash`.
pub fn smallest_outpoint(outpoints: &[OutPoint]) -> Option<OutPoint> {
    outpoints
        .iter()
        .min_by_key(|outpoint| (outpoint.txid.to_byte_array(), outpoint.vout.to_le_bytes()))
        .copied()
}

/// BIP352 `input_hash`, binding the lexicographically smallest outpoint of the transaction to the
/// sum of the input public keys.
pub fn input_hash(smallest_outpoint: &OutPoint, a_sum_public: Point) -> Option<Scalar<Public>> {
    let hash = tagged("BIP0352/Inputs")
        .chain_update(smallest_outpoint.txid.as_byte_array())
        .chain_update(smallest_outpoint.vout.to_le_bytes())
        .chain_update(a_sum_public.to_bytes())
        .finalize();
    Scalar::from_bytes(hash.into())?.non_zero()
}

/// BIP352 `t_k`, the tweak of the `k`th output to a scan key given that key's shared secret
/// `ecdh = input_hash * a_sum * B_scan`.
fn shared_secret_tweak(ecdh: Point, k: u32) -> Option<Scalar<Public>> {
    let hash = tagged("BIP0352/SharedSecret")
        .chain_update(ecdh.to_bytes())
        .chain_update(k.to_be_bytes())
        .finalize();
    Scalar::from_bytes(hash.into())?.non_zero()
}

/// The sums `Gx` and `T` over `inputs`, where the summed input secret is `a_sum = Gx * x + T`.
fn input_sums(inputs: &[TweakContext]) -> (Scalar<Public, Zero>, Scalar<Public, Zero>) {
    let gx = inputs
        .iter()
        .fold(Scalar::<Public, Zero>::zero(), |acc, input| {
            s!(acc + input.gv * input.gacc).public()
        });
    let t = inputs
        .iter()
        .fold(Scalar::<Public, Zero>::zero(), |acc, input| {
            s!(acc + input.gv * input.tacc).public()
        });
    (gx, t)
}

/// `a_sum * G` for the summed input secret, given the public key `x * G` of the account the inputs
/// are relative to.
pub fn a_sum_public(account_key: Point, inputs: &[TweakContext]) -> Option<Point> {
    let (gx, t) = input_sums(inputs);
    g!(gx * account_key + t * G).normalize().non_zero()
}

/// Combines the ECDH shares of a threshold of account key shares into the BIP352 shared secret
/// `input_hash * a_sum * B_scan`, where `a_sum` is the sum of the input secrets that `inputs`
/// describe.
///
/// Each entry of `shares` is a share index and `x_j * B_scan` for that share's account key share
/// `x_j`. The account secret is the Lagrange combination of the shares, so the result does not
/// depend on which threshold subset was used.
pub fn combine_ecdh_shares(
    input_hash: Scalar<Public>,
    b_scan: Point,
    inputs: &[TweakContext],
    shares: &[(ShareIndex, Point)],
) -> Option<Point> {
    let indices = shares.iter().map(|(index, _)| *index).collect::<Vec<_>>();
    let distinct = indices.iter().collect::<BTreeSet<_>>();
    if indices.is_empty() || distinct.len() != indices.len() {
        return None;
    }

    let (gx, t) = input_sums(inputs);

    // x * B_scan = sum of lambda_j * (x_j * B_scan)
    let x_b_scan = shares.iter().fold(
        Point::<Normal, Public, Zero>::zero(),
        |acc, &(index, share)| {
            let lambda = poly::eval_basis_poly_at_0(index, indices.iter().copied());
            g!(acc + lambda * share).normalize()
        },
    );

    let a_sum_b_scan = g!(gx * x_b_scan + t * b_scan).normalize();
    g!(input_hash * a_sum_b_scan).normalize().non_zero()
}

/// One party's share of the ECDH for a scan key: `x_j * scan_key`, with a BIP374 proof that it uses
/// the same account key share `x_j` as the party's share image `x_j * G`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct EcdhShare {
    pub scan_key: Point,
    pub share_index: ShareIndex,
    pub ecdh_share: Point,
    pub proof: [u8; 64],
}

/// The share of `account_share`, the account key share at `share_index`, for `scan_key`, with its
/// proof.
///
/// We use the transaction's `input_hash` as the DLEQ message so a proof made for another
/// transaction does not verify. BIP374 leaves the message to the application: binding it here
/// ties each proof to one send. Prover and verifier must pass the same value.
pub fn prove_ecdh_share(
    share_index: ShareIndex,
    account_share: Scalar,
    scan_key: Point,
    input_hash: Scalar<Public>,
    rng: &mut impl RngCore,
) -> Option<EcdhShare> {
    let mut aux_rand = [0u8; 32];
    rng.fill_bytes(&mut aux_rand);
    let (proof, ecdh_share) = dleq_prove(
        account_share,
        scan_key,
        &aux_rand,
        Some(&input_hash.to_bytes()),
    )?;
    Some(EcdhShare {
        scan_key,
        share_index,
        ecdh_share,
        proof,
    })
}

/// Whether `share` was made with the account key share behind `image`, the share image
/// `x_j * G`, for the transaction with `input_hash`, the same value the prover used as the DLEQ
/// message.
pub fn verify_ecdh_share(
    image: Point<Normal, Public, Zero>,
    input_hash: Scalar<Public>,
    share: &EcdhShare,
) -> bool {
    let Some(image) = image.non_zero() else {
        return false;
    };
    dleq_verify(
        image,
        share.scan_key,
        share.ecdh_share,
        &share.proof,
        Some(&input_hash.to_bytes()),
    )
}

/// A silent payment recipient, identified by the scan and spend keys in its address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, bincode::Encode, bincode::Decode)]
pub struct SilentPaymentRecipient {
    pub scan_key: Point,
    pub spend_key: Point,
}

/// Limit on silent payment outputs to one scan key within a transaction (BIP352 `K_max`).
const K_MAX: u32 = 2323;

/// BIP352 output keys for `recipients`, in the order given, as x-only public keys.
///
/// `shared_secrets` maps each scan key to its shared secret, see [`combine_ecdh_shares`]. Outputs
/// to one scan key are numbered by `k` in recipient order, so the order of recipients to a scan
/// key matters. Returns `None` if a scan key has no shared secret, a group exceeds `K_MAX`, or a
/// tweak is invalid.
pub fn output_keys(
    recipients: &[SilentPaymentRecipient],
    shared_secrets: &BTreeMap<Point, Point>,
) -> Option<Vec<[u8; 32]>> {
    let mut next_k = BTreeMap::<Point, u32>::new();
    recipients
        .iter()
        .map(|recipient| {
            let k = next_k.entry(recipient.scan_key).or_default();
            if *k >= K_MAX {
                return None;
            }
            let t = shared_secret_tweak(*shared_secrets.get(&recipient.scan_key)?, *k)?;
            *k += 1;
            let output = g!(recipient.spend_key + t * G).normalize().non_zero()?;
            Some(output.to_xonly_bytes())
        })
        .collect()
}
