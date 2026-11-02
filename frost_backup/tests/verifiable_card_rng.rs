//! Tests for the card's child streams and uniform draws (spec L1.4 rules 1–3 and 6).

use frost_backup::verifiable_card::rng::{
    self, below, child, distinct, share_image_bytes, shuffle, Phase, RngCtx,
};
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use schnorr_fun::{frost::ShareImage, fun::prelude::*};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Chi-square critical value at p = 0.001 (Wilson–Hilferty), for 36 degrees of freedom.
const CHI2_36_P001: f64 = 68.1;

fn rng(seed: u8) -> ChaCha20Rng {
    ChaCha20Rng::from_seed([seed; 32])
}

fn chi_square(counts: &[u64]) -> f64 {
    let total: u64 = counts.iter().sum();
    let expected = total as f64 / counts.len() as f64;
    counts
        .iter()
        .map(|&count| (count as f64 - expected).powi(2) / expected)
        .sum()
}

/// An rng that replays a script of u32s.
struct Script(Vec<u32>);

impl RngCore for Script {
    fn next_u32(&mut self) -> u32 {
        self.0.remove(0)
    }
    fn next_u64(&mut self) -> u64 {
        unreachable!()
    }
    fn fill_bytes(&mut self, _: &mut [u8]) {
        unreachable!()
    }
    fn try_fill_bytes(&mut self, _: &mut [u8]) -> Result<(), rand_core::Error> {
        unreachable!()
    }
}

// ---- child streams ----

fn ctx(phase: Phase) -> RngCtx {
    RngCtx {
        share_image: [0x5A; 65],
        phase,
        pass: 0,
        position: 0,
        display_id: 1,
        run: 0,
    }
}

fn stream(tag: &[u8], parent_seed: u8, ctx: &RngCtx) -> [u8; 64] {
    let mut out = [0u8; 64];
    child(tag, &mut rng(parent_seed), ctx).fill_bytes(&mut out);
    out
}

#[test]
fn child_streams_are_domain_separated() {
    use rng::tag::*;
    let base = ctx(Phase::DeviceRound);
    let first = stream(DR_OPTIONS, 7, &base);
    assert_eq!(
        stream(DR_OPTIONS, 7, &base),
        first,
        "same inputs, same stream"
    );
    assert_ne!(stream(DR_OPTIONS, 8, &base), first, "different parent");

    let tags = [SESSION, DR_E, DR_OPTIONS, SELF_CHECK];
    let by_tag: BTreeSet<[u8; 64]> = tags.iter().map(|tag| stream(tag, 7, &base)).collect();
    assert_eq!(by_tag.len(), tags.len());

    let ctxs = [
        base,
        RngCtx {
            share_image: [0x5B; 65],
            ..base
        },
        ctx(Phase::SelfCheck),
        RngCtx { pass: 1, ..base },
        RngCtx {
            position: 1,
            ..base
        },
        RngCtx {
            display_id: 2,
            ..base
        },
        RngCtx {
            display_id: 1 << 24,
            ..base
        },
        RngCtx { run: 1, ..base },
    ];
    let by_ctx: BTreeSet<[u8; 64]> = ctxs.iter().map(|c| stream(DR_OPTIONS, 7, c)).collect();
    assert_eq!(by_ctx.len(), ctxs.len());
}

#[test]
fn rng_ctx_layout() {
    let ctx = RngCtx {
        share_image: core::array::from_fn(|i| i as u8),
        phase: Phase::SelfCheck,
        pass: 0xA1,
        position: 0xB2,
        display_id: 0x0102_0304,
        run: 0xC3,
    };
    let bytes = ctx.to_bytes();
    assert_eq!(bytes.len(), 73);
    assert_eq!(&bytes[..65], &ctx.share_image);
    assert_eq!(bytes[65], 6);
    assert_eq!(bytes[66], 0xA1);
    assert_eq!(bytes[67], 0xB2);
    assert_eq!(&bytes[68..72], &[1, 2, 3, 4]);
    assert_eq!(bytes[72], 0xC3);
    // The phase values are pinned: 1-3 and 5 stay reserved.
    assert_eq!(
        [Phase::DeviceRound, Phase::SelfCheck].map(|p| p as u8),
        [4, 6]
    );
}

#[test]
fn child_is_th_of_parent_bytes_and_ctx() {
    // TH(tag, m) = SHA256(SHA256(tag) ‖ SHA256(tag) ‖ m), from the spec, not the crate.
    let base = ctx(Phase::SelfCheck);
    let mut parent = rng(9);
    let mut draw = [0u8; 32];
    parent.fill_bytes(&mut draw);
    let tag_hash = Sha256::digest(rng::tag::SELF_CHECK);
    let seed: [u8; 32] = Sha256::new()
        .chain_update(tag_hash)
        .chain_update(tag_hash)
        .chain_update(draw)
        .chain_update(base.to_bytes())
        .finalize()
        .into();
    let mut expected = [0u8; 64];
    ChaCha20Rng::from_seed(seed).fill_bytes(&mut expected);
    assert_eq!(stream(rng::tag::SELF_CHECK, 9, &base), expected);
}

#[test]
fn share_image_bytes_layout() {
    let index = Scalar::<Public, Zero>::from(3u32).non_zero().unwrap();
    let point = g!(index * G).normalize().mark_zero();
    let img = ShareImage {
        index,
        image: point,
    };
    let bytes = share_image_bytes(&img);
    assert_eq!(&bytes[..32], &index.to_bytes());
    assert_eq!(&bytes[32..], &point.to_bytes());
    assert_eq!(bytes[31], 3);
}

// ---- 8. below ----

#[test]
fn below_is_uniform() {
    let mut rng = rng(8);
    let mut counts = [0u64; 37];
    for _ in 0..100_000 {
        counts[below(&mut rng, 37) as usize] += 1;
    }
    let chi = chi_square(&counts);
    assert!(chi < CHI2_36_P001, "chi-square {chi}");
}

#[test]
fn below_rejects_the_biased_tail() {
    // 2^32 mod 3 = 1, so u32::MAX is drawn again; 2^32 mod 80 = 16.
    let mut script = Script(vec![u32::MAX, 5]);
    assert_eq!(below(&mut script, 3), 2);
    assert!(script.0.is_empty());
    let mut script = Script(vec![u32::MAX - 15, u32::MAX - 16]);
    assert_eq!(below(&mut script, 80), (u32::MAX - 16) % 80);
    assert!(script.0.is_empty());
    // Powers of two and n = 1 reject nothing, but still draw.
    let mut script = Script(vec![u32::MAX]);
    assert_eq!(below(&mut script, 32), 31);
    let mut script = Script(vec![u32::MAX]);
    assert_eq!(below(&mut script, 1), 0);
    assert!(script.0.is_empty());
}

#[test]
fn shuffle_and_distinct() {
    let mut rng = rng(10);
    let mut xs: Vec<u16> = (0..100).collect();
    shuffle(&mut rng, &mut xs);
    let mut sorted = xs.clone();
    sorted.sort();
    assert_eq!(sorted, (0..100).collect::<Vec<_>>());

    for _ in 0..1000 {
        let e = distinct::<5>(&mut rng, 32);
        assert!(e.iter().all(|&j| j < 32));
        assert_eq!(e.iter().collect::<BTreeSet<_>>().len(), 5);
    }
    let all = distinct::<32>(&mut rng, 32);
    assert_eq!(all.iter().collect::<BTreeSet<_>>().len(), 32);

    // Every box is equally likely to be in E.
    let mut counts = [0u64; 32];
    for _ in 0..20_000 {
        for j in distinct::<5>(&mut rng, 32) {
            counts[j as usize] += 1;
        }
    }
    let chi = chi_square(&counts);
    // 31 degrees of freedom, p = 0.001 (Wilson–Hilferty).
    assert!(chi < 61.1, "chi-square {chi}");
}
