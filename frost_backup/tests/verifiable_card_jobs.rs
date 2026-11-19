//! Tests for the verifiable card's device-sized jobs and phone-side helpers: the stepped seal
//! and opening jobs, the box draw, the phone's frame builders, pinned seal and phone-half check,
//! the stepped repair search, the redacted `Debug` impls and the lookalike words.

use frost_backup::{
    bip39_words::{closest_lookalikes, lookalike_pool, BIP39_WORDS},
    verifiable_card::{
        codec::{self, DecodeError, Entries, RepairJob, RepairOutcome, RepairStats, Target},
        jobs::{OpenStart, SealJob},
        layout::{self, Bits, CARRIES_PER_BOX, COMMITTED, HALF, WORDS, WORD_BITS},
        phone::{self, OpeningBuilder, PinnedSeal, SealBuilder},
        proof::{
            self, CardError, CardSecrets, Context, Opening, Ops, Seal, Setup, UnitCosts,
            VerifyError, CHECK_BOXES, OPENING_SPLIT, POINTS_PER_FRAME, PROOFS_PER_FRAME, PROOF_LEN,
        },
    },
    Fingerprint, ShareBackup,
};
use rand::{seq::SliceRandom, Rng};
use rand_chacha::ChaCha20Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};
use schnorr_fun::{
    frost::{SecretShare, SharedKey},
    fun::prelude::*,
};
use sha2::{Digest, Sha256};

const KEY_ID: [u8; 32] = [0x11; 32];
const ACCESS_STRUCTURE_ID: [u8; 32] = [0x22; 32];

/// The ESP32-C3 unit costs the bench measured (bench run 1): s·P 38.1 ms, a two-point lincomb
/// 51.6 ms, a SHA-256 block 58 µs. The bench didn't report additions, normalisations or
/// decompressions, so these are generous estimates: the measured 45 ms first slice of a
/// commit proof leaves about 6 ms for its decompression, normalisation and addition together.
const BENCH: UnitCosts = UnitCosts {
    mul: 38_100,
    lincomb: 51_600,
    add: 200,
    normalize: 4_000,
    decompress: 4_000,
    sha_block: 58,
};
/// The longest a job step may take on the device.
const SLICE_US: u64 = 100_000;
/// A bit proof's release is stock verification, two lincombs: 104 ms (design §9 R3).
const RELEASE_US: u64 = 105_000;

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

fn secrets(share: &SecretShare, key: &SharedKey) -> CardSecrets {
    CardSecrets::new(share, key, KEY_ID, ACCESS_STRUCTURE_ID, &mut Ops::default()).unwrap()
}

/// TH(tag, m) = SHA256(SHA256(tag) ‖ SHA256(tag) ‖ m), written out from the spec.
fn th(tag: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let tag: [u8; 32] = Sha256::digest(tag).into();
    let mut hash = Sha256::new();
    hash.update(tag);
    hash.update(tag);
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// hP = TH("frostsnap/card/c1/phone-half", L ‖ BE16(y_33) ‖ … ‖ BE16(y_64)), written out from
/// spec PK.2 and PK.5.
fn phone_hash(phone_half: &[u16; HALF]) -> [u8; 32] {
    let l = th(
        b"frostsnap/card/c1/layout",
        &[&[layout::FMT_C.len() as u8], layout::FMT_C],
    );
    let words: Vec<u8> = phone_half.iter().flat_map(|y| y.to_be_bytes()).collect();
    th(b"frostsnap/card/c1/phone-half", &[&l, &words])
}

/// Gives the same byte to every call: a dead RNG.
struct Dead(u8);

impl RngCore for Dead {
    fn next_u32(&mut self) -> u32 {
        u32::from_ne_bytes([self.0; 4])
    }
    fn next_u64(&mut self) -> u64 {
        u64::from_ne_bytes([self.0; 8])
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        dest.fill(self.0)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for Dead {}

/// Runs a seal job to the end, returning the seal and the ops of each step.
fn seal_stepped(setup: &Setup, secrets: &CardSecrets) -> (Seal, Vec<Ops>) {
    let mut job = SealJob::new();
    let mut steps = Vec::new();
    loop {
        let mut ops = Ops::default();
        let out = job.step(setup, secrets, &mut ops).unwrap();
        steps.push(ops);
        assert_eq!(job.progress(), (steps.len() as u32, job.progress().1));
        if let Some(seal) = out {
            assert_eq!(job.progress().0, job.progress().1);
            return (seal, steps);
        }
    }
}

/// The ops of every step of a sliced opening, by kind.
#[derive(Default)]
struct OpenSteps {
    start: Vec<Ops>,
    finish: Ops,
    checks: Vec<Ops>,
    /// The three slices of each carry proof.
    proofs: Vec<[Ops; 3]>,
}

/// Opens box `j` one step at a time, as a device would.
#[allow(clippy::too_many_arguments)]
fn open_sliced(
    setup: &Setup,
    secrets: &CardSecrets,
    h_c: &[u8; 32],
    n: &[u8; 32],
    j: usize,
    run: u8,
    rng: &mut (impl RngCore + CryptoRng),
) -> (Opening, OpenSteps) {
    let mut steps = OpenSteps::default();
    let mut ops = Ops::default();
    let mut start = OpenStart::new(setup, secrets, h_c, n, j, run, rng, &mut ops).unwrap();
    steps.start.push(ops);
    loop {
        let mut ops = Ops::default();
        let done = start.step(secrets, &mut ops).unwrap();
        steps.start.push(ops);
        if done {
            break;
        }
    }
    let mut job = start.finish(&mut steps.finish).unwrap();
    loop {
        let mut ops = Ops::default();
        let done = job.check_next(&mut ops).unwrap();
        steps.checks.push(ops);
        if done {
            break;
        }
    }
    let mut checked = job.into_checked().unwrap();
    while checked.next_carry().is_some() {
        let mut ops = [Ops::default(); 3];
        let prover = checked.carry_prover(rng, &mut ops[0]).unwrap();
        let unverified = prover.respond(&mut ops[1]).unwrap();
        checked.add_proof(unverified, &mut ops[2]).unwrap();
        steps.proofs.push(ops);
    }
    (checked.finish().unwrap(), steps)
}

#[test]
fn seal_job_equals_the_seal() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    for case in 0..20 {
        let (key, shares) = wallet(2 + case % 3, &mut rng);
        let share = &shares[case % shares.len()];
        let secrets = secrets(share, &key);
        let mut ops = Ops::default();
        let seal = setup.seal(&secrets, &mut ops).unwrap();
        let (stepped, steps) = seal_stepped(&setup, &secrets);
        // Setup::seal runs this job, so this holds by construction; the spec rebuild below is
        // what pins the bytes.
        assert_eq!(stepped, seal, "case {case}");
        // The same work, cut up.
        let total = steps.iter().fold(Ops::default(), |sum, step| Ops {
            mul: sum.mul + step.mul,
            lincomb: sum.lincomb + step.lincomb,
            add: sum.add + step.add,
            normalize: sum.normalize + step.normalize,
            decompress: sum.decompress + step.decompress,
            sha_blocks: sum.sha_blocks + step.sha_blocks,
        });
        assert_eq!(total, ops);

        // And as the spec writes it, from the public pieces.
        let commitments: Vec<[u8; 33]> = (0..COMMITTED)
            .map(|v| {
                setup
                    .commit(&secrets, v, &mut Ops::default())
                    .unwrap()
                    .to_bytes()
            })
            .collect();
        assert_eq!(seal.commitments, commitments);
        let coin_commit = th(
            b"frostsnap/card/commit-v1/box-coin-commit",
            &[&secrets.coin()],
        );
        assert_eq!(
            coin_commit,
            proof::coin_commitment(&secrets.coin(), &mut Ops::default())
        );
        assert_eq!(seal.coin_commit, coin_commit);
        let ctx = context(share, &key).hash(&mut Ops::default());
        let phone_half = codec::encode(share, &key).unwrap().phone_half();
        let hp = phone_hash(&phone_half);
        assert_eq!(*secrets.phone_hash(), hp);
        assert_eq!(hp, proof::phone_half_hash(&phone_half, &mut Ops::default()));
        let mut parts: Vec<&[u8]> = vec![&ctx, &coin_commit, &hp];
        parts.extend(commitments.iter().map(|c| &c[..]));
        assert_eq!(seal.h_c, th(b"frostsnap/card/c1/seal", &parts));
    }
}

#[test]
fn a_spent_seal_job_refuses_to_step() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([2; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let secrets = secrets(&shares[0], &key);
    let mut job = SealJob::default();
    while job
        .step(&setup, &secrets, &mut Ops::default())
        .unwrap()
        .is_none()
    {}
    assert_eq!(
        job.step(&setup, &secrets, &mut Ops::default()),
        Err(CardError::SelfCheck)
    );
}

#[test]
fn sliced_opening_verifies_and_matches_the_monolithic_one() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([3; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[1];
    let context = context(share, &key);
    let secrets = secrets(share, &key);
    let mut ops = Ops::default();
    let seal = setup.seal(&secrets, &mut ops).unwrap();
    let proofs = setup
        .prove_commit_bits(&secrets, &seal, &mut rng, &mut ops)
        .unwrap();
    let proven = setup
        .verify_seal(&context, &seal, secrets.phone_half(), &proofs, &mut ops)
        .unwrap();
    let words = codec::encode(share, &key).unwrap().words;
    let n: [u8; 32] = rng.gen();

    for j in [0, 12, 31] {
        let seed: [u8; 32] = rng.gen();
        let (opening, steps) = open_sliced(
            &setup,
            &secrets,
            &seal.h_c,
            &n,
            j,
            1,
            &mut ChaCha20Rng::from_seed(seed),
        );
        // new, ρ again, the σ step, then one step per Q; one check per β.
        assert_eq!(steps.start.len(), 3 + CARRIES_PER_BOX);
        assert_eq!(steps.checks.len(), WORD_BITS);
        assert_eq!(opening.y, words[j]);
        assert_eq!(
            setup.verify_opening(&proven, secrets.phone_half(), &n, j, &opening, &mut ops),
            Ok(())
        );
        // The monolithic path, with the same RNG. Setup::open runs these same steps, so this
        // holds by construction and checks only the wiring. What pins the bytes is elsewhere:
        // the known-answer vectors in proof.rs, and verifiable_card_review.rs, which rebuilds
        // each opening's Q's from the spec.
        let whole = setup
            .open(
                &secrets,
                &seal,
                &n,
                j,
                1,
                &mut ChaCha20Rng::from_seed(seed),
                &mut ops,
            )
            .unwrap();
        assert_eq!(whole, opening);
    }
}

#[test]
fn opening_steps_refuse_to_skip_ahead() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([4; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let secrets = secrets(&shares[0], &key);
    let (h_c, n) = ([5; 32], [6; 32]);
    let mut ops = Ops::default();
    // Only device words have openings, and the refusal comes before the RNG is touched.
    for j in [HALF, WORDS - 1, WORDS, usize::MAX] {
        let before = rng.clone();
        assert!(matches!(
            OpenStart::new(&setup, &secrets, &h_c, &n, j, 1, &mut rng, &mut ops),
            Err(CardError::NoSuchBox)
        ));
        assert_eq!(rng, before);
    }
    let mut start = OpenStart::new(&setup, &secrets, &h_c, &n, 3, 1, &mut rng, &mut ops).unwrap();
    start.step(&secrets, &mut ops).unwrap();
    assert!(matches!(start.finish(&mut ops), Err(CardError::SelfCheck)));

    let mut start = OpenStart::new(&setup, &secrets, &h_c, &n, 3, 1, &mut rng, &mut ops).unwrap();
    while !start.step(&secrets, &mut ops).unwrap() {}
    let mut job = start.finish(&mut ops).unwrap();
    job.check_next(&mut ops).unwrap();
    assert!(matches!(job.into_checked(), Err(CardError::SelfCheck)));

    // Other secrets than the job started with: the witness check stops it.
    let (other_key, other_shares) = wallet(2, &mut rng);
    let other = self::secrets(&other_shares[0], &other_key);
    let mut start = OpenStart::new(&setup, &secrets, &h_c, &n, 3, 1, &mut rng, &mut ops).unwrap();
    assert_eq!(start.step(&other, &mut ops), Err(CardError::SelfCheck));
}

#[test]
fn every_step_fits_a_device_slice() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([5; 32]);
    let (key, shares) = wallet(3, &mut rng);
    let secrets = secrets(&shares[2], &key);
    // The longest step of each kind, in order. Run with `--nocapture` to see them.
    let mut longest: Vec<(&str, u64)> = Vec::new();
    let mut fits = |what: &'static str, ops: &Ops, budget: u64| {
        let us = ops.cost_us(&BENCH);
        assert!(us <= budget, "{what}: {us} us, {ops:?}");
        match longest.iter_mut().find(|(name, _)| *name == what) {
            Some((_, max)) => *max = (*max).max(us),
            None => longest.push((what, us)),
        }
        us
    };

    let (seal, steps) = seal_stepped(&setup, &secrets);
    assert_eq!(steps.len(), 201);
    let seal_max = steps
        .iter()
        .map(|ops| fits("seal step", ops, SLICE_US))
        .max()
        .unwrap();
    assert!(seal_max > 60_000, "two C's a step: {seal_max} us");

    // A commit proof's three slices.
    for v in [0, 300, COMMITTED - 1] {
        let mut ops = [Ops::default(); 3];
        let prover = setup
            .commit_prover(&secrets, &seal, v, &mut rng, &mut ops[0])
            .unwrap();
        prover
            .respond(&mut ops[1])
            .unwrap()
            .release(&mut ops[2])
            .unwrap();
        fits("commit proof start", &ops[0], SLICE_US);
        fits("commit proof respond", &ops[1], SLICE_US);
        fits("commit proof release", &ops[2], RELEASE_US);
    }

    let (_, steps) = open_sliced(&setup, &secrets, &seal.h_c, &[9; 32], 20, 1, &mut rng);
    // On C′ the first two steps each derive the 311 blindings, about 935 SHA-256 blocks: they
    // fit a slice again.
    for ops in &steps.start {
        fits("opening start step", ops, SLICE_US);
    }
    fits("opening finish", &steps.finish, SLICE_US);
    for ops in &steps.checks {
        fits("witness check", ops, SLICE_US);
    }
    for ops in &steps.proofs {
        fits("carry proof start", &ops[0], SLICE_US);
        fits("carry proof respond", &ops[1], SLICE_US);
        fits("carry proof release", &ops[2], RELEASE_US);
    }
    let slices = steps.start.len() + 1 + steps.checks.len() + 3 * steps.proofs.len();
    assert_eq!(slices, 3 + CARRIES_PER_BOX + 1 + 11 + 3 * CARRIES_PER_BOX);
    for (what, us) in &longest {
        println!("{what:<22} {:>6.1} ms", *us as f64 / 1e3);
    }
}

#[test]
fn boxes_are_five_distinct_and_pinned() {
    let (h_c, n, coin) = ([0x01; 32], [0x09; 32], [0x03; 32]);
    let mut ops = Ops::default();
    let d = proof::derive_boxes(&h_c, &n, &coin, &mut ops);
    assert_eq!(d, proof::derive_boxes(&h_c, &n, &coin, &mut Ops::default()));
    // One block, of TH's 97-byte message: the tag block and 3 more.
    assert_eq!(ops.sha_blocks, 4);

    // As B5 writes it, under the proof-v3 tag (PK-O3, build plan A9).
    let mut spec = Vec::new();
    for k in 0..=u8::MAX {
        for b in th(b"frostsnap/card/proof-v3/boxes", &[&h_c, &n, &coin, &[k]]) {
            if !spec.contains(&(b & 31)) && spec.len() < CHECK_BOXES {
                spec.push(b & 31);
            }
        }
    }
    assert_eq!(d.to_vec(), spec);
    // Pinned from this implementation, so an accidental change shows.
    assert_eq!(d, [6, 8, 14, 11, 1]);

    let mut rng = ChaCha20Rng::from_seed([6; 32]);
    for _ in 0..200 {
        let d = proof::derive_boxes(&rng.gen(), &rng.gen(), &rng.gen(), &mut ops);
        assert!(d.iter().all(|&j| (j as usize) < HALF), "D is in words 1-32");
        for (k, j) in d.iter().enumerate() {
            assert!(!d[k + 1..].contains(j), "{d:?}");
        }
    }
}

#[test]
fn the_coin_must_match_its_commitment() {
    let coin = [0x5a; 32];
    let commit = proof::coin_commitment(&coin, &mut Ops::default());
    assert!(proof::coin_matches(&commit, &coin, &mut Ops::default()));
    for bit in [0, 7, 100, 255] {
        let mut flipped = coin;
        flipped[bit / 8] ^= 1 << (bit % 8);
        assert!(!proof::coin_matches(&commit, &flipped, &mut Ops::default()));
        let mut bad_commit = commit;
        bad_commit[bit / 8] ^= 1 << (bit % 8);
        assert!(!proof::coin_matches(
            &bad_commit,
            &coin,
            &mut Ops::default()
        ));
    }
}

#[test]
fn there_is_no_bit_past_the_last() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([7; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let secrets = secrets(&shares[0], &key);
    let mut ops = Ops::default();
    let seal = setup.seal(&secrets, &mut ops).unwrap();
    for v in [COMMITTED, COMMITTED + 9, usize::MAX] {
        assert!(matches!(
            setup.commit_prover(&secrets, &seal, v, &mut rng, &mut ops),
            Err(CardError::NoSuchBit)
        ));
        assert_eq!(
            setup.prove_commit_bit(&secrets, &seal, v, &mut rng, &mut ops),
            Err(CardError::NoSuchBit)
        );
    }
    // A short seal is refused, not indexed past its end.
    let short = Seal {
        commitments: seal.commitments[..10].to_vec(),
        ..seal.clone()
    };
    assert!(matches!(
        setup.commit_prover(&secrets, &short, 10, &mut rng, &mut ops),
        Err(CardError::SelfCheck)
    ));
}

/// A sealed share, its phone half, its commit proofs and an opening of a word of a D (every
/// word of D is a computed device word, opened by carries).
struct Fixture {
    setup: Setup,
    context: Context,
    phone_half: [u16; HALF],
    seal: Seal,
    proofs: Vec<[u8; PROOF_LEN]>,
    n: [u8; 32],
    d: [u8; CHECK_BOXES],
    opening: Opening,
}

fn fixture() -> Fixture {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([8; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[0];
    let context = context(share, &key);
    let secrets = secrets(share, &key);
    let mut ops = Ops::default();
    let seal = setup.seal(&secrets, &mut ops).unwrap();
    let proofs = setup
        .prove_commit_bits(&secrets, &seal, &mut rng, &mut ops)
        .unwrap();
    let n: [u8; 32] = rng.gen();
    let d = proof::derive_boxes(&seal.h_c, &n, &secrets.coin(), &mut ops);
    let opening = setup
        .open(&secrets, &seal, &n, d[0] as usize, 1, &mut rng, &mut ops)
        .unwrap();
    Fixture {
        setup,
        context,
        phone_half: *secrets.phone_half(),
        seal,
        proofs,
        n,
        d,
        opening,
    }
}

#[test]
fn the_phone_pins_proves_and_checks_an_opening() {
    let f = fixture();
    let mut ops = Ops::default();
    let mut rng = ChaCha20Rng::from_seed([9; 32]);

    // The points arrive in frames, in any order.
    let mut builder = SealBuilder::default();
    assert_eq!(builder.next_missing(), Some((0, POINTS_PER_FRAME as u8)));
    let mut frames: Vec<u16> = (0..COMMITTED as u16).step_by(POINTS_PER_FRAME).collect();
    frames.shuffle(&mut rng);
    for from in frames {
        let end = (from as usize + POINTS_PER_FRAME).min(COMMITTED);
        builder
            .add(from, &f.seal.commitments[from as usize..end])
            .unwrap();
    }
    assert_eq!(builder.next_missing(), None);
    let seal = builder.finish(f.seal.coin_commit, f.seal.h_c).unwrap();
    assert_eq!(seal, f.seal);

    let mut pinned = f
        .setup
        .pin(&f.context, seal.clone(), &f.phone_half, &mut ops)
        .unwrap();
    assert!(pinned.proven().is_none());
    while let Some((from, count)) = pinned.next_missing(PROOFS_PER_FRAME as u8) {
        assert!(count as usize <= PROOFS_PER_FRAME);
        let range = from as usize..from as usize + count as usize;
        f.setup
            .verify_commit_proofs(&mut pinned, from, &f.proofs[range], &mut ops)
            .unwrap();
    }
    assert_eq!(pinned.verified().len(), COMMITTED);
    // A re-sent frame verifies again.
    f.setup
        .verify_commit_proofs(&mut pinned, 16, &f.proofs[16..32], &mut ops)
        .unwrap();
    let proven = pinned.proven().unwrap();
    assert_eq!(proven.seal(), &f.seal);

    // The phone's own record gives it back. The record holds no P: P is pulled again and
    // checked against the pin.
    let recorded = PinnedSeal::from_record(seal.clone(), &f.context, *pinned.verified());
    assert_eq!(recorded, pinned);
    assert!(recorded.proven().is_some());
    let hp = proof::phone_half_hash(&f.phone_half, &mut ops);
    assert_eq!(recorded.check_phone_half(&f.phone_half, &mut ops), Ok(hp));
    let mut partly = *pinned.verified();
    partly.set(200, false);
    let partial = PinnedSeal::from_record(seal.clone(), &f.context, partly);
    assert!(partial.proven().is_none());
    assert_eq!(partial.next_missing(16), Some((200, 1)));
    // A stray bit past the 311 C's doesn't stand in for a missing one.
    let mut stray = *pinned.verified();
    stray.set(0, false);
    stray.set(COMMITTED, true);
    assert_eq!(stray.len(), COMMITTED);
    let stray = PinnedSeal::from_record(seal.clone(), &f.context, stray);
    assert!(stray.proven().is_none());
    assert_eq!(stray.next_missing(16), Some((0, 1)));

    // The opening, in its three parts.
    let mut opening = OpeningBuilder::new(f.opening.j);
    let (j, y, carries) = f.opening.carries_part();
    opening.add_carries(j, y, carries).unwrap();
    for part in [1, 2] {
        assert!(!opening.is_complete());
        let (first, proofs) = f.opening.proofs_part(part).unwrap();
        opening.add_proofs(j, first, proofs).unwrap();
    }
    assert!(opening.is_complete());
    let opening = opening.finish().unwrap();
    assert_eq!(opening, f.opening);
    assert_eq!(
        f.setup
            .check_opening(&proven, &f.phone_half, &f.n, &f.d, j, &opening, &mut ops),
        Ok(opening.y)
    );
    // Only boxes in D.
    let outside = (0..WORDS as u8).find(|j| !f.d.contains(j)).unwrap();
    let mut moved = opening.clone();
    moved.j = outside;
    assert_eq!(
        f.setup.check_opening(
            &proven,
            &f.phone_half,
            &f.n,
            &f.d,
            outside,
            &moved,
            &mut ops
        ),
        Err(VerifyError::NotInD)
    );
    // Another box of D than the opening's.
    assert_eq!(
        f.setup.check_opening(
            &proven,
            &f.phone_half,
            &f.n,
            &f.d,
            f.d[1],
            &opening,
            &mut ops
        ),
        Err(VerifyError::WrongBox)
    );
    // P pulled again after an app restart: a P′ is refused against the pin before the opening
    // is looked at, and the opening checked against it is refused the same way.
    let mut swapped = f.phone_half;
    swapped[3] ^= 0x155;
    assert_eq!(
        recorded.check_phone_half(&swapped, &mut ops),
        Err(VerifyError::PhoneHalf)
    );
    assert_eq!(
        f.setup
            .check_opening(&proven, &swapped, &f.n, &f.d, j, &opening, &mut ops),
        Err(VerifyError::PhoneHalf)
    );
}

#[test]
fn the_phone_refuses_bad_seal_frames() {
    let f = fixture();
    let points = &f.seal.commitments;
    let mut builder = SealBuilder::default();
    // Past the last C, too many, none, and an offset that would overflow.
    assert_eq!(
        builder.add(COMMITTED as u16 - 11, &points[..12]),
        Err(VerifyError::Frame)
    );
    assert_eq!(
        builder.add(0, &points[..POINTS_PER_FRAME + 1]),
        Err(VerifyError::Frame)
    );
    assert_eq!(builder.add(0, &[]), Err(VerifyError::Frame));
    assert_eq!(builder.add(u16::MAX, &points[..1]), Err(VerifyError::Frame));
    assert_eq!(builder, SealBuilder::default());

    builder.add(0, &points[..32]).unwrap();
    // An overlap, at either end.
    assert_eq!(builder.add(16, &points[16..48]), Err(VerifyError::Frame));
    assert_eq!(builder.add(31, &points[31..33]), Err(VerifyError::Frame));
    builder
        .add(COMMITTED as u16 - 20, &points[COMMITTED - 20..])
        .unwrap();
    assert_eq!(builder.next_missing(), Some((32, 32)));
    assert_eq!(
        builder.clone().finish(f.seal.coin_commit, f.seal.h_c),
        Err(VerifyError::Shape)
    );

    // A seal pinned under another context, with another P, with a P word past 11 bits, or
    // with a tampered C.
    let mut ops = Ops::default();
    let mut other = f.context;
    other.key_id = [0x33; 32];
    assert_eq!(
        f.setup.pin(&other, f.seal.clone(), &f.phone_half, &mut ops),
        Err(VerifyError::SealHash)
    );
    let mut swapped = f.phone_half;
    swapped.swap(0, 1);
    assert_ne!(swapped, f.phone_half);
    assert_eq!(
        f.setup.pin(&f.context, f.seal.clone(), &swapped, &mut ops),
        Err(VerifyError::SealHash)
    );
    let mut wide = f.phone_half;
    wide[20] = u16::MAX;
    assert_eq!(
        f.setup.pin(&f.context, f.seal.clone(), &wide, &mut ops),
        Err(VerifyError::WordRange)
    );
    let mut tampered = f.seal.clone();
    tampered.commitments[200] = tampered.commitments[201];
    assert_eq!(
        f.setup.pin(&f.context, tampered, &f.phone_half, &mut ops),
        Err(VerifyError::Link)
    );
    let mut short = f.seal.clone();
    short.commitments.pop();
    assert_eq!(
        f.setup.pin(&f.context, short, &f.phone_half, &mut ops),
        Err(VerifyError::Shape)
    );
}

#[test]
fn the_phone_refuses_bad_proof_frames() {
    let f = fixture();
    let mut ops = Ops::default();
    let mut pinned = f
        .setup
        .pin(&f.context, f.seal.clone(), &f.phone_half, &mut ops)
        .unwrap();
    let proofs = &f.proofs;
    for (from, frame) in [
        (0, &proofs[..PROOFS_PER_FRAME + 1]),
        (0, &proofs[..0]),
        (301, &proofs[300..]),
        (u16::MAX, &proofs[..1]),
    ] {
        assert_eq!(
            f.setup
                .verify_commit_proofs(&mut pinned, from, frame, &mut ops),
            Err(VerifyError::Frame)
        );
    }
    // A proof for another C fails, and nothing in its frame is noted.
    let mut swapped = proofs[32..48].to_vec();
    swapped.swap(3, 4);
    assert_eq!(
        f.setup
            .verify_commit_proofs(&mut pinned, 32, &swapped, &mut ops),
        Err(VerifyError::CommitProof(35))
    );
    assert_eq!(pinned.verified(), &Bits::default());
    // A record with more C's than the seal has is never proven.
    let mut long = f.seal.clone();
    long.commitments.push(f.seal.commitments[0]);
    let mut all = Bits::default();
    (0..COMMITTED).for_each(|v| all.set(v, true));
    assert!(PinnedSeal::from_record(long, &f.context, all)
        .proven()
        .is_none());
}

#[test]
fn the_phone_refuses_bad_opening_parts() {
    let f = fixture();
    let (j, y, carries) = f.opening.carries_part();
    let (_, first) = f.opening.proofs_part(1).unwrap();
    let (_, second) = f.opening.proofs_part(2).unwrap();
    assert_eq!(f.opening.proofs_part(0), None);
    assert_eq!(f.opening.proofs_part(3), None);
    let other = (j + 1) % WORDS as u8;

    let mut builder = OpeningBuilder::new(j);
    assert_eq!(
        builder.add_carries(other, y, carries),
        Err(VerifyError::WrongBox)
    );
    assert_eq!(
        builder.add_carries(j, 1 << WORD_BITS, carries),
        Err(VerifyError::WordRange)
    );
    assert_eq!(
        builder.add_carries(j, u16::MAX, carries),
        Err(VerifyError::WordRange)
    );
    assert_eq!(
        builder.add_carries(j, y, &carries[1..]),
        Err(VerifyError::Frame)
    );
    // Proofs before the Q's.
    assert_eq!(builder.add_proofs(j, 0, first), Err(VerifyError::Frame));
    assert_eq!(builder, OpeningBuilder::new(j));
    builder.add_carries(j, y, carries).unwrap();
    assert_eq!(builder.add_carries(j, y, carries), Err(VerifyError::Frame));
    // Out of order, the wrong size, the wrong box.
    assert_eq!(
        builder.add_proofs(j, OPENING_SPLIT as u8, second),
        Err(VerifyError::Frame)
    );
    assert_eq!(
        builder.add_proofs(j, 0, &[first, second].concat()),
        Err(VerifyError::Frame)
    );
    assert_eq!(
        builder.add_proofs(j, 0, &first[1..]),
        Err(VerifyError::Frame)
    );
    assert_eq!(
        builder.add_proofs(other, 0, first),
        Err(VerifyError::WrongBox)
    );
    builder.add_proofs(j, 0, first).unwrap();
    assert_eq!(builder.add_proofs(j, 0, first), Err(VerifyError::Frame));
    assert!(builder.clone().finish().is_none());
    assert!(!builder.is_complete());
    builder.add_proofs(j, OPENING_SPLIT as u8, second).unwrap();
    assert!(builder.is_complete());
    assert_eq!(
        builder
            .clone()
            .add_proofs(j, CARRIES_PER_BOX as u8, &second[..1]),
        Err(VerifyError::Frame)
    );
    assert_eq!(builder.finish(), Some(f.opening.clone()));
}

#[test]
fn a_dead_rng_still_gives_each_run_its_own_sigmas() {
    let setup = Setup::new();
    let mut rng = ChaCha20Rng::from_seed([10; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let secrets = secrets(&shares[1], &key);
    let (h_c, n) = (rng.gen(), rng.gen());
    let run = |run: u8| open_sliced(&setup, &secrets, &h_c, &n, 9, run, &mut Dead(0x01)).0;
    let (first, again, second) = (run(1), run(1), run(2));
    // Nothing but the run differs, so with a dead RNG the run alone moves the σ's.
    assert_eq!(first, again);
    assert_eq!(first.y, second.y);
    for (q, other) in first.carries.iter().zip(&second.carries) {
        assert_ne!(q, other);
    }
}

/// Changes the words in `boxes` to other words.
fn corrupt(entries: &mut Entries, boxes: &[usize], rng: &mut impl Rng) {
    for &j in boxes {
        let word = entries[j].unwrap();
        entries[j] = Some(word ^ rng.gen_range(1..2048));
    }
}

fn repair_stepped(
    entries: &Entries,
    wrong: usize,
    target: &Target,
    per_step: u32,
) -> (RepairOutcome, RepairStats) {
    let mut job = RepairJob::new(entries, wrong).unwrap();
    loop {
        let images = job.stats().images;
        let outcome = job.step(target, per_step);
        assert!(job.stats().images <= images + 1, "one s·G a step at most");
        if let Some(outcome) = outcome {
            return (outcome, *job.stats());
        }
    }
}

#[test]
fn the_repair_job_steps_through_the_search() {
    let mut rng = ChaCha20Rng::from_seed([11; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[2];
    let card = codec::encode(share, &key).unwrap();
    let target = Target::Polynomial(key.point_polynomial());
    for e in 1..=codec::MAX_REPAIR_WRONG {
        let mut entries = card.words.map(Some);
        let boxes: Vec<usize> = (0..WORDS).collect();
        corrupt(
            &mut entries,
            &boxes
                .choose_multiple(&mut rng, e)
                .copied()
                .collect::<Vec<_>>(),
            &mut rng,
        );
        let mut stats = RepairStats::default();
        let whole = codec::repair_search(&entries, e, &target, &mut stats, &mut |_| false).unwrap();
        match &whole {
            RepairOutcome::Found(decoded) => assert_eq!(decoded.share, *share),
            other => panic!("e = {e}: {other:?}"),
        }
        for per_step in [1, 7, 40] {
            assert_eq!(
                repair_stepped(&entries, e, &target, per_step),
                (whole.clone(), stats),
                "e = {e}, {per_step} per step"
            );
        }
        // A search for fewer wrong words than there are tries every set and finds nothing.
        if e > 1 {
            let mut stats = RepairStats::default();
            let whole =
                codec::repair_search(&entries, 1, &target, &mut stats, &mut |_| false).unwrap();
            assert_eq!(whole, RepairOutcome::Exhausted);
            assert_eq!(stats.subsets, WORDS as u64);
            assert_eq!(repair_stepped(&entries, 1, &target, 7), (whole, stats));
        }
    }

    // The job stops at the cap, though the search itself goes to 3.
    assert_eq!(
        RepairJob::new(&card.words.map(Some), codec::MAX_REPAIR_WRONG + 1).map(|_| ()),
        Err(DecodeError::TooManyErasures)
    );

    // With unreadable boxes, and refusals.
    let mut entries = card.words.map(Some);
    entries[4] = None;
    corrupt(&mut entries, &[17, 25], &mut rng);
    let mut stats = RepairStats::default();
    let whole = codec::repair_search(&entries, 2, &target, &mut stats, &mut |_| false).unwrap();
    assert!(matches!(whole, RepairOutcome::Found(_)));
    assert_eq!(repair_stepped(&entries, 2, &target, 40), (whole, stats));
    assert_eq!(
        RepairJob::new(&entries, 3).map(|_| ()),
        Err(DecodeError::TooManyErasures)
    );
    assert_eq!(
        RepairJob::new(&entries, usize::MAX).map(|_| ()),
        Err(DecodeError::TooManyErasures)
    );
    let mut stats = RepairStats::default();
    assert_eq!(
        codec::repair_search(&entries, usize::MAX, &target, &mut stats, &mut |_| false),
        Err(DecodeError::TooManyErasures)
    );
    entries[0] = Some(2048);
    assert_eq!(
        RepairJob::new(&entries, 1).map(|_| ()),
        Err(DecodeError::WordRange)
    );
    // A step of no sets still tries one, so a caller can't stall the search.
    let mut job = RepairJob::new(&card.words.map(Some), 1).unwrap();
    // The card is whole, so the first set rebuilds it.
    assert!(matches!(
        job.step(&target, 0),
        Some(RepairOutcome::Found(decoded)) if decoded.share == *share
    ));
    assert_eq!(job.stats().subsets, 1);
}

#[test]
fn encode_checked_is_encode() {
    let mut rng = ChaCha20Rng::from_seed([12; 32]);
    for threshold in [1, 2, 3, 5] {
        let (key, shares) = wallet(threshold, &mut rng);
        let share = &shares[0];
        assert_eq!(
            codec::encode_checked(share, &key),
            codec::encode(share, &key)
        );
    }
}

#[test]
fn debug_shows_no_secret() {
    let mut rng = ChaCha20Rng::from_seed([13; 32]);
    let (key, shares) = wallet(2, &mut rng);
    let share = &shares[1];
    let card = codec::encode(share, &key).unwrap();
    let shown = format!("{card:?} {card:#?}");
    assert_eq!(format!("{card:?}"), "Card { .. }");
    for &word in &card.words {
        assert!(!shown.contains(BIP39_WORDS[word as usize]));
    }

    let mut entries = card.words.map(Some);
    corrupt(&mut entries, &[3], &mut rng);
    let target = Target::Polynomial(key.point_polynomial());
    let decoded = codec::decode(&entries, Some(&target), None).unwrap();
    assert_eq!(decoded.share, *share);
    let shown = format!("{decoded:?} {:?}", decoded.codeword);
    let scalar = hex(&share.share.to_bytes());
    assert!(!shown.contains(&scalar), "{shown}");
    assert!(!shown.contains("coeffs"), "{shown}");
    // The values in any form Debug would print them.
    assert!(!shown.contains(&format!("{:?}", card.values)), "{shown}");
    assert!(shown.contains("corrected: 8"), "{shown}");

    let job = RepairJob::new(&entries, 1).unwrap();
    let shown = format!("{job:?}");
    assert!(shown.starts_with("RepairJob { stats: "), "{shown}");
    assert!(!shown.contains("Some("), "{shown}");
}

/// `frostsnap_widgets/src/backup/distractor.rs` as it was before the move.
mod vendored {
    use frost_backup::bip39_words::BIP39_WORDS;

    fn levenshtein(a: &[u8], b: &[u8]) -> usize {
        const MAX_LEN: usize = 9;
        let mut prev = [0usize; MAX_LEN];
        let mut curr = [0usize; MAX_LEN];

        let b_len = b.len();
        for (j, slot) in prev.iter_mut().enumerate().take(b_len + 1) {
            *slot = j;
        }

        for i in 1..=a.len() {
            curr[0] = i;
            for j in 1..=b_len {
                let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
                curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
            }
            core::mem::swap(&mut prev, &mut curr);
        }
        prev[b_len]
    }

    fn shared_suffix_len(a: &[u8], b: &[u8]) -> usize {
        a.iter()
            .rev()
            .zip(b.iter().rev())
            .take_while(|(x, y)| x == y)
            .count()
    }

    fn distractor_score(target: &[u8], candidate: &[u8]) -> i32 {
        let edit_dist = levenshtein(target, candidate) as i32;
        let suffix = shared_suffix_len(target, candidate) as i32;
        edit_dist * 3 - suffix * 2
    }

    pub fn find_closest_distractors(correct_index: u16) -> [u16; 2] {
        let target = BIP39_WORDS[correct_index as usize].as_bytes();

        let mut best = [(i32::MAX, 0u16); 2];

        for (i, &word) in BIP39_WORDS.iter().enumerate() {
            if i as u16 == correct_index {
                continue;
            }
            let score = distractor_score(target, word.as_bytes());

            if score < best[0].0 {
                best[1] = best[0];
                best[0] = (score, i as u16);
            } else if score < best[1].0 {
                best[1] = (score, i as u16);
            }
        }

        [best[0].1, best[1].1]
    }
}

#[test]
fn lookalikes_are_the_old_distractors() {
    let mut rng = ChaCha20Rng::from_seed([14; 32]);
    let mut indices: Vec<u16> = (0..47).map(|_| rng.gen_range(0..2048)).collect();
    indices.extend([0, 1, 2047]);
    for index in indices {
        let lookalikes = closest_lookalikes(index).unwrap();
        assert_eq!(lookalikes, vendored::find_closest_distractors(index));
        assert!(!lookalikes.contains(&index));
        assert_ne!(lookalikes[0], lookalikes[1]);
    }
    for index in [2048, u16::MAX] {
        assert_eq!(closest_lookalikes(index), None);
    }
}

#[test]
fn the_pool_starts_with_the_two_closest() {
    for index in (0..2048).step_by(7).chain([2047]) {
        let pool = lookalike_pool(index).unwrap();
        assert_eq!(pool[..2], closest_lookalikes(index).unwrap());
        assert!(!pool.contains(&index));
        for (k, word) in pool.iter().enumerate() {
            assert!(!pool[..k].contains(word));
        }
    }
    assert_eq!(lookalike_pool(2048), None);
}

#[test]
fn d_stays_in_the_device_half() {
    let mut rng = ChaCha20Rng::from_seed([15; 32]);
    let mut ops = Ops::default();
    for _ in 0..10_000 {
        let d = proof::derive_boxes(&rng.gen(), &rng.gen(), &rng.gen(), &mut ops);
        for (k, j) in d.iter().enumerate() {
            assert!((*j as usize) < HALF, "{d:?}");
            assert!(!d[..k].contains(j), "{d:?}");
        }
    }
}

#[test]
fn the_phone_word_hashes_pin_their_order() {
    let half: [u16; HALF] = core::array::from_fn(|k| k as u16 * 61);
    let h_c = [7; 32];
    let h = phone::h32(&h_c, &half);
    let entries: Vec<(u8, u16)> = (32..64).zip(half).collect();
    assert_eq!(h, phone::phone_words_hash(&h_c, &entries));
    // The spec's TH over h_C ‖ u8(len) ‖ (u8 j ‖ u16be y)*.
    let mut message = vec![32u8];
    for (j, y) in &entries {
        message.push(*j);
        message.extend(y.to_be_bytes());
    }
    assert_eq!(h, th(phone::PHONE_WORDS_TAG, &[&h_c, &message]));

    let d = [0, 5, 31, 2, 9];
    let h37 = phone::h37(&h_c, &half, &d, &[1, 2, 3, 4, 5]);
    assert_ne!(h, h37);
    // D in another order is another hash.
    assert_ne!(
        h37,
        phone::h37(&h_c, &half, &[5, 0, 31, 2, 9], &[2, 1, 3, 4, 5])
    );
    // Another word of the half is another hash.
    let mut other = half;
    other[31] ^= 1;
    assert_ne!(h, phone::h32(&h_c, &other));
}

#[test]
#[should_panic(expected = "fewer than 32 words")]
fn a_hash_over_31_words_panics() {
    let words: Vec<(u8, u16)> = (32..63).map(|j| (j, 1)).collect();
    phone::phone_words_hash(&[0; 32], &words);
}
