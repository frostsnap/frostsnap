//! Review tests for the verifiable card, format C′ (the packed layout), from outside the crate.
//!
//! The first half re-derives spec v3 Part 4 from the spec text alone: PK.1 (points, slots and
//! packing), PK.2 (the layout descriptor, μ and the masks) and PK.5 (the committed set, the bit
//! sets, ctx, hP, h_C and open_dom), with the parts of L1.5.2.4 (blindings, coin), L1.5.3.2
//! (the carry opening) and L1.5.4.1 (the bit-proof transcript) that PK.5 keeps. It has its own
//! GF(2^11) arithmetic and Lagrange interpolation over the 61 value points, its own bit strings
//! and tagged hashes, and the layout descriptor copied from spec PK.2 (`SPEC_FMT_C`), never from
//! the code. It checks the codec, the bit sets, the seal, the openings and the bit proofs
//! against that byte for byte, so the known answers in `proof.rs` (pinned from the
//! implementation) are pinned to the spec through this file.
//!
//! The second half plays a lying chip against the phone's checks: integer non-bits, free bits,
//! the largest carries, a phone half swapped after the pin, and a phone half that isn't the
//! masks.

use frost_backup::{
    verifiable_card::{
        codec,
        layout::{
            self, BoxSets, Meta, CARRY_BITS, COMMITTED, FREE, HALF, V_LINK, WORDS, WORD_BITS,
        },
        proof::{
            self, BitCtx, CardSecrets, Context, Opening, Ops, Seal, Setup, VerifyError, PROOF_LEN,
        },
    },
    Fingerprint, ShareBackup,
};
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};
use schnorr_fun::{
    frost::{SecretShare, ShareImage, SharedKey},
    fun::{prelude::*, subtle::Choice},
};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

const KEY_ID: [u8; 32] = [0x11; 32];
const ACCESS_STRUCTURE_ID: [u8; 32] = [0x22; 32];
/// Copied from spec v3 PK.2 (as amended for owner B6), not from the code.
const SPEC_FMT_C: &[u8] = b"frostsnap-card|packed-c1|gf2^11|poly=0x805|n=64|k=61|x=word|rand=2019..2023,33..64|thr=2021|hidden=2024..2047|h=idx8+scalar256-be|self=2019,2020,2022,2023,33..52|wallet=53..64|D=words1-32|phone=words33-64";

fn sha(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}

/// TH(tag, m) = SHA256(SHA256(tag) ‖ SHA256(tag) ‖ m).
fn th(tag: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let tag = sha(&[tag]);
    let mut all: Vec<&[u8]> = vec![&tag[..], &tag[..]];
    all.extend_from_slice(parts);
    sha(&all)
}

fn h2s(bytes: [u8; 32]) -> Scalar<Secret, Zero> {
    Scalar::from_bytes_mod_order(bytes)
}

/// 2^k as a scalar.
fn pow2(k: usize) -> Scalar<Public, Zero> {
    let mut bytes = [0u8; 32];
    bytes[31 - k / 8] = 1 << (k % 8);
    Scalar::from_bytes_mod_order(bytes)
}

/// Bit k of a byte string, most significant bit of byte 0 first.
fn bit_of(bytes: &[u8], k: usize) -> u16 {
    ((bytes[k / 8] >> (7 - k % 8)) & 1) as u16
}

/// The 11-bit value at bits 11q..11q + 10 of a byte string, most significant first.
fn eleven(bytes: &[u8], q: usize) -> u16 {
    (0..11).fold(0, |acc, k| (acc << 1) | bit_of(bytes, 11 * q + k))
}

/// Shift-and-add multiplication in GF(2^11) mod x^11 + x^2 + 1, independent of `gf.rs`.
fn gf_mul(a: u16, b: u16) -> u16 {
    let mut r: u32 = 0;
    for i in 0..11 {
        if (b >> i) & 1 == 1 {
            r ^= (a as u32) << i;
        }
    }
    for bit in (11..=20).rev() {
        if (r >> bit) & 1 == 1 {
            r ^= 0x805 << (bit - 11);
        }
    }
    r as u16
}

/// a^-1 = a^(2^11 − 2).
fn gf_inv(a: u16) -> u16 {
    assert_ne!(a, 0);
    let (mut base, mut exp, mut acc) = (a, 2046u32, 1u16);
    while exp > 0 {
        if exp & 1 == 1 {
            acc = gf_mul(acc, base);
        }
        base = gf_mul(base, base);
        exp >>= 1;
    }
    acc
}

/// The 61 value points of PK.1, in slot order: the random hidden values at 2019..2023, the
/// phone words 33..64, then the secret hidden values at 2024..2047.
fn spec_nodes() -> [u16; 61] {
    let points: Vec<u16> = (2019..=2023).chain(33..=64).chain(2024..=2047).collect();
    points.try_into().unwrap()
}

/// λ_k(x) = Π_{m≠k} (x − x_m)/(x_k − x_m) for every value point x_k (subtraction is XOR).
fn spec_lagrange(x: u16) -> [u16; 61] {
    let nodes = spec_nodes();
    core::array::from_fn(|k| {
        let (mut num, mut den) = (1u16, 1u16);
        for (m, &node) in nodes.iter().enumerate() {
            if m != k {
                num = gf_mul(num, x ^ node);
                den = gf_mul(den, nodes[k] ^ node);
            }
        }
        gf_mul(num, gf_inv(den))
    })
}

/// λ_k(w) for the device words w = 1..32, computed once.
fn device_rows() -> &'static [[u16; 61]; 32] {
    static ROWS: OnceLock<[[u16; 61]; 32]> = OnceLock::new();
    ROWS.get_or_init(|| core::array::from_fn(|j| spec_lagrange(j as u16 + 1)))
}

/// f(x) = Σ_k λ_k(x)·v_k.
fn spec_eval(row: &[u16; 61], values: &[u16; 61]) -> u16 {
    row.iter()
        .zip(values)
        .fold(0, |acc, (&l, &v)| acc ^ gf_mul(l, v))
}

/// L = TH("frostsnap/card/c1/layout", u8 len(FMT_C) ‖ FMT_C).
fn spec_l() -> [u8; 32] {
    th(
        b"frostsnap/card/c1/layout",
        &[&[SPEC_FMT_C.len() as u8], SPEC_FMT_C],
    )
}

/// hP = TH("frostsnap/card/c1/phone-half", L ‖ BE16(y_33) ‖ … ‖ BE16(y_64)).
fn spec_phone_hash(phone: &[u16; HALF]) -> [u8; 32] {
    let words: Vec<u8> = phone.iter().flat_map(|y| y.to_be_bytes()).collect();
    th(b"frostsnap/card/c1/phone-half", &[&spec_l(), &words])
}

struct SpecCard {
    /// Slot order, as PK.1's value slots.
    values: [u16; 61],
    words: [u16; 64],
    counter: u8,
    mu: u16,
}

impl SpecCard {
    fn phone(&self) -> [u16; HALF] {
        self.words[32..].try_into().unwrap()
    }
}

/// PK.1 and PK.2, written from the spec text.
fn spec_encode(
    s: &[u8; 32],
    index: u8,
    threshold: u8,
    f: &[Point<Normal, Public, Zero>],
) -> SpecCard {
    let l = spec_l();
    // Dh = u8(i) ‖ ser32(s), D[0] first; h_m = Σ_k Dh[11m + k]·2^(10 − k).
    let dh: Vec<u8> = core::iter::once(index).chain(s.iter().copied()).collect();
    let h: [u16; 24] = core::array::from_fn(|m| eleven(&dh, m));
    // μ = top_11(TH("frostsnap/card/c1/thr", L ‖ ser32(s))).
    let mu = eleven(&th(b"frostsnap/card/c1/thr", &[&l, s]), 0);
    let ser_f: Vec<u8> = f.iter().flat_map(|p| p.to_bytes()).collect();
    for counter in 0..=255u8 {
        // Self-checking m1..m24: the first 264 bits of B_0 ‖ B_1.
        let b: Vec<u8> = [0u8, 1]
            .iter()
            .flat_map(|&b| {
                th(
                    b"frostsnap/card/c1/mask",
                    &[&l, s, &[index], &[counter], &[b]],
                )
            })
            .collect();
        // Wallet-bound m25..m36: the first 132 bits of B_w.
        let b_w = th(
            b"frostsnap/card/c1/mask-wallet",
            &[&l, s, &[index], &ser_f, &[counter]],
        );
        let m: Vec<u16> = (0..24)
            .map(|q| eleven(&b, q))
            .chain((0..12).map(|q| eleven(&b_w, q)))
            .collect();
        // f(2019), f(2020) = m1, m2; f(2021) = (0 0 0 t) XOR μ; f(2022), f(2023) = m3, m4;
        // w33..w64 = m5..m36; f(2024 + m) = h_m.
        let mut values = [0u16; 61];
        values[0] = m[0];
        values[1] = m[1];
        values[2] = threshold as u16 ^ mu;
        values[3] = m[2];
        values[4] = m[3];
        values[5..37].copy_from_slice(&m[4..36]);
        values[37..].copy_from_slice(&h);
        let mut words = [0u16; 64];
        for (j, row) in device_rows().iter().enumerate() {
            words[j] = spec_eval(row, &values);
        }
        words[32..].copy_from_slice(&values[5..37]);
        let distinct = (0..64).all(|a| (a + 1..64).all(|b| words[a] != words[b]));
        if distinct {
            return SpecCard {
                values,
                words,
                counter,
                mu,
            };
        }
    }
    panic!("no counter");
}

/// v for value bit (slot, g), from the committed-set table of spec PK.5 (slots as PK.1). The
/// phone words and the index bits are public: no v.
fn spec_v(slot: usize, g: usize) -> Option<usize> {
    match slot {
        // The 5 random hidden values, x = 2019..2023.
        0..=4 => Some(11 * slot + g),
        // P.
        5..=36 => None,
        // h0: bits 0..2 are scalar bits 253..255, bits 3..10 the index.
        37 if g < 3 => Some(55 + g),
        37 => None,
        // h_m, m = 1..23: 58 + 11(m − 1) + g.
        38..=60 => Some(58 + 11 * (slot - 38) + g),
        _ => unreachable!("61 slots"),
    }
}

/// The value bit that holds scalar bit t: bit (t mod 11) of h_{23 − ⌊t/11⌋} (PK.1).
fn spec_scalar_v(t: usize) -> usize {
    spec_v(37 + 23 - t / 11, t % 11).unwrap()
}

/// x_v, from the spec's values.
fn spec_bits(values: &[u16; 61]) -> Vec<u32> {
    let mut x = vec![0; COMMITTED];
    for (slot, &value) in values.iter().enumerate() {
        for g in 0..11 {
            if let Some(v) = spec_v(slot, g) {
                x[v] = ((value >> g) & 1) as u32;
            }
        }
    }
    x
}

/// For each output bit β, the value bits (slot, g) that feed it.
type PublicSets = Vec<Vec<(usize, usize)>>;

/// T_wβ and P_wβ for device word w = j + 1 by spec PK.5: value bit (k, g) feeds output bit β
/// when bit β of λ_k(w) ⊗ x^g is 1. The public ones are given as (slot, g).
fn spec_sets(j: usize) -> (Vec<Vec<usize>>, PublicSets) {
    let row = &device_rows()[j];
    let mut t = vec![vec![]; 11];
    let mut p = vec![vec![]; 11];
    for (slot, &lambda) in row.iter().enumerate() {
        for g in 0..11 {
            let e = gf_mul(lambda, 1 << g);
            for beta in 0..11 {
                if (e >> beta) & 1 == 1 {
                    match spec_v(slot, g) {
                        Some(v) => t[beta].push(v),
                        None => p[beta].push((slot, g)),
                    }
                }
            }
        }
    }
    (t, p)
}

/// ⊕ of the public bits in one P_wβ, from the values.
fn public_parity(p: &[(usize, usize)], values: &[u16; 61]) -> u32 {
    p.iter()
        .map(|&(slot, g)| ((values[slot] >> g) & 1) as u32)
        .fold(0, |a, b| a ^ b)
}

fn wallet(threshold: usize, rng: &mut impl RngCore) -> (SharedKey, Vec<SecretShare>) {
    let (backups, shared_key) = ShareBackup::generate_shares(
        Scalar::random(rng),
        threshold,
        threshold + 1,
        Fingerprint::NONE,
        rng,
    );
    let shares = backups
        .into_iter()
        .map(|backup| backup.extract_secret(&shared_key).unwrap())
        .collect();
    (shared_key, shares)
}

fn context(share: &SecretShare, key: &SharedKey) -> Context {
    Context::new(
        KEY_ID,
        ACCESS_STRUCTURE_ID,
        share.share_image(),
        key.threshold(),
    )
    .unwrap()
}

fn index_u8(share: &SecretShare) -> u8 {
    u32::try_from(share.index).unwrap() as u8
}

/// Gives the same 32 bytes to every call: a stuck TRNG, or a fixed aux32.
struct Stuck([u8; 32]);

impl RngCore for Stuck {
    fn next_u32(&mut self) -> u32 {
        u32::from_le_bytes(self.0[..4].try_into().unwrap())
    }
    fn next_u64(&mut self) -> u64 {
        u64::from_le_bytes(self.0[..8].try_into().unwrap())
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for (i, b) in dest.iter_mut().enumerate() {
            *b = self.0[i % 32];
        }
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for Stuck {}

#[test]
fn codec_matches_spec_pk1_pk2_written_independently() {
    // The descriptor in the code is the spec's, byte for byte.
    assert_eq!(layout::FMT_C, SPEC_FMT_C);
    assert_eq!(layout::layout_hash(), spec_l());
    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    let mut counters = [0usize; 4];
    // Real wallets, through `encode`.
    for t in [2, 3, 5] {
        let (key, shares) = wallet(t, &mut rng);
        for share in &shares {
            let card = codec::encode(share, &key).unwrap();
            let spec = spec_encode(
                &share.share.to_bytes(),
                index_u8(share),
                t as u8,
                key.point_polynomial(),
            );
            assert_eq!(card.values, spec.values);
            assert_eq!(card.words, spec.words);
            assert_eq!(card.counter, spec.counter);
            assert_eq!(card.phone_half(), spec.phone());
        }
    }
    // Wide thresholds (0 is "not recorded") and indices, so every bit of h0 and of the
    // threshold value is exercised, through `encode_meta`.
    for case in 0..120 {
        let threshold: u8 = match case % 5 {
            0 => 255,
            1 => 0,
            2 => 8,
            _ => rng.gen_range(1..=255),
        };
        let index: u8 = match case % 7 {
            0 => 255,
            1 => 1,
            _ => rng.gen_range(1..=255),
        };
        let f: Vec<Point<Normal, Public, Zero>> = (0..threshold.clamp(1, 12))
            .map(|_| Point::random(&mut rng).mark_zero())
            .collect();
        let s = Scalar::random(&mut rng).mark_zero();
        let meta = Meta { threshold, index };
        let card = codec::encode_meta(s, meta, &f).unwrap();
        let spec = spec_encode(&s.to_bytes(), index, threshold, &f);
        assert_eq!(
            (card.values, card.words, card.counter),
            (spec.values, spec.words, spec.counter),
            "case {case}"
        );
        counters[(card.counter as usize).min(3)] += 1;
        // PK.1's diagram, bit by bit.
        let sb = s.to_bytes();
        let sbit = |t: usize| ((sb[31 - t / 8] >> (t % 8)) & 1) as u16;
        let h0 = card.values[37];
        assert_eq!(h0 >> 3, index as u16);
        assert_eq!(h0 & 7, (sbit(255) << 2) | (sbit(254) << 1) | sbit(253));
        assert_eq!(card.values[38] >> 10, sbit(252));
        assert_eq!(card.values[60] & 1, sbit(0));
        // The threshold value at x = 2021, slot 2: top 3 bits of u zero, u mod 256 = t.
        let u = card.values[layout::THRESHOLD_SLOT] ^ spec.mu;
        assert_eq!(u, threshold as u16);
        assert_eq!(codec::threshold_mask(s), spec.mu);
    }
    // About 37% of cards take counter 0.
    assert!(counters[1] + counters[2] + counters[3] > 0, "{counters:?}");
    assert!(counters[0] > 0, "{counters:?}");
}

#[test]
fn bit_sets_match_spec_pk5() {
    // The committed order v of PK.5, and v_link.
    for v in 0..COMMITTED {
        let (slot, g) = layout::value_bit(v);
        assert_eq!(spec_v(slot, g), Some(v), "v = {v}");
    }
    let committed = (0..61)
        .flat_map(|slot| (0..11).filter_map(move |g| spec_v(slot, g)))
        .count();
    assert_eq!((committed, FREE), (311, 55));
    assert_eq!(V_LINK, spec_scalar_v(0));
    assert_eq!(V_LINK, 300);
    for t in 0..256 {
        assert_eq!(
            layout::committed_of_scalar_bit(t),
            spec_scalar_v(t),
            "t = {t}"
        );
    }
    // T_wβ over every device word, and the honest carry bound.
    let mut sizes = vec![];
    for j in 0..HALF {
        let (t, _) = spec_sets(j);
        let sets = BoxSets::new(j);
        for (set, t) in sets.committed.iter().zip(&t) {
            assert_eq!(set.iter().collect::<Vec<_>>(), {
                let mut v = t.clone();
                v.sort();
                v
            });
            sizes.push(t.len());
        }
    }
    // PK.1's parity-set sizes (owner B6): min 138, max 184, so a carry fits 7 bits.
    let (min, max) = (*sizes.iter().min().unwrap(), *sizes.iter().max().unwrap());
    assert_eq!((min, max), (138, 184));
    assert!(max / 2 < 1 << CARRY_BITS);
    assert_eq!(layout::MAX_SET, max);

    // The public part: bit β of the word is the committed parity XOR the public parity over
    // P and the index.
    let mut rng = ChaCha20Rng::from_seed([10; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[0];
    let card = codec::encode(share, &key).unwrap();
    let x = spec_bits(&card.values);
    for j in 0..HALF {
        let (t, p) = spec_sets(j);
        let public = BoxSets::new(j).public_word(index_u8(share), &card.phone_half());
        for beta in 0..11 {
            let parity = public_parity(&p[beta], &card.values);
            assert_eq!(
                ((public >> beta) & 1) as u32,
                parity,
                "word {}, β = {beta}",
                j + 1
            );
            let committed: u32 = t[beta].iter().map(|&v| x[v]).sum();
            assert_eq!(
                ((card.words[j] >> beta) & 1) as u32,
                (committed & 1) ^ parity,
                "word {}, β = {beta}",
                j + 1
            );
        }
    }
}

/// The seal, from spec PK.5 and L1.5.2.4 alone.
struct SpecSeal {
    card: SpecCard,
    ctx: [u8; 32],
    key: [u8; 32],
    r: Vec<Scalar<Secret, Zero>>,
    x: Vec<u32>,
    commitments: Vec<[u8; 33]>,
    coin: [u8; 32],
    coin_commit: [u8; 32],
    hp: [u8; 32],
    h_c: [u8; 32],
}

fn spec_seal(h: Point, share: &SecretShare, key: &SharedKey) -> SpecSeal {
    let s = share.share.to_bytes();
    let index = index_u8(share);
    let t = key.threshold() as u8;
    let image = share.share_image().image;
    // ctx = TH(ctx-tag, u8 len(FMT_C) ‖ FMT_C ‖ key_id ‖ access_structure_id ‖ i_sh ‖ ser(S_i)
    // ‖ u8(t_f) ‖ u8(i)).
    let ctx = th(
        b"frostsnap/card/commit-v1/ctx",
        &[
            &[SPEC_FMT_C.len() as u8],
            SPEC_FMT_C,
            &KEY_ID,
            &ACCESS_STRUCTURE_ID,
            &share.index.to_bytes(),
            &image.to_bytes(),
            &[t],
            &[index],
        ],
    );
    let k_card = th(b"frostsnap/card/commit-v1/blind-key", &[&s, &ctx]);
    let mut r: Vec<Scalar<Secret, Zero>> = (0..COMMITTED)
        .map(|v| {
            h2s(th(
                b"frostsnap/card/commit-v1/blind",
                &[&k_card, &(v as u16).to_be_bytes()],
            ))
        })
        .collect();
    // r_300 = −Σ_{v scalar, v≠300} 2^{t(v)}·r_v.
    let mut sum = Scalar::<Secret, Zero>::zero();
    for tt in 1..256 {
        let w = pow2(tt);
        let rv = r[spec_scalar_v(tt)];
        sum = s!(sum + w * rv);
    }
    r[spec_scalar_v(0)] = s!(-sum);
    let card = spec_encode(&s, index, t, key.point_polynomial());
    let x = spec_bits(&card.values);
    let commitments: Vec<[u8; 33]> = (0..COMMITTED)
        .map(|v| {
            let xv = Scalar::<Public, Zero>::from(x[v]);
            let rv = r[v];
            g!(xv * G + rv * h).normalize().to_bytes()
        })
        .collect();
    let coin = th(b"frostsnap/card/commit-v1/box-coin", &[&k_card]);
    let coin_commit = th(b"frostsnap/card/commit-v1/box-coin-commit", &[&coin]);
    let hp = spec_phone_hash(&card.phone());
    // h_C = TH("frostsnap/card/c1/seal", ctx ‖ coin_commit ‖ hP ‖ ser(C_0) ‖ … ‖ ser(C_310)).
    let mut seal_parts: Vec<&[u8]> = vec![&ctx[..], &coin_commit[..], &hp[..]];
    seal_parts.extend(commitments.iter().map(|c| &c[..]));
    let h_c = th(b"frostsnap/card/c1/seal", &seal_parts);
    SpecSeal {
        card,
        ctx,
        key: k_card,
        r,
        x,
        commitments,
        coin,
        coin_commit,
        hp,
        h_c,
    }
}

#[test]
fn seal_matches_spec_pk5_written_independently() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([2; 32]);
    let (key, shares) = wallet(3, &mut rng);
    for share in &shares {
        let context = context(share, &key);
        let mut ops = Ops::default();
        let spec = spec_seal(setup.h, share, &key);
        assert_eq!(context.hash(&mut ops), spec.ctx);
        let secrets = CardSecrets::new(share, &key, KEY_ID, ACCESS_STRUCTURE_ID, &mut ops).unwrap();
        assert_eq!(*secrets.phone_half(), spec.card.phone());
        assert_eq!(*secrets.phone_hash(), spec.hp);
        assert_eq!(
            proof::phone_half_hash(&spec.card.phone(), &mut ops),
            spec.hp
        );
        for v in 0..COMMITTED {
            assert_eq!(
                secrets.blinding(v, &mut ops).unwrap().mark_zero(),
                spec.r[v],
                "r_{v}"
            );
            assert_eq!(secrets.bit(v).unwrap_u8() as u32, spec.x[v], "x_{v}");
        }
        let seal = setup.seal(&secrets, &mut ops).unwrap();
        assert_eq!(seal.commitments, spec.commitments);
        assert_eq!(secrets.coin(), spec.coin);
        assert_eq!(seal.coin_commit, spec.coin_commit);
        assert_eq!(seal.h_c, spec.h_c);
        // The phone pins it with the spec's P, and only with it.
        setup
            .pin(&context, seal.clone(), &spec.card.phone(), &mut ops)
            .unwrap();
        let mut other = spec.card.phone();
        other[0] ^= 1;
        assert_eq!(
            setup.pin(&context, seal, &other, &mut ops).err(),
            Some(VerifyError::SealHash)
        );
    }
}

#[test]
fn opening_matches_spec_l1_5_3_2_and_pk5_written_independently() {
    let setup = Setup::new();
    let h = setup.h;
    let mut rng = ChaCha20Rng::from_seed([3; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[1];
    let context = context(share, &key);
    let mut ops = Ops::default();
    let secrets = CardSecrets::new(share, &key, KEY_ID, ACCESS_STRUCTURE_ID, &mut ops).unwrap();
    let seal = setup.seal(&secrets, &mut ops).unwrap();
    let proofs = setup
        .prove_commit_bits(&secrets, &seal, &mut rng, &mut ops)
        .unwrap();
    let spec = spec_seal(h, share, &key);
    let phone = spec.card.phone();
    let proven = setup
        .verify_seal(&context, &seal, &phone, &proofs, &mut ops)
        .unwrap();
    let half = Scalar::<Public, Zero>::from(2u32)
        .non_zero()
        .unwrap()
        .invert();
    let n = [0x09; 32];
    let n_tag = th(b"frostsnap/card/proof-v3/n-tag", &[&n]);
    assert_eq!(proof::n_tag(&n, &mut ops), n_tag);

    for (j, run) in [(0usize, 0u8), (12, 1), (31, 3), (20, 2)] {
        let mut device_rng = ChaCha20Rng::from_seed([j as u8; 32]);
        // rng32 is the first 32 bytes the opening draws.
        let mut fresh = [0u8; 32];
        device_rng.clone().fill_bytes(&mut fresh);
        let opening = setup
            .open(&secrets, &seal, &n, j, run, &mut device_rng, &mut ops)
            .unwrap();
        let y = spec.card.words[j];
        assert_eq!(opening.y, y);
        let (t_sets, p_sets) = spec_sets(j);
        let mut carries = vec![];
        for beta in 0..11 {
            let w: u32 = t_sets[beta].iter().map(|&v| spec.x[v]).sum();
            // y′_β = bit_β(y) ⊕ the public parity over the index and P.
            let y_prime =
                ((y >> beta) & 1) as u32 ^ public_parity(&p_sets[beta], &spec.card.values);
            assert_eq!(w % 2, y_prime, "y′ is the committed parity");
            let q = (w - y_prime) / 2;
            assert!(q < 1 << 7, "7 carry bits");
            let mut sigma = vec![Scalar::<Secret, Zero>::zero(); 7];
            let mut weighted = Scalar::<Secret, Zero>::zero();
            for (l, sigma_l) in sigma.iter_mut().enumerate().skip(1) {
                *sigma_l = h2s(th(
                    b"frostsnap/card/proof-v3/carry-blind",
                    &[
                        &spec.key,
                        &n_tag,
                        &[run, j as u8, beta as u8, l as u8],
                        &fresh,
                    ],
                ));
                let w = pow2(l + 1);
                let sl = *sigma_l;
                weighted = s!(weighted + w * sl);
            }
            let rho = t_sets[beta]
                .iter()
                .fold(Scalar::<Secret, Zero>::zero(), |acc, &v| {
                    let rv = spec.r[v];
                    s!(acc + rv)
                });
            sigma[0] = s!(half * (rho - weighted));
            for (l, &sl) in sigma.iter().enumerate() {
                let ql = Scalar::<Public, Zero>::from((q >> l) & 1);
                carries.push(g!(ql * G + sl * h).normalize().to_bytes());
            }
        }
        assert_eq!(carries.len(), 77);
        assert_eq!(opening.carries, carries, "box {j}");
        // open_dom = TH("frostsnap/card/proof-v3/open", h_C ‖ hP ‖ N ‖ u8 j ‖ BE16(y) ‖ Q's).
        let mut parts: Vec<&[u8]> = vec![&spec.h_c[..], &spec.hp[..], &n[..]];
        let jb = [j as u8];
        let yb = y.to_be_bytes();
        parts.push(&jb);
        parts.push(&yb);
        parts.extend(carries.iter().map(|q| &q[..]));
        let dom = th(b"frostsnap/card/proof-v3/open", &parts);
        assert_eq!(
            proof::open_domain(&seal.h_c, &spec.hp, &n, j as u8, y, &carries, &mut ops),
            dom
        );
        for (k, (q, p)) in carries.iter().zip(&opening.proofs).enumerate() {
            let q = Point::from_bytes(*q).unwrap();
            assert!(setup.verify_bit(&q, BitCtx::carry(dom, k), p, &mut ops));
        }
        assert_eq!(
            setup.verify_opening(&proven, &phone, &n, j, &opening, &mut ops),
            Ok(())
        );
    }
}

/// A bit proof by spec L1.5.4.1's transcript, step by step, under the proof-v3 name.
fn spec_prove(h: Point, x: Point, ctx: BitCtx, bit: bool, r: Scalar, aux: [u8; 32]) -> [u8; 96] {
    let name = sha(&[b"frostsnap/card/proof-v3/bit"]);
    let x_minus_g = g!(x - G).normalize().non_zero().unwrap();
    let statement = [
        &[ctx.role][..],
        &ctx.dom[..],
        &ctx.index.to_be_bytes()[..],
        &h.to_bytes()[..],
        &x.to_bytes()[..],
        &h.to_bytes()[..],
        &x_minus_g.to_bytes()[..],
    ]
    .concat();
    let seed = sha(&[&name, &name, &statement, &r.to_bytes(), &aux]);
    let mut nonce_rng = ChaCha20Rng::from_seed(seed);
    let mut e_fake = [0u8; 16];
    nonce_rng.fill_bytes(&mut e_fake);
    let mut bytes = [0u8; 32];
    nonce_rng.fill_bytes(&mut bytes);
    let z_fake = Scalar::<Public, Zero>::from_bytes_mod_order(bytes);
    nonce_rng.fill_bytes(&mut bytes);
    let k = Scalar::<Secret, Zero>::from_bytes_mod_order(bytes);
    let challenge = |e: &[u8; 16]| {
        let mut b = [0u8; 32];
        b[16..].copy_from_slice(e);
        Scalar::<Public, Zero>::from_bytes_mod_order(b)
    };
    let a_real = g!(k * h).normalize();
    let target = if bit { x } else { x_minus_g };
    let ef = challenge(&e_fake);
    let a_fake = g!(z_fake * h - ef * target).normalize();
    let (a_l, a_r) = if bit {
        (a_fake, a_real)
    } else {
        (a_real, a_fake)
    };
    let full = sha(&[&name, &name, &statement, &a_l.to_bytes(), &a_r.to_bytes()]);
    let e: [u8; 16] = full[..16].try_into().unwrap();
    let e_real: [u8; 16] = core::array::from_fn(|i| e[i] ^ e_fake[i]);
    let er = challenge(&e_real);
    let z_real = s!(k + er * r).public();
    let (z_l, e_l, z_r) = if bit {
        (z_fake, e_fake, z_real)
    } else {
        (z_real, e_real, z_fake)
    };
    [&e[..], &z_l.to_bytes()[..], &e_l[..], &z_r.to_bytes()[..]]
        .concat()
        .try_into()
        .unwrap()
}

#[test]
fn bit_proofs_match_spec_l1_5_4_1_written_independently() {
    let setup = Setup::new();
    let h = setup.h;
    let mut rng = ChaCha20Rng::from_seed([4; 32]);
    for case in 0..40 {
        let bit = case % 2 == 1;
        let r = Scalar::random(&mut rng);
        let x = if bit {
            g!(r * h + G).normalize().non_zero().unwrap()
        } else {
            g!(r * h).normalize().non_zero().unwrap()
        };
        let ctx = BitCtx {
            role: (case / 2 % 2) as u8,
            dom: rng.gen(),
            index: rng.gen(),
        };
        let aux: [u8; 32] = if case < 4 { [0x01; 32] } else { rng.gen() };
        let ours = setup
            .prove_bit(
                x,
                ctx,
                Choice::from(bit as u8),
                r,
                Some(&mut Stuck(aux)),
                &mut Ops::default(),
            )
            .unwrap();
        assert_eq!(ours, spec_prove(h, x, ctx, bit, r, aux), "case {case}");
        // The stock prover agrees too.
        let witness = if bit {
            sigma_fun::Either::Right(r)
        } else {
            sigma_fun::Either::Left(r)
        };
        let statement = setup.statement(x, ctx, &mut Ops::default()).unwrap();
        let stock = setup
            .fs()
            .prove(&witness, &statement, Some(&mut Stuck(aux)));
        assert_eq!(proof::encode_proof(&stock), ours);
    }
}

#[test]
fn point_encodings_are_canonical() {
    let mut ops = Ops::default();
    let p = Point::random(&mut rand::thread_rng());
    let good = p.to_bytes();
    assert!(proof::point_from_bytes(&good, &mut ops).is_some());
    for prefix in [0x00, 0x01, 0x04, 0x05, 0x06, 0x07] {
        let mut bad = good;
        bad[0] = prefix;
        assert!(
            proof::point_from_bytes(&bad, &mut ops).is_none(),
            "{prefix}"
        );
    }
    assert!(proof::point_from_bytes(&[0; 33], &mut ops).is_none());
    assert!(proof::point_from_bytes(&G.to_bytes(), &mut ops).is_none());
    // A point with a small x, written as x + p (which still fits in 32 bytes).
    let p_bytes: [u8; 32] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xff, 0xff,
        0xfc, 0x2f,
    ];
    let small = (1u8..=255)
        .find_map(|x| {
            let mut bytes = [0u8; 33];
            bytes[0] = 2;
            bytes[32] = x;
            Point::<Normal, Public, NonZero>::from_bytes(bytes).map(|_| x)
        })
        .unwrap();
    let mut wrapped = [0u8; 33];
    wrapped[0] = 2;
    wrapped[1..].copy_from_slice(&p_bytes);
    // p + small: p ends in ...fc2f, so adding a byte below 0xd1 doesn't carry.
    assert!(small < 0xd0);
    wrapped[32] += small;
    assert!(proof::point_from_bytes(&wrapped, &mut ops).is_none());
}

/// A lying chip's seal: commitments to integer values x_v, which needn't be bits, with
/// blindings that make the link exact whenever Σ_t 2^t·x_t = s, sealed (h_C over hP) with the
/// phone half P it serves.
struct Liar {
    x: Vec<u32>,
    r: Vec<Scalar>,
    /// The phone half it sealed with.
    phone: [u16; HALF],
    seal: Seal,
}

fn liar_seal(
    setup: &Setup,
    context: &Context,
    x: Vec<u32>,
    phone: [u16; HALF],
    rng: &mut impl RngCore,
) -> Liar {
    let h = setup.h;
    let mut r: Vec<Scalar> = (0..COMMITTED).map(|_| Scalar::random(rng)).collect();
    let mut sum = Scalar::<Secret, Zero>::zero();
    for t in (1..256).rev() {
        let rv = r[spec_scalar_v(t)];
        sum = s!(sum + sum + rv);
    }
    r[V_LINK] = s!(-(sum + sum)).non_zero().unwrap();
    let commitments: Vec<[u8; 33]> = (0..COMMITTED)
        .map(|v| {
            let xv = Scalar::<Public, Zero>::from(x[v]);
            let rv = r[v];
            g!(xv * G + rv * h).normalize().to_bytes()
        })
        .collect();
    let ctx = context.hash(&mut Ops::default());
    let mut coin_commit = [0u8; 32];
    rng.fill_bytes(&mut coin_commit);
    let hp = spec_phone_hash(&phone);
    let mut parts: Vec<&[u8]> = vec![&ctx[..], &coin_commit[..], &hp[..]];
    parts.extend(commitments.iter().map(|c| &c[..]));
    let h_c = th(b"frostsnap/card/c1/seal", &parts);
    Liar {
        x,
        r,
        phone,
        seal: Seal {
            commitments,
            coin_commit,
            h_c,
        },
    }
}

impl Liar {
    /// Every commit proof it can make: honest ones for bits, the stock prover's best try for
    /// the rest.
    fn commit_proofs(
        &self,
        setup: &Setup,
        rng: &mut (impl RngCore + CryptoRng),
    ) -> Vec<[u8; PROOF_LEN]> {
        (0..COMMITTED)
            .map(|v| {
                let x = Point::from_bytes(self.seal.commitments[v]).unwrap();
                let ctx = BitCtx::commit(self.seal.h_c, v);
                match self.x[v] {
                    0 | 1 => setup
                        .prove_bit(
                            x,
                            ctx,
                            Choice::from(self.x[v] as u8),
                            self.r[v],
                            Some(&mut *rng),
                            &mut Ops::default(),
                        )
                        .unwrap(),
                    _ => {
                        let statement = setup.statement(x, ctx, &mut Ops::default()).unwrap();
                        let witness = sigma_fun::Either::Right(self.r[v]);
                        proof::encode_proof(&setup.fs().prove(
                            &witness,
                            &statement,
                            Some(&mut *rng),
                        ))
                    }
                }
            })
            .collect()
    }

    /// The public part of device word `j + 1` under P: the index bits and the phone half.
    fn pub_j(index: u8, phone: &[u16; HALF], j: usize) -> u16 {
        BoxSets::new(j).public_word(index, phone)
    }

    /// The word its committed integers give at device word `j + 1` under `phone`: the parity
    /// of each output bit's sum, XOR the public part.
    fn word(&self, index: u8, phone: &[u16; HALF], j: usize) -> u16 {
        let sets = BoxSets::new(j);
        let public = Self::pub_j(index, phone, j);
        (0..WORD_BITS).fold(0, |y, beta| {
            let w: u32 = sets.committed[beta].iter().map(|v| self.x[v]).sum();
            y | ((((w & 1) as u16) ^ ((public >> beta) & 1)) << beta)
        })
    }

    /// An opening of device word `j + 1` that's consistent with the committed integers under
    /// `phone`: the word is [`word`](Self::word), the carries are genuine bits, so every carry
    /// proof is honest, and open_dom covers hP(phone).
    fn open(
        &self,
        setup: &Setup,
        (index, phone): (u8, &[u16; HALF]),
        n: &[u8; 32],
        j: usize,
        rng: &mut (impl RngCore + CryptoRng),
    ) -> Opening {
        let h = setup.h;
        let sets = BoxSets::new(j);
        let public = Self::pub_j(index, phone, j);
        let half = Scalar::<Public, Zero>::from(2u32)
            .non_zero()
            .unwrap()
            .invert();
        let mut y = 0u16;
        let mut points = vec![];
        let mut witnesses = vec![];
        for beta in 0..WORD_BITS {
            let w: u32 = sets.committed[beta].iter().map(|v| self.x[v]).sum();
            let y_prime = w & 1;
            y |= ((y_prime as u16) ^ ((public >> beta) & 1)) << beta;
            let q = (w - y_prime) / 2;
            assert!(
                q < 1 << CARRY_BITS,
                "the carry needs more than {CARRY_BITS} bits"
            );
            let rho = sets.committed[beta]
                .iter()
                .fold(Scalar::<Secret, Zero>::zero(), |acc, v| {
                    let rv = self.r[v];
                    s!(acc + rv)
                });
            let mut sigma: Vec<Scalar> =
                (0..CARRY_BITS).map(|_| Scalar::random(&mut *rng)).collect();
            let weighted = (1..CARRY_BITS).fold(Scalar::<Secret, Zero>::zero(), |acc, l| {
                let w = pow2(l + 1);
                let sl = sigma[l];
                s!(acc + w * sl)
            });
            sigma[0] = s!(half * (rho - weighted)).non_zero().unwrap();
            for (l, sl) in sigma.into_iter().enumerate() {
                let bit = (q >> l) & 1;
                let ql = Scalar::<Public, Zero>::from(bit);
                points.push(g!(ql * G + sl * h).normalize().non_zero().unwrap());
                witnesses.push((bit == 1, sl));
            }
        }
        let carries: Vec<[u8; 33]> = points.iter().map(|q| q.to_bytes()).collect();
        let dom = proof::open_domain(
            &self.seal.h_c,
            &spec_phone_hash(phone),
            n,
            j as u8,
            y,
            &carries,
            &mut Ops::default(),
        );
        let proofs = points
            .iter()
            .zip(witnesses)
            .enumerate()
            .map(|(k, (q, (bit, sigma)))| {
                setup
                    .prove_bit(
                        *q,
                        BitCtx::carry(dom, k),
                        Choice::from(bit as u8),
                        sigma,
                        Some(&mut *rng),
                        &mut Ops::default(),
                    )
                    .unwrap()
            })
            .collect();
        Opening {
            j: j as u8,
            y,
            carries,
            proofs,
        }
    }
}

/// Spec L1.5.3.2's checks of an opening, written from the spec: (3) for every β, then (4) for
/// every carry under open_dom with hP(phone). `Setup::verify_opening` runs them only against a
/// proven seal and only with the pinned P; this shows what they'd accept without either.
fn opening_holds(
    setup: &Setup,
    seal: &Seal,
    (index, phone): (u8, &[u16; HALF]),
    n: &[u8; 32],
    j: usize,
    opening: &Opening,
) -> bool {
    let mut ops = Ops::default();
    let point = |bytes: &[u8; 33]| Point::<Normal, Public, NonZero>::from_bytes(*bytes).unwrap();
    let sets = BoxSets::new(j);
    let public = Liar::pub_j(index, phone, j);
    let qs: Vec<Point> = opening.carries.iter().map(point).collect();
    for beta in 0..WORD_BITS {
        let sum =
            sets.committed[beta]
                .iter()
                .fold(Point::<NonNormal, Public, Zero>::zero(), |sum, v| {
                    let c = point(&seal.commitments[v]);
                    g!(sum + c)
                });
        let y_prime = ((opening.y ^ public) >> beta) & 1;
        let d = if y_prime == 1 { g!(sum - G) } else { sum };
        let carries = (0..CARRY_BITS).fold(Point::<NonNormal, Public, Zero>::zero(), |acc, l| {
            let w = pow2(l + 1);
            let q = qs[CARRY_BITS * beta + l];
            g!(acc + w * q)
        });
        if d != carries {
            return false;
        }
    }
    let dom = proof::open_domain(
        &seal.h_c,
        &spec_phone_hash(phone),
        n,
        opening.j,
        opening.y,
        &opening.carries,
        &mut ops,
    );
    qs.iter()
        .zip(&opening.proofs)
        .enumerate()
        .all(|(k, (q, p))| setup.verify_bit(q, BitCtx::carry(dom, k), p, &mut ops))
}

/// Spec L1.5.3.1 and T7: committing x_t = b′_t + 2 at 30 positions keeps the link and every
/// opening, while the bits b′ say another scalar. Only the 311 commit proofs catch it, and
/// openings are checked only against a seal they've proven.
#[test]
fn integer_non_bits_pass_the_pin_and_every_opening_but_not_the_proofs() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([5; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[0];
    let context = context(share, &key);
    let card = codec::encode(share, &key).unwrap();
    let bits = layout::committed_bits(&card.values);
    let phone = card.phone_half();
    let public = (context.meta.index, &phone);
    let mut x: Vec<u32> = (0..COMMITTED).map(|v| bits.get(v) as u32).collect();
    // Where scalar bits (t + 1, t) are (1, 0), write (0, 2): the same integer.
    let mut moved = vec![];
    let mut t = 0;
    while t < 255 && moved.len() < 30 {
        let (low, high) = (spec_scalar_v(t), spec_scalar_v(t + 1));
        if x[high] == 1 && x[low] == 0 {
            x[high] = 0;
            x[low] = 2;
            moved.push(low);
            t += 2;
        } else {
            t += 1;
        }
    }
    assert!(moved.len() >= 20, "{}", moved.len());
    let liar = liar_seal(&setup, &context, x, phone, &mut rng);
    let mut ops = Ops::default();
    // Pinned: the link and h_C (over hP) hold.
    let mut pinned = setup
        .pin(&context, liar.seal.clone(), &liar.phone, &mut ops)
        .unwrap();

    // Every device word's opening passes the opening checks, with the words of the card the
    // bits b′ = x mod 2 spell; the phone half is P.
    let n: [u8; 32] = rng.gen();
    let entries: [Option<u16>; WORDS] = core::array::from_fn(|j| {
        if j >= HALF {
            return Some(phone[j - HALF]);
        }
        let opening = liar.open(&setup, public, &n, j, &mut rng);
        assert!(
            opening_holds(&setup, &liar.seal, public, &n, j, &opening),
            "box {j}"
        );
        Some(opening.y)
    });
    let differs = (0..WORDS)
        .filter(|&j| entries[j] != Some(card.words[j]))
        .count();
    assert!(differs >= 28, "{differs}");
    // That card is a codeword of another scalar: the phone would have certified a card that
    // doesn't restore the share.
    let codeword = codec::decode_unique(&entries).unwrap();
    assert_eq!(codeword.corrected, 0);
    assert_ne!(layout::unpack(&codeword.values()).1, share.share.to_bytes());
    let record = codec::Target::Share {
        image: share.share_image(),
        threshold: 2,
    };
    assert_eq!(
        codec::decode(&entries, Some(&record), None),
        Err(codec::DecodeError::MasksFailed)
    );

    // The commit proofs at the doctored positions can't verify, so the seal is never proven
    // and no opening of it can reach `verify_opening`.
    let proofs = liar.commit_proofs(&setup, &mut rng);
    let first = *moved.iter().min().unwrap();
    assert_eq!(
        setup
            .verify_seal(&context, &liar.seal, &liar.phone, &proofs, &mut ops)
            .err(),
        Some(VerifyError::CommitProof(first))
    );
    for (v, proof) in proofs.iter().enumerate() {
        assert_eq!(
            setup.verify_commit_bit(&mut pinned, v, proof, &mut ops),
            !moved.contains(&v),
            "v = {v}"
        );
    }
    assert!(pinned.proven().is_none());
}

/// T7's positive control: free bits (the hidden random values) are whatever the chip
/// committed, and every proof and opening still passes. Only the mask check notices: read cold
/// the card gives the share with failed masks, and against F it's refused before s·G (owner B7,
/// secfix-attack B1), so the device's restore takes it neither way.
#[test]
fn free_bits_are_unbound_and_only_the_masks_notice() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([6; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[1];
    let context = context(share, &key);
    let card = codec::encode(share, &key).unwrap();
    let bits = layout::committed_bits(&card.values);
    let phone = card.phone_half();
    let public = (context.meta.index, &phone);
    let x: Vec<u32> = (0..COMMITTED)
        .map(|v| {
            if v < FREE {
                rng.gen_range(0..2)
            } else {
                bits.get(v) as u32
            }
        })
        .collect();
    let liar = liar_seal(&setup, &context, x, phone, &mut rng);
    let proofs = liar.commit_proofs(&setup, &mut rng);
    let mut ops = Ops::default();
    let proven = setup
        .verify_seal(&context, &liar.seal, &phone, &proofs, &mut ops)
        .unwrap();
    let n: [u8; 32] = rng.gen();
    let entries: [Option<u16>; WORDS] = core::array::from_fn(|j| {
        if j >= HALF {
            return Some(phone[j - HALF]);
        }
        let opening = liar.open(&setup, public, &n, j, &mut rng);
        assert_eq!(
            setup.verify_opening(&proven, &phone, &n, j, &opening, &mut ops),
            Ok(())
        );
        Some(opening.y)
    });
    let poly = key.point_polynomial();
    assert_eq!(
        codec::decode(&entries, Some(&codec::Target::Polynomial(poly)), None),
        Err(codec::DecodeError::MasksFailed)
    );
    let decoded = codec::decode(&entries, None, None).unwrap();
    assert_eq!(decoded.share, *share);
    assert!(!decoded.masks.self_ok && !decoded.masks.passed());
}

/// PK.2's "the chip can serve any P" and owner B7 (which replaced B4): a chip that picks a
/// phone half that isn't the masks seals it, proves and opens honestly, so the backup check
/// can't catch it. Read cold, the card is the share at its proof-bound index with failed masks;
/// against F or the share's record it's refused on the masks. The device's restore takes it
/// neither way: only the offline repair tool (spec PK.8) can restore it.
#[test]
fn a_phone_half_that_isnt_the_masks_doesnt_restore() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([11; 32]);
    let (key, shares) = wallet(3, &mut rng);
    let share = &shares[2];
    let context = context(share, &key);
    let card = codec::encode(share, &key).unwrap();
    let bits = layout::committed_bits(&card.values);
    let x: Vec<u32> = (0..COMMITTED).map(|v| bits.get(v) as u32).collect();
    // Not the KDF's masks: any 32 words.
    let phone: [u16; HALF] = core::array::from_fn(|_| rng.gen_range(0..2048));
    assert_ne!(phone, card.phone_half());
    let liar = liar_seal(&setup, &context, x, phone, &mut rng);
    let proofs = liar.commit_proofs(&setup, &mut rng);
    let mut ops = Ops::default();
    let proven = setup
        .verify_seal(&context, &liar.seal, &phone, &proofs, &mut ops)
        .unwrap();
    // A D's worth of openings verify.
    let n: [u8; 32] = rng.gen();
    for j in [0, 9, 17, 25, 31] {
        let opening = liar.open(&setup, (context.meta.index, &phone), &n, j, &mut rng);
        assert_eq!(opening.y, liar.word(context.meta.index, &phone, j));
        assert_eq!(
            setup.verify_opening(&proven, &phone, &n, j, &opening, &mut ops),
            Ok(())
        );
    }
    let entries: [Option<u16>; WORDS] = core::array::from_fn(|j| {
        Some(if j < HALF {
            liar.word(context.meta.index, &phone, j)
        } else {
            phone[j - HALF]
        })
    });
    // With F, and with only the share's own record: refused on the masks.
    let poly = key.point_polynomial();
    let record = codec::Target::Share {
        image: share.share_image(),
        threshold: 3,
    };
    for target in [codec::Target::Polynomial(poly), record] {
        assert_eq!(
            codec::decode(&entries, Some(&target), None),
            Err(codec::DecodeError::MasksFailed)
        );
    }
    // Cold: the share, with the masks failed.
    let decoded = codec::decode(&entries, None, None).unwrap();
    assert_eq!(decoded.share, *share);
    assert!(!decoded.masks.self_ok && !decoded.masks.passed());
}

/// Binding (1) against a lying chip (PK.6, `red-soundness/swap.txt`): the phone pins P, and
/// after an app restart the chip serves P′ with openings made to fit P′. Without hP in h_C and
/// open_dom those openings would pass; with it the phone refuses P′ against the pin, and the
/// openings made for P′ fail against the pinned P.
#[test]
fn a_phone_half_swapped_after_the_pin_is_refused() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([12; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[0];
    let context = context(share, &key);
    let card = codec::encode(share, &key).unwrap();
    let bits = layout::committed_bits(&card.values);
    let x: Vec<u32> = (0..COMMITTED).map(|v| bits.get(v) as u32).collect();
    let phone = card.phone_half();
    let liar = liar_seal(&setup, &context, x, phone, &mut rng);
    let proofs = liar.commit_proofs(&setup, &mut rng);
    let mut ops = Ops::default();
    let mut pinned = setup
        .pin(&context, liar.seal.clone(), &phone, &mut ops)
        .unwrap();
    setup
        .verify_commit_proofs(&mut pinned, 0, &proofs[..16], &mut ops)
        .unwrap();
    for from in (16..COMMITTED).step_by(16) {
        let end = (from + 16).min(COMMITTED);
        setup
            .verify_commit_proofs(&mut pinned, from as u16, &proofs[from..end], &mut ops)
            .unwrap();
    }
    let proven = pinned.proven().unwrap();

    // The re-pull serves P′.
    let mut swapped = phone;
    for k in [0, 7, 19] {
        swapped[k] ^= rng.gen_range(1..2048);
    }
    assert_eq!(
        pinned.check_phone_half(&swapped, &mut ops),
        Err(VerifyError::PhoneHalf)
    );
    let n: [u8; 32] = rng.gen();
    let mut changed = 0;
    for j in [2, 11, 23] {
        let opening = liar.open(&setup, (context.meta.index, &swapped), &n, j, &mut rng);
        // The spec's equations alone, under P′, accept it: that's the attack.
        assert!(opening_holds(
            &setup,
            &liar.seal,
            (context.meta.index, &swapped),
            &n,
            j,
            &opening
        ));
        changed += (opening.y != card.words[j]) as usize;
        // The phone refuses P′ itself...
        assert_eq!(
            setup.verify_opening(&proven, &swapped, &n, j, &opening, &mut ops),
            Err(VerifyError::PhoneHalf)
        );
        // ...and an opening made for P′ fails against the pinned P.
        assert!(setup
            .verify_opening(&proven, &phone, &n, j, &opening, &mut ops)
            .is_err());
    }
    // P′ moves the device words the paper doesn't hold.
    assert!(changed > 0);
    // A seal made with hP(P) can't be re-pinned with P′ either.
    assert_eq!(
        setup
            .pin(&context, liar.seal.clone(), &swapped, &mut ops)
            .err(),
        Some(VerifyError::SealHash)
    );
}

/// Every honest carry fits 7 bits, even with every committed bit 1 (the largest sums). That
/// card's scalar is 2^256 − 1, so the share image is (2^256 − 1)·G.
#[test]
fn the_largest_carries_fit_seven_bits() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([7; 32]);
    let all_ones = Scalar::<Secret, Zero>::from_bytes_mod_order([0xff; 32]);
    let image = ShareImage {
        index: Scalar::<Public, Zero>::from(3u32).non_zero().unwrap(),
        image: g!(all_ones * G).normalize(),
    };
    let context = Context::new(KEY_ID, ACCESS_STRUCTURE_ID, image, 2).unwrap();
    let phone = [0x7ff; HALF];
    let liar = liar_seal(&setup, &context, vec![1; COMMITTED], phone, &mut rng);
    let proofs = liar.commit_proofs(&setup, &mut rng);
    let mut ops = Ops::default();
    let proven = setup
        .verify_seal(&context, &liar.seal, &phone, &proofs, &mut ops)
        .unwrap();
    let n: [u8; 32] = rng.gen();
    for j in 0..HALF {
        let opening = liar.open(&setup, (context.meta.index, &phone), &n, j, &mut rng);
        assert_eq!(
            setup.verify_opening(&proven, &phone, &n, j, &opening, &mut ops),
            Ok(())
        );
    }
}

#[test]
fn proofs_are_bound_to_their_place() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([8; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let mut ops = Ops::default();
    let sealed: Vec<_> = shares
        .iter()
        .map(|share| {
            let context = context(share, &key);
            let secrets =
                CardSecrets::new(share, &key, KEY_ID, ACCESS_STRUCTURE_ID, &mut ops).unwrap();
            let seal = setup.seal(&secrets, &mut ops).unwrap();
            (context, secrets, seal)
        })
        .collect();
    let (context, secrets, seal) = &sealed[0];
    // A commit proof of another share's seal, at the same v.
    let other = setup
        .prove_commit_bit(&sealed[1].1, &sealed[1].2, 100, &mut rng, &mut ops)
        .unwrap();
    let phone = secrets.phone_half();
    let mut pinned = setup.pin(context, seal.clone(), phone, &mut ops).unwrap();
    assert!(!setup.verify_commit_bit(&mut pinned, 100, &other, &mut ops));
    let proofs = setup
        .prove_commit_bits(secrets, seal, &mut rng, &mut ops)
        .unwrap();
    let proven = setup
        .verify_seal(context, seal, phone, &proofs, &mut ops)
        .unwrap();

    let n: [u8; 32] = rng.gen();
    let a = setup
        .open(secrets, seal, &n, 3, 0, &mut rng, &mut ops)
        .unwrap();
    let b = setup
        .open(secrets, seal, &n, 3, 1, &mut rng, &mut ops)
        .unwrap();
    // Run 1's proofs under run 0's carries.
    let mixed = Opening {
        proofs: b.proofs.clone(),
        ..a.clone()
    };
    assert_eq!(
        setup.verify_opening(&proven, phone, &n, 3, &mixed, &mut ops),
        Err(VerifyError::CarryProof(0))
    );
    // Two proofs swapped.
    let mut swapped = a.clone();
    swapped.proofs.swap(10, 11);
    assert_eq!(
        setup.verify_opening(&proven, phone, &n, 3, &swapped, &mut ops),
        Err(VerifyError::CarryProof(10))
    );
    // A commit proof in a carry's place, and a carry proof in a commit's place.
    let mut commit_as_carry = a.clone();
    commit_as_carry.proofs[0] = setup
        .prove_commit_bit(secrets, seal, 0, &mut rng, &mut ops)
        .unwrap();
    assert_eq!(
        setup.verify_opening(&proven, phone, &n, 3, &commit_as_carry, &mut ops),
        Err(VerifyError::CarryProof(0))
    );
    assert!(!setup.verify_commit_bit(&mut pinned, 0, &a.proofs[0], &mut ops));
    // Under another h_C the carries' domain changes, so their proofs fail (and that seal
    // doesn't pin). The same for another hP.
    let mut reseal = seal.clone();
    reseal.h_c[0] ^= 1;
    assert_eq!(
        setup.pin(context, reseal.clone(), phone, &mut ops).err(),
        Some(VerifyError::SealHash)
    );
    let hp = spec_phone_hash(phone);
    let q = Point::from_bytes(a.carries[0]).unwrap();
    let dom = proof::open_domain(&seal.h_c, &hp, &n, 3, a.y, &a.carries, &mut ops);
    assert!(setup.verify_bit(&q, BitCtx::carry(dom, 0), &a.proofs[0], &mut ops));
    let dom = proof::open_domain(&reseal.h_c, &hp, &n, 3, a.y, &a.carries, &mut ops);
    assert!(!setup.verify_bit(&q, BitCtx::carry(dom, 0), &a.proofs[0], &mut ops));
    let mut other_hp = hp;
    other_hp[31] ^= 1;
    let dom = proof::open_domain(&seal.h_c, &other_hp, &n, 3, a.y, &a.carries, &mut ops);
    assert!(!setup.verify_bit(&q, BitCtx::carry(dom, 0), &a.proofs[0], &mut ops));
}

/// Spec L1.4 rule 6 and T6: with a stuck TRNG every nonce and σ is still bound to its
/// statement, box and run, so nothing repeats where it would leak.
#[test]
fn a_stuck_rng_repeats_nothing_that_leaks() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([9; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[0];
    let mut ops = Ops::default();
    let secrets = CardSecrets::new(share, &key, KEY_ID, ACCESS_STRUCTURE_ID, &mut ops).unwrap();
    let seal = setup.seal(&secrets, &mut ops).unwrap();
    let mut stuck = Stuck([0; 32]);

    // The same statement gives the same proof: no nonce meets two challenges.
    let p1 = setup
        .prove_commit_bit(&secrets, &seal, 7, &mut stuck, &mut ops)
        .unwrap();
    let p2 = setup
        .prove_commit_bit(&secrets, &seal, 7, &mut stuck, &mut ops)
        .unwrap();
    assert_eq!(p1, p2);

    let n: [u8; 32] = rng.gen();
    let open = |j, run, stuck: &mut Stuck| {
        setup
            .open(&secrets, &seal, &n, j, run, stuck, &mut Ops::default())
            .unwrap()
    };
    let small = |a: &[u8; 33], b: &[u8; 33]| {
        let a = Point::<Normal, Public, NonZero>::from_bytes(*a).unwrap();
        let b = Point::<Normal, Public, NonZero>::from_bytes(*b).unwrap();
        let d = g!(a - b);
        d.is_zero() || d == *G || g!(d + G).is_zero()
    };
    let box3 = open(3, 0, &mut stuck);
    // Another box after a "power cycle", and another run of the same box: the carries differ
    // by more than a multiple of G.
    for other in [open(4, 0, &mut stuck), open(3, 1, &mut stuck)] {
        for (a, b) in box3.carries.iter().zip(&other.carries) {
            assert!(!small(a, b));
        }
    }
    // The same box and run repeats exactly, which shows nothing new.
    assert_eq!(open(3, 0, &mut stuck), box3);
}

/// §7.9's fixed inputs (s = h2s(SHA256("frostsnap card tv1 s")), i = 3, t = 2), derived from
/// the PK text and checked against the implementation. Run with `--nocapture` for candidate
/// T5 pins; they aren't an independent vector until a second implementation (T13) reproduces
/// them.
#[test]
fn spec_7_9_fixed_inputs() {
    let s = Scalar::<Secret, Zero>::from_bytes_mod_order(sha(&[b"frostsnap card tv1 s"]));
    let a0 = Scalar::<Secret, Zero>::from_bytes_mod_order(sha(&[b"frostsnap card tv1 a0"]));
    let index = Scalar::<Public, Zero>::from(3u32).non_zero().unwrap();
    let a1 = s!((s - a0) * { index.invert() });
    let key = SharedKey::from_poly(vec![g!(a0 * G).normalize(), g!(a1 * G).normalize()]);
    let share = SecretShare { index, share: s };
    assert_eq!(key.share_image(index), share.share_image());
    let key = key.non_zero().unwrap();

    let setup = Setup::new();
    let spec = spec_seal(setup.h, &share, &key);
    assert_eq!(context(&share, &key).hash(&mut Ops::default()), spec.ctx);
    let mut ops = Ops::default();
    let secrets = CardSecrets::new(&share, &key, KEY_ID, ACCESS_STRUCTURE_ID, &mut ops).unwrap();
    for v in [0, FREE, V_LINK] {
        assert_eq!(
            secrets.blinding(v, &mut ops).unwrap().mark_zero(),
            spec.r[v],
            "r_{v}"
        );
    }
    assert_eq!(*secrets.phone_hash(), spec.hp);
    assert_eq!(secrets.coin(), spec.coin);
    let seal = setup.seal(&secrets, &mut ops).unwrap();
    assert_eq!(
        (seal.commitments.clone(), seal.coin_commit, seal.h_c),
        (spec.commitments.clone(), spec.coin_commit, spec.h_c)
    );
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    println!("L           {}", hex(&spec_l()));
    println!("ctx         {}", hex(&spec.ctx));
    println!("K_card      {}", hex(&spec.key));
    for v in [0, FREE, V_LINK] {
        println!("r_{v:<9} {}", hex(&spec.r[v].to_bytes()));
    }
    for v in [0, V_LINK, COMMITTED - 1] {
        println!("C_{v:<9} {}", hex(&spec.commitments[v]));
    }
    println!("coin        {}", hex(&spec.coin));
    println!("coin_commit {}", hex(&spec.coin_commit));
    println!("hP          {}", hex(&spec.hp));
    println!("h_C         {}", hex(&spec.h_c));
}
