//! The seal and the start of an opening, cut into steps a device can run between UI polls.
//!
//! Each step is sized for at most about 100 ms on the ESP32-C3: two C's, sixteen of the link's
//! points, one Q, or one pass over the 311 blindings. [`Setup::seal`] and [`OpenJob::new`] run
//! every step in turn, so the stepped and the monolithic paths give the same bytes for the same
//! RNG. The carry and commit proofs are already stepped by
//! [`CardBitProver`](super::proof::CardBitProver), and the witness check by
//! [`OpenJob::check_next`].
//!
//! When the device may step them is frostsnap_core's rule, not this module's (PK.7): the seal
//! before word 1; no proof or opening between `WriteStart` and `DeviceHalfWritten`, so that
//! their timing can't show how the user wrote the device half; the commit proofs only after
//! `DeviceHalfWritten`; and openings only in the phone round.

use super::{
    codec::wipe,
    layout::{BoxSets, CARRIES_PER_BOX, CARRY_BITS, COMMITTED, FREE, HALF, WORD_BITS},
    proof::{
        carry_sum, coin_commitment, derive_carries, link_steps, open_domain, recheck_rho,
        seal_hash, start_carries, CardError, CardSecrets, Carries, OpenJob, Ops, Seal, Setup,
        POINT_LEN,
    },
};
use alloc::{boxed::Box, vec::Vec};
use rand_core::{CryptoRng, RngCore};
use schnorr_fun::fun::prelude::*;

/// C's computed per step, the first time and when the free ones are computed again.
const COMMITS_PER_STEP: usize = 2;
/// The link's points per step: a decompression and two additions each. The bench didn't
/// measure decompression; at the 6 ms its numbers allow, 16 would be about 102 ms, so time a
/// link step on the device and drop this to 12 if it's over about 90 ms.
const LINK_PER_STEP: usize = 16;
/// The scalar bits the link sums over.
const LINK_BITS: usize = 256;
/// Every step of a seal: the C's, the free C's again, the link, then coin_commit and h_C.
pub const SEAL_STEPS: u32 = (COMMITTED.div_ceil(COMMITS_PER_STEP)
    + FREE.div_ceil(COMMITS_PER_STEP)
    + LINK_BITS.div_ceil(LINK_PER_STEP)
    + 1) as u32;
const _: () = assert!(SEAL_STEPS == 201);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SealPhase {
    /// C_v for v = 0..311.
    Commit,
    /// The 55 free-bit C's again, compared: the link doesn't cover them.
    Recheck,
    /// Σ_t 2^t·C_t, by Horner from bit 255 down, then compared with S_i.
    Link,
    /// coin_commit and h_C (over hP, which [`CardSecrets`] computed twice), each computed twice
    /// and compared.
    Finish,
    /// The seal has been returned.
    Done,
}

/// [`Setup::seal`] in steps. It holds only public values: the C's and the link's partial sum.
#[derive(Clone, Debug)]
pub struct SealJob {
    commitments: Vec<[u8; POINT_LEN]>,
    phase: SealPhase,
    /// The next v, or the scalar bits the link has summed.
    cursor: usize,
    link: Point<NonNormal, Public, Zero>,
    steps: u32,
}

impl Default for SealJob {
    fn default() -> Self {
        Self::new()
    }
}

impl SealJob {
    pub fn new() -> Self {
        Self {
            commitments: Vec::with_capacity(COMMITTED),
            phase: SealPhase::Commit,
            cursor: 0,
            link: Point::zero(),
            steps: 0,
        }
    }

    /// One step. It gives the seal from the last step, after every self-check of
    /// [`Setup::seal`] passed. After an error, or once the seal is out, the job is spent and
    /// every further step is [`CardError::SelfCheck`].
    pub fn step(
        &mut self,
        setup: &Setup,
        secrets: &CardSecrets,
        ops: &mut Ops,
    ) -> Result<Option<Seal>, CardError> {
        let result = self.advance(setup, secrets, ops);
        match result {
            Ok(_) => self.steps += 1,
            Err(_) => self.phase = SealPhase::Done,
        }
        result
    }

    fn advance(
        &mut self,
        setup: &Setup,
        secrets: &CardSecrets,
        ops: &mut Ops,
    ) -> Result<Option<Seal>, CardError> {
        match self.phase {
            SealPhase::Commit => {
                let end = (self.cursor + COMMITS_PER_STEP).min(COMMITTED);
                for v in self.cursor..end {
                    self.commitments
                        .push(setup.commit(secrets, v, ops)?.to_bytes());
                }
                self.cursor = end;
                if end == COMMITTED {
                    self.phase = SealPhase::Recheck;
                    self.cursor = 0;
                }
            }
            SealPhase::Recheck => {
                let end = (self.cursor + COMMITS_PER_STEP).min(FREE);
                for v in self.cursor..end {
                    if setup.commit(secrets, v, ops)?.to_bytes() != self.commitments[v] {
                        return Err(CardError::SelfCheck);
                    }
                }
                self.cursor = end;
                if end == FREE {
                    self.phase = SealPhase::Link;
                    self.cursor = 0;
                }
            }
            SealPhase::Link => {
                let end = (self.cursor + LINK_PER_STEP).min(LINK_BITS);
                // Scalar bits 255 − cursor down to 256 − end.
                let bits = (LINK_BITS - end..LINK_BITS - self.cursor).rev();
                self.link = link_steps(self.link, &self.commitments, bits, ops)
                    .ok_or(CardError::SelfCheck)?;
                self.cursor = end;
                if end == LINK_BITS {
                    if self.link != secrets.image {
                        return Err(CardError::SelfCheck);
                    }
                    self.phase = SealPhase::Finish;
                }
            }
            SealPhase::Finish => {
                let phone_hash = secrets.phone_hash();
                let coin_commit = coin_commitment(&secrets.coin, ops);
                let h_c = seal_hash(
                    &secrets.ctx,
                    &coin_commit,
                    phone_hash,
                    &self.commitments,
                    ops,
                );
                if coin_commitment(&secrets.coin, ops) != coin_commit
                    || seal_hash(
                        &secrets.ctx,
                        &coin_commit,
                        phone_hash,
                        &self.commitments,
                        ops,
                    ) != h_c
                {
                    return Err(CardError::SelfCheck);
                }
                self.phase = SealPhase::Done;
                return Ok(Some(Seal {
                    commitments: core::mem::take(&mut self.commitments),
                    coin_commit,
                    h_c,
                }));
            }
            SealPhase::Done => return Err(CardError::SelfCheck),
        }
        Ok(None)
    }

    /// Steps done and steps in all.
    pub fn progress(&self) -> (u32, u32) {
        (self.steps, SEAL_STEPS)
    }
}

/// The start of an opening of device word `j + 1` (spec L1.5.3.2, PK.5), in steps:
/// [`new`](Self::new) draws the fresh bytes and computes y and ρ_β, the first
/// [`step`](Self::step) computes ρ_β again, the second the σ's, and each later step one Q.
/// [`finish`](Self::finish) hashes open_dom (over h_C and hP) and gives the [`OpenJob`]. It
/// takes h_C rather than the seal, so the device can drop the seal once the check has started.
/// The secret side is wiped when it's dropped.
pub struct OpenStart<'a> {
    setup: &'a Setup,
    /// Boxed, with the sums, so neither sits in the device's job frames (about 1.3 KB).
    sets: Box<BoxSets>,
    h_c: [u8; 32],
    /// hP, from the secrets the job started with.
    phone_hash: [u8; 32],
    n: [u8; 32],
    j: usize,
    run: u8,
    carries: Carries,
    rechecked: bool,
    derived: bool,
    points: Vec<[u8; POINT_LEN]>,
    /// The current β's Q's.
    qs: Vec<Point>,
    /// Σ_l 2^{l+1}·Q_{β,l}, for the witness self-check.
    sums: Box<[Point<NonNormal, Public, Zero>; WORD_BITS]>,
}

impl<'a> OpenStart<'a> {
    /// Draws the 32 fresh bytes that hedge the σ's (the RNG's only use until the proofs), then
    /// y, computed twice and compared, and ρ_β. Only device words (j < 32) have openings, and
    /// every one of them is computed, so every one is opened the same way: anything else is
    /// [`CardError::NoSuchBox`], before the RNG is touched.
    #[allow(clippy::too_many_arguments)]
    pub fn new<R: RngCore + CryptoRng>(
        setup: &'a Setup,
        secrets: &CardSecrets,
        h_c: &[u8; 32],
        n: &[u8; 32],
        j: usize,
        run: u8,
        rng: &mut R,
        ops: &mut Ops,
    ) -> Result<Self, CardError> {
        if j >= HALF {
            return Err(CardError::NoSuchBox);
        }
        let sets = Box::new(BoxSets::new(j));
        let mut fresh = [0u8; 32];
        rng.fill_bytes(&mut fresh);
        let carries = start_carries(secrets, &sets, fresh, ops);
        wipe(&mut fresh, [0; 32]);
        let carries = carries?;
        Ok(Self {
            setup,
            sets,
            h_c: *h_c,
            phone_hash: *secrets.phone_hash(),
            n: *n,
            j,
            run,
            carries,
            rechecked: false,
            derived: false,
            points: Vec::with_capacity(CARRIES_PER_BOX),
            qs: Vec::with_capacity(CARRY_BITS),
            sums: Box::new([Point::zero(); WORD_BITS]),
        })
    }

    /// One step: first ρ_β again, then the σ's, then one Q per step. True once all 77 Q's are
    /// in, after 79 steps. `secrets` must be the ones the job was started with, or the witness
    /// check fails.
    pub fn step(&mut self, secrets: &CardSecrets, ops: &mut Ops) -> Result<bool, CardError> {
        if !self.rechecked {
            recheck_rho(&self.carries, secrets, &self.sets, ops)?;
            self.rechecked = true;
            return Ok(false);
        }
        if !self.derived {
            derive_carries(
                &mut self.carries,
                secrets,
                &self.sets,
                &self.n,
                self.j,
                self.run,
                ops,
            )?;
            self.derived = true;
            return Ok(false);
        }
        let k = self.points.len();
        if k < CARRIES_PER_BOX {
            let sigma = self.carries.sigma(k).ok_or(CardError::SelfCheck)?;
            let q = self.setup.pedersen(self.carries.bit(k), sigma, ops)?;
            self.points.push(q.to_bytes());
            self.qs.push(q);
            if self.qs.len() == CARRY_BITS {
                self.sums[k / CARRY_BITS] = carry_sum(&self.qs, ops);
                self.qs.clear();
            }
        }
        Ok(self.points.len() == CARRIES_PER_BOX)
    }

    /// open_dom over hP and the 77 Q's, and the job that checks the witness and proves the
    /// carries.
    pub fn finish(self, ops: &mut Ops) -> Result<OpenJob<'a>, CardError> {
        if self.points.len() != CARRIES_PER_BOX {
            return Err(CardError::SelfCheck);
        }
        let dom = open_domain(
            &self.h_c,
            &self.phone_hash,
            &self.n,
            self.j as u8,
            self.carries.word(),
            &self.points,
            ops,
        );
        Ok(OpenJob::from_parts(
            self.setup,
            self.j,
            self.carries,
            self.points,
            self.sums,
            dom,
        ))
    }
}
