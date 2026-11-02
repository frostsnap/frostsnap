//! A card session: the secrets, the card's words and the jobs of one share's card flow, held
//! in RAM by the firmware (as it holds the nonce job batch) and run a slice per poll.
//!
//! The session's first job encodes the card ([`Job::Encode`]): it holds the decrypted share
//! until the card's secrets are derived, then wipes it. Its words are used only once core has
//! checked them against the record's words hash ([`CardSession::check_words`], spec PK.6 (2)).
//! The seal is computed only when it's needed (before `SealedC` exists, when an unpinned phone
//! pulls it in SEALED, or for the proofs), and the bit proofs only in PROVING (spec PK.7): core
//! arms them, never this module.
//!
//! Deviation (design §9 R23): the [`CardSecrets`] live for the whole session rather than per
//! slice. They and the words are wiped when the session is dropped, which happens on exit,
//! `Cancel` and reset.

use super::card::{wipe, CardQuizItems, CardState, CardWords3, SessionFacts};
use super::*;
use crate::message::card::CardSealed;
use frost_backup::{
    bip39_words::lookalike_pool,
    verifiable_card::{
        codec::{self, Card, EncodeJob, COUNTERS_PER_STEP, FIXED_COUNTERS},
        jobs::{OpenStart, SealJob},
        layout::{CARRIES_PER_BOX, COMMITTED, HALF, WORDS, WORD_BITS},
        proof::{
            CardBitProver, CardError, CardSecrets, CheckedOpenJob, OpenJob, Opening, Ops, Seal,
            Setup, Unverified, CHECK_BOXES, PROOF_LEN,
        },
        rng::{self, child, share_image_bytes, tag, Phase, RngCtx, SHARE_IMAGE_LEN},
    },
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use rand_core::{CryptoRng, RngCore};

/// What the firmware needs to open a session. Core builds it when a `Begin` is accepted. It
/// holds the ciphertext, not the share, and its `Debug` shows only the share image.
#[derive(Clone)]
pub struct CardSessionStart {
    pub access_structure_ref: AccessStructureRef,
    pub share_index: ShareIndex,
    pub share_image: ShareImage,
    pub encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
    pub coord_contrib: CoordShareDecryptionContrib,
    pub root_shared_key: SharedKey,
    pub sealed: Option<CardSealed>,
    /// The record's state when the session was asked for.
    pub state: CardState,
}

impl core::fmt::Debug for CardSessionStart {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CardSessionStart")
            .field("share_image", &self.share_image)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardSessionError {
    /// The share didn't decrypt.
    Decrypt,
    /// The card couldn't be encoded, or a self-check failed.
    Card(CardError),
}

impl From<CardError> for CardSessionError {
    fn from(error: CardError) -> Self {
        CardSessionError::Card(error)
    }
}

impl CardSessionStart {
    /// Decrypts the share as `decrypt_to_backup` does, and opens the session's child CSPRNG
    /// from `rng`, the parent of every draw the session makes (design S2). Nothing else is
    /// computed here: the encode is the session's first job, so no slice takes long.
    pub fn open<'s>(
        self,
        hmac: &mut impl DeviceSecretDerivation,
        setup: &'s Setup,
        rng: &mut (impl RngCore + CryptoRng),
    ) -> Result<CardSession<'s>, CardSessionError> {
        let encryption_key = hmac.get_share_encryption_key(
            self.access_structure_ref,
            self.share_index,
            self.coord_contrib,
        );
        let share = WipedShareSecret(SecretShare {
            index: self.share_index,
            share: self
                .encrypted_secret_share
                .decrypt(encryption_key)
                .ok_or(CardSessionError::Decrypt)?,
        });
        let (image_bytes, rng) = session_rng(&self.share_image, rng);
        let encode = EncodeStage {
            job: EncodeJob::new(&share.0, &self.root_shared_key)
                .map_err(|e| CardSessionError::Card(e.into()))?,
            share,
            root_shared_key: self.root_shared_key,
            key_id: self.access_structure_ref.key_id.0,
            access_structure_id: self.access_structure_ref.access_structure_id.0,
            first: None,
            slices: 0,
        };
        Ok(CardSession {
            setup,
            share_image: self.share_image,
            built: None,
            encode: Some(Box::new(encode)),
            h_c: self.sealed.map(|sealed| sealed.h_c),
            seal: None,
            // About 30 KB once filled: allocated when the proofs are first wanted, in PROVING.
            proofs: ProofBuffer::default(),
            words_ok: false,
            write_started: false,
            seal_ready: false,
            seal_wanted: false,
            seal_for_proofs: false,
            proofs_wanted: false,
            n: None,
            ui_at: CardUiStep::Other,
            ready: Vec::new(),
            asked: None,
            job: Job::Idle,
            image_bytes,
            rng,
            display_seq: 0,
            dr: None,
            round_pick: None,
            recheck: None,
            final_begin_deferred: false,
        })
    }
}

/// The decrypted share, held by the encode job only until the card's secrets are derived. It's
/// wiped when dropped.
struct WipedShareSecret(SecretShare);

impl Drop for WipedShareSecret {
    fn drop(&mut self) {
        wipe(&mut self.0.share, Scalar::zero());
    }
}

/// The session's child CSPRNG, `child(SESSION, fw rng, ctx{image})`. Its own frame:
/// `CardSessionStart::open` is on a deep chain (addendum row 23).
#[inline(never)]
fn session_rng(
    share_image: &ShareImage,
    rng: &mut (impl RngCore + CryptoRng),
) -> ([u8; SHARE_IMAGE_LEN], SessionRng) {
    let image_bytes = share_image_bytes(share_image);
    // The SESSION tag sets this stream apart; the phase is only a placeholder.
    let ctx = RngCtx {
        share_image: image_bytes,
        phase: Phase::DeviceRound,
        pass: 0,
        position: 0,
        display_id: 0,
        run: 0,
    };
    (image_bytes, SessionRng(child(tag::SESSION, rng, &ctx)))
}

/// The parent CSPRNG of a session's draws, or one of its child streams. It's overwritten when
/// dropped: ChaCha20Rng doesn't wipe itself.
pub(super) struct SessionRng(pub(super) ChaCha20Rng);

impl Drop for SessionRng {
    fn drop(&mut self) {
        self.0 = ChaCha20Rng::from_seed([0; 32]);
        core::hint::black_box(&mut self.0);
    }
}

/// The encode job (design §2.3): the card is encoded twice and the two compared, then its
/// secrets derived and checked against the share image.
struct EncodeStage {
    share: WipedShareSecret,
    root_shared_key: SharedKey,
    key_id: [u8; 32],
    access_structure_id: [u8; 32],
    job: EncodeJob,
    /// The first encode's card, while the second runs.
    first: Option<Card>,
    slices: u32,
}

/// The card and its secrets, once the encode job is done.
struct Built {
    secrets: CardSecrets,
    card: Card,
}

/// The device round's live answer screen (design §5.4): the word, E's index k and the three
/// options in the order shown. A pick is scored against this, never against a redraw. Its
/// `Debug` shows only the display id: the word is an element of E.
pub struct DrScreen {
    pub display_id: u32,
    pub j: u8,
    pub k: u8,
    pub options: CardWords3,
}

impl core::fmt::Debug for DrScreen {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DrScreen")
            .field("display_id", &self.display_id)
            .finish_non_exhaustive()
    }
}

/// `word`, two of its six closest lookalikes picked with `rng`, in an order shuffled with
/// `rng` (design O11).
/// The word and two of its pool: `picked`'s places if given, else two drawn. The order is
/// always fresh. Also returns the two places.
fn shuffled_options(
    rng: &mut ChaCha20Rng,
    word: u16,
    picked: Option<[u8; 2]>,
) -> Option<(CardWords3, [u8; 2])> {
    let pool = lookalike_pool(word)?;
    let [a, b] = picked.unwrap_or_else(|| rng::distinct::<2>(rng, pool.len() as u8));
    let mut options = CardWords3([word, *pool.get(a as usize)?, *pool.get(b as usize)?]);
    rng::shuffle(rng, &mut options.0);
    Some((options, [a, b]))
}

/// The commit proofs, in the order they're computed: from `start` to the last, then from 0, so
/// a phone that resumes gets the range it's missing first.
#[derive(Clone, Debug, PartialEq)]
pub struct ProofBuffer {
    start: u16,
    proofs: Vec<[u8; PROOF_LEN]>,
}

/// An empty buffer that holds nothing: what's left after the proofs are dropped.
impl Default for ProofBuffer {
    fn default() -> Self {
        Self {
            start: 0,
            proofs: Vec::new(),
        }
    }
}

impl ProofBuffer {
    /// Room for all 311 proofs, keeping where they start.
    fn reserve(&mut self) {
        self.proofs.reserve_exact(COMMITTED - self.proofs.len());
    }

    /// Where proof v is in the buffer, if it's computed.
    fn position(&self, v: u16) -> Option<usize> {
        let v = v as usize;
        if v >= COMMITTED {
            return None;
        }
        let position = (v + COMMITTED - self.start as usize) % COMMITTED;
        (position < self.proofs.len()).then_some(position)
    }

    pub fn has(&self, v: u16) -> bool {
        self.position(v).is_some()
    }

    pub fn get(&self, v: u16) -> Option<&[u8; PROOF_LEN]> {
        self.proofs.get(self.position(v)?)
    }

    pub fn len(&self) -> usize {
        self.proofs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }

    fn is_complete(&self) -> bool {
        self.proofs.len() == COMMITTED
    }

    /// The v the next proof is for.
    fn next(&self) -> Option<u16> {
        (!self.is_complete())
            .then(|| ((self.start as usize + self.proofs.len()) % COMMITTED) as u16)
    }
}

/// Which screen family is up, so background events and user actions act only on a screen of
/// their own kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardUiStep {
    Other,
    /// The gate screen with its dots.
    Gate,
    /// "Preparing" until this session's encode (or the first seal) is done; its event then
    /// shows the step.
    Preparing,
    /// Words 1–32 to write.
    DeviceHalf,
    /// The quiz on words 1–32.
    SelfCheck,
    /// "Now use your phone".
    PhoneHalf,
    /// A later check's quiz.
    Recheck,
    /// A device-round screen. Owner B12: the device round runs after N, so it has no Stop.
    DeviceRound {
        screen: RoundScreen,
    },
}

/// The device round's screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundScreen {
    Intro,
    Answer,
    Mismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusyTask {
    Seal,
    Proofs,
    Opening,
    /// The session's encode job.
    Card,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobProgress {
    pub task: BusyTask,
    pub done: u32,
    pub total: u32,
}

/// What a job's last slice finished.
pub enum CardJobEvent {
    /// The card is encoded and its secrets derived: the session can show and prove it.
    Encoded,
    /// The seal, computed and self-checked.
    Sealed(Seal),
    ProofsDone,
    /// The opening of word j is in the session, waiting to be released.
    OpeningReady {
        j: u8,
    },
    /// A self-check failed and the job stopped.
    Failed(CardError),
}

/// A job armed in this poll takes its first slice in the next, after the flush, so what armed
/// it (an `OpenRun`) is on flash before it computes anything.
enum ArmedJob {
    Seal,
    Proofs,
    Open { j: u8, run: u8 },
}

/// One commit proof or carry proof, in three slices: k·H, the response, and the stock
/// verification before release. The job states are boxed so a slice moves pointers, not
/// kilobytes, on the stack.
enum ProofStep<'s> {
    Start,
    Respond(Box<CardBitProver<'s>>),
    Release(Box<Unverified<'s>>),
}

/// An opening: new, then σ and the 77 Q's, then 11 witness checks, then 77 carry proofs of three
/// slices each.
enum OpenStage<'s> {
    New,
    Start(Box<OpenStart<'s>>),
    Check(Box<OpenJob<'s>>),
    Carry {
        job: Box<CheckedOpenJob<'s>>,
        step: ProofStep<'s>,
    },
}

enum Job<'s> {
    Idle,
    Armed(ArmedJob),
    Encode(Box<EncodeStage>),
    Seal(SealJob),
    Proof {
        v: u16,
        step: ProofStep<'s>,
    },
    Open {
        j: u8,
        run: u8,
        slices: u32,
        stage: OpenStage<'s>,
    },
    Failed,
}

/// Slices in an opening: new, ρ again, σ, 77 Q's, 11 β's and 77 × 3 for the carry proofs.
const OPEN_SLICES: u32 = (1 + 2 + CARRIES_PER_BOX + WORD_BITS + 3 * CARRIES_PER_BOX) as u32;
/// Slices in the encode job: two encodes of the fixed counters, then the secrets.
const ENCODE_SLICES: u32 = 2 * (FIXED_COUNTERS as u32).div_ceil(COUNTERS_PER_STEP as u32) + 1;
/// Finished openings a session keeps once a job event or request is handled: at most one
/// computed, unsent opening (spec L1.5.3.2), about 9.9 KB.
const MAX_READY: usize = 1;
/// Slices in the commit proofs.
const PROOF_SLICES: u32 = 3 * COMMITTED as u32;

pub struct CardSession<'s> {
    setup: &'s Setup,
    share_image: ShareImage,
    /// The card and its secrets, once the encode job is done. The writing and quiz screens read
    /// the words by index (spec L1.0.3 rule 5).
    built: Option<Built>,
    /// Core checked the built card's words against the record's words hash (or sealed them):
    /// only then does [`words`](Self::words) give them.
    words_ok: bool,
    /// The encode job, until `card_session_opened` arms it.
    encode: Option<Box<EncodeStage>>,
    /// h_C, once sealed: an opening takes it from here, not from the seal.
    h_c: Option<[u8; 32]>,
    /// In RAM only when computed in this session; dropped at `FinalBegin`.
    pub(super) seal: Option<Seal>,
    pub(super) proofs: ProofBuffer,
    pub(super) write_started: bool,
    pub(super) seal_ready: bool,
    /// A pull came before the encode was done: the seal follows it.
    pub(super) seal_wanted: bool,
    /// The proofs needed the seal computed again in this session: the proving bar covers it.
    seal_for_proofs: bool,
    /// The proofs were wanted before the seal was in RAM: they follow it.
    pub(super) proofs_wanted: bool,
    pub(super) n: Option<[u8; 32]>,
    pub(super) ui_at: CardUiStep,
    /// Finished openings that haven't been released, at most [`MAX_READY`].
    pub(super) ready: Vec<Opening>,
    /// The word the phone asked for and hasn't got yet.
    pub(super) asked: Option<u8>,
    job: Job<'s>,
    /// [`share_image_bytes`] of the share: every draw's context.
    image_bytes: [u8; SHARE_IMAGE_LEN],
    /// The parent of every draw (design S2).
    rng: SessionRng,
    /// The opaque display id counter: never a word number. The first display is 1.
    display_seq: u32,
    pub(super) dr: Option<DrScreen>,
    /// E[k]'s two lookalikes as first shown, by k and their places in its pool: a second look
    /// shows the same two (design O11), so a lie shown once is shown again.
    round_pick: Option<(u8, [u8; 2])>,
    /// A later check's hash from the phone, until the encode is done and it's answered.
    pub(super) recheck: Option<Option<[u8; 32]>>,
    /// A `FinalBegin` came before the encode was done: it's handled once it is.
    pub(super) final_begin_deferred: bool,
}

impl core::fmt::Debug for CardSession<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CardSession")
            .field("share_image", &self.share_image)
            .finish_non_exhaustive()
    }
}

impl<'s> CardSession<'s> {
    pub fn share_image(&self) -> ShareImage {
        self.share_image
    }

    /// What the gate needs from this session.
    pub(super) fn facts(&self) -> SessionFacts<'_> {
        SessionFacts {
            write_started: self.write_started,
            seal_ready: self.seal_ready,
            proofs: &self.proofs,
            at_gate: self.ui_at == CardUiStep::Gate,
            words: self.words(),
        }
    }

    /// The card's 64 words, once encoded and checked against the record's words hash. Only
    /// core reads them, and only a half ever goes into a screen or a frame.
    pub(super) fn words(&self) -> Option<&[u16; WORDS]> {
        self.built
            .as_ref()
            .filter(|_| self.words_ok)
            .map(|built| &built.card.words)
    }

    /// TH(c1/words, L ‖ the 64 words) of the built card, for its `SealedC`.
    pub(super) fn words_hash(&self) -> Option<[u8; 32]> {
        self.built
            .as_ref()
            .map(|built| codec::card_words_hash(&built.card.words))
    }

    /// Binding (2), spec PK.6: whether the built card's words are the ones the record sealed
    /// (`None` for a card not sealed yet, whose seal is made from these). Its words are given
    /// out only after this passed.
    pub(super) fn check_words(&mut self, sealed: Option<[u8; 32]>) -> bool {
        self.words_ok = match (sealed, self.words_hash()) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(sealed), Some(hash)) => sealed == hash,
        };
        self.words_ok
    }

    pub(super) fn secrets(&self) -> Option<&CardSecrets> {
        self.built.as_ref().map(|built| &built.secrets)
    }

    /// The next opaque display id.
    pub(super) fn next_display(&mut self) -> u32 {
        self.display_seq += 1;
        self.display_seq
    }

    fn ctx(&self, phase: Phase, pass: u8, position: u8, display_id: u32) -> RngCtx {
        RngCtx {
            share_image: self.image_bytes,
            phase,
            pass,
            position,
            display_id,
            run: 0,
        }
    }

    /// E: the device round's 5 distinct words of 33–64 (spec L2.7.6 B2), drawn once at
    /// `FinalBegin` and persisted before any of them is shown. The phone never learns it.
    pub(super) fn draw_round_boxes(&mut self) -> [u8; CHECK_BOXES] {
        let ctx = self.ctx(Phase::DeviceRound, 0, 0, 0);
        let mut e_rng = SessionRng(child(tag::DR_E, &mut self.rng.0, &ctx));
        rng::distinct::<CHECK_BOXES>(&mut e_rng.0, HALF as u8).map(|j| j + HALF as u8)
    }

    /// The answer screen's options for word j (spec L2.7.7, design O11): its word and two of
    /// its six closest lookalikes, in a fresh order for this display. The two are picked once
    /// per k in a session, so a second look shows the same ones. The stated exception to the
    /// no-secret-index rule: these words are on the device screen anyway.
    pub(super) fn round_options(
        &mut self,
        j: u8,
        k: u8,
        attempt: u8,
        display_id: u32,
    ) -> Option<CardWords3> {
        let mut word = *self.words()?.get(j as usize)?;
        let ctx = self.ctx(Phase::DeviceRound, attempt, k, display_id);
        let mut options_rng = SessionRng(child(tag::DR_OPTIONS, &mut self.rng.0, &ctx));
        let picked = self
            .round_pick
            .filter(|(picked_k, _)| *picked_k == k)
            .map(|(_, picked)| picked);
        let options = shuffled_options(&mut options_rng.0, word, picked);
        wipe(&mut word, 0);
        let (options, picked) = options?;
        self.round_pick = Some((k, picked));
        Some(options)
    }

    /// Whether `pick` is word j's. A pick of "None of these" (`None`) or past the options never
    /// is.
    pub(super) fn round_pick_matches(&self, j: u8, options: &CardWords3, pick: Option<u8>) -> bool {
        let Some(mut word) = self
            .words()
            .and_then(|words| words.get(j as usize).copied())
        else {
            return false;
        };
        let picked = pick.and_then(|i| options.0.get(i as usize).copied());
        let ok = picked == Some(word);
        wipe(&mut word, 0);
        ok
    }

    /// A quiz's items for these words: (word number, word).
    pub(super) fn quiz_items(&self, positions: impl Iterator<Item = u8>) -> CardQuizItems {
        let words = self.words();
        CardQuizItems(
            positions
                .filter_map(|j| Some((j + 1, *words?.get(j as usize)?)))
                .collect(),
        )
    }

    /// A quiz's option shuffle seed (the quiz widget takes a `u32`), fresh for each quiz shown.
    pub(super) fn self_check_seed(&mut self) -> u32 {
        let display_id = self.next_display();
        let ctx = self.ctx(Phase::SelfCheck, 0, 0, display_id);
        SessionRng(child(tag::SELF_CHECK, &mut self.rng.0, &ctx))
            .0
            .next_u32()
    }

    /// No answer screen is live: a step screen or a verdict is up.
    pub(super) fn clear_displays(&mut self) {
        self.dr = None;
    }

    /// The encode, the session's first job.
    pub(super) fn arm_encode(&mut self) {
        if let Some(encode) = self.encode.take() {
            self.job = Job::Encode(encode);
        }
    }

    pub(super) fn arm_seal(&mut self) {
        self.seal_wanted = false;
        self.job = Job::Armed(ArmedJob::Seal);
    }

    /// A pull needs the seal: it's computed now, or after the encode.
    pub(super) fn want_seal(&mut self) {
        if self.seal_ready || matches!(self.job, Job::Seal(_) | Job::Armed(ArmedJob::Seal)) {
            return;
        }
        if self.built.is_some() && matches!(self.job, Job::Idle) {
            self.arm_seal();
        } else {
            self.seal_wanted = true;
        }
    }

    /// No seal job runs while writing: one a pull armed in SEALED is stopped, and a wanted one
    /// forgotten. A seal already in RAM is kept.
    pub(super) fn stop_seal_job(&mut self) {
        self.seal_wanted = false;
        if matches!(self.job, Job::Seal(_) | Job::Armed(ArmedJob::Seal)) {
            self.job = Job::Idle;
        }
    }

    /// The commit proofs, now if the seal is in RAM and nothing else runs, else once the seal
    /// is done. Core calls it only in PROVING (`card_want_proofs`).
    pub(super) fn want_proofs(&mut self) {
        if matches!(self.job, Job::Proof { .. } | Job::Armed(ArmedJob::Proofs)) {
            return;
        }
        self.proofs.reserve();
        if self.seal_ready && matches!(self.job, Job::Idle) {
            self.proofs_wanted = false;
            if !self.proofs.is_complete() {
                self.job = Job::Armed(ArmedJob::Proofs);
            }
        } else {
            self.proofs_wanted = true;
            self.seal_for_proofs |= !self.seal_ready;
            self.want_seal();
        }
    }

    /// All 311 proofs are computed in this session.
    pub(super) fn proofs_complete(&self) -> bool {
        self.proofs.is_complete()
    }

    pub(super) fn arm_open(&mut self, j: u8, run: u8) {
        self.job = Job::Armed(ArmedJob::Open { j, run });
    }

    /// This session's seal matched the record.
    pub(super) fn seal_ready(&mut self, sealed: CardSealed, seal: Seal) {
        self.h_c = Some(sealed.h_c);
        self.seal = Some(seal);
        self.seal_ready = true;
    }

    /// While no proof is computed yet, the proofs start at the phone's first missing one.
    pub(super) fn prefer_proofs_from(&mut self, from: u16) {
        let started = matches!(
            self.job,
            Job::Proof {
                step: ProofStep::Respond(_) | ProofStep::Release(_),
                ..
            }
        );
        if self.proofs.is_empty() && !started && (from as usize) < COMMITTED {
            self.proofs.start = from;
            if let Job::Proof { v, .. } = &mut self.job {
                *v = from;
            }
        }
    }

    /// CHECKING: the seal and the proofs aren't needed again, so they're dropped and their
    /// jobs stopped. A running opening carries on.
    pub(super) fn enter_checking(&mut self, n: [u8; 32]) {
        self.n = Some(n);
        self.drop_seal_and_proofs();
    }

    /// The seal and the proofs are dropped and their jobs stopped; the encode and a running
    /// opening carry on.
    pub(super) fn drop_seal_and_proofs(&mut self) {
        self.seal = None;
        self.seal_ready = false;
        self.seal_wanted = false;
        self.proofs_wanted = false;
        self.proofs = ProofBuffer::default();
        if matches!(
            self.job,
            Job::Seal(_)
                | Job::Proof { .. }
                | Job::Armed(ArmedJob::Seal)
                | Job::Armed(ArmedJob::Proofs)
        ) {
            self.job = Job::Idle;
        }
    }

    /// The verdict is in: nothing more is computed, released or displayed.
    pub(super) fn finish_check(&mut self) {
        self.job = Job::Idle;
        self.ready.clear();
        self.asked = None;
        self.clear_displays();
    }

    pub(super) fn fail(&mut self) {
        self.job = Job::Failed;
        self.encode = None;
        self.ready.clear();
    }

    /// The word whose opening is armed or computing.
    pub(super) fn opening_box(&self) -> Option<u8> {
        match self.job {
            Job::Armed(ArmedJob::Open { j, .. }) | Job::Open { j, .. } => Some(j),
            _ => None,
        }
    }

    /// The finished opening of word j, waiting to be released.
    pub(super) fn ready_opening(&self, j: u8) -> Option<&Opening> {
        self.ready.iter().find(|opening| opening.j == j)
    }

    /// Drops the opening of word j once it's released.
    pub(super) fn take_ready(&mut self, j: u8) {
        self.ready.retain(|opening| opening.j != j);
    }

    /// Keeps at most [`MAX_READY`] unreleased openings, dropping the oldest that the phone
    /// didn't ask for (its run is spent either way).
    pub(super) fn trim_ready(&mut self) {
        while self.ready.len() > MAX_READY {
            let drop = self
                .ready
                .iter()
                .position(|opening| Some(opening.j) != self.asked)
                .unwrap_or(0);
            self.ready.remove(drop);
        }
    }

    /// The proving screen's bar (owner B2): the seal, when the proofs needed it computed again in
    /// this session (about 15 s), then the 311 bit proofs (about 63 s), as one bar driven by the
    /// jobs themselves. None unless the proofs are wanted, running or done.
    pub fn proving_progress(&self) -> Option<f32> {
        const SEAL: f32 = 15.0;
        const PROOFS: f32 = 63.0;
        let proving = self.proofs_wanted
            || matches!(self.job, Job::Armed(ArmedJob::Proofs) | Job::Proof { .. })
            || !self.proofs.is_empty();
        if !proving {
            return None;
        }
        let job = self.progress();
        let fraction = |task| {
            job.filter(|job| job.task == task)
                .map_or(0.0, |job| job.done as f32 / job.total.max(1) as f32)
        };
        let seal = if self.seal_for_proofs { SEAL } else { 0.0 };
        let seal_done = if self.seal_ready {
            1.0
        } else {
            fraction(BusyTask::Seal)
        };
        let proofs_done =
            fraction(BusyTask::Proofs).max(self.proofs.len() as f32 / COMMITTED as f32);
        Some((seal_done * seal + proofs_done * PROOFS) / (seal + PROOFS))
    }

    pub fn progress(&self) -> Option<JobProgress> {
        let (task, done, total) = match &self.job {
            Job::Encode(stage) => (BusyTask::Card, stage.slices, ENCODE_SLICES),
            Job::Armed(ArmedJob::Seal) => (BusyTask::Seal, 0, 1),
            Job::Seal(job) => {
                let (done, total) = job.progress();
                (BusyTask::Seal, done, total)
            }
            Job::Armed(ArmedJob::Proofs) | Job::Proof { .. } => {
                let step = match &self.job {
                    Job::Proof {
                        step: ProofStep::Respond(_),
                        ..
                    } => 1,
                    Job::Proof {
                        step: ProofStep::Release(_),
                        ..
                    } => 2,
                    _ => 0,
                };
                (
                    BusyTask::Proofs,
                    3 * self.proofs.len() as u32 + step,
                    PROOF_SLICES,
                )
            }
            Job::Armed(ArmedJob::Open { .. }) => (BusyTask::Opening, 0, OPEN_SLICES),
            Job::Open { slices, .. } => (BusyTask::Opening, *slices, OPEN_SLICES),
            Job::Idle | Job::Failed => return None,
        };
        Some(JobProgress { task, done, total })
    }

    /// One slice of the current job: at most about 100 ms on the ESP32-C3 (the proof release
    /// is about 104 ms). A failed self-check stops the job for good.
    #[inline(never)]
    pub fn step(&mut self, rng: &mut (impl RngCore + CryptoRng)) -> Option<CardJobEvent> {
        let mut ops = Ops::default();
        match self.advance(rng, &mut ops) {
            Ok(event) => event,
            Err(error) => {
                self.job = Job::Failed;
                Some(CardJobEvent::Failed(error))
            }
        }
    }

    fn advance(
        &mut self,
        rng: &mut (impl RngCore + CryptoRng),
        ops: &mut Ops,
    ) -> Result<Option<CardJobEvent>, CardError> {
        let setup = self.setup;
        let job = core::mem::replace(&mut self.job, Job::Idle);
        let (job, event) = match job {
            Job::Idle => (Job::Idle, None),
            Job::Failed => (Job::Failed, None),
            Job::Encode(mut stage) => match encode_step(&mut stage, ops)? {
                Some(built) => {
                    self.built = Some(built);
                    (Job::Idle, Some(CardJobEvent::Encoded))
                }
                None => (Job::Encode(stage), None),
            },
            Job::Armed(ArmedJob::Seal) => (Job::Seal(SealJob::new()), None),
            Job::Armed(ArmedJob::Proofs) => match self.proofs.next() {
                Some(v) => (
                    Job::Proof {
                        v,
                        step: ProofStep::Start,
                    },
                    None,
                ),
                None => (Job::Idle, Some(CardJobEvent::ProofsDone)),
            },
            Job::Armed(ArmedJob::Open { j, run }) => (
                Job::Open {
                    j,
                    run,
                    slices: 0,
                    stage: OpenStage::New,
                },
                None,
            ),
            Job::Seal(mut job) => {
                let secrets = self.secrets().ok_or(CardError::SelfCheck)?;
                match seal_step(&mut job, setup, secrets, ops)? {
                    Some(seal) => (Job::Idle, Some(CardJobEvent::Sealed(seal))),
                    None => (Job::Seal(job), None),
                }
            }
            Job::Proof { v, step } => {
                let step = match step {
                    ProofStep::Start => {
                        let seal = self.seal.as_ref().ok_or(CardError::SelfCheck)?;
                        let secrets = self.secrets().ok_or(CardError::SelfCheck)?;
                        ProofStep::Respond(commit_prover(
                            setup, secrets, seal, v as usize, rng, ops,
                        )?)
                    }
                    ProofStep::Respond(prover) => ProofStep::Release(respond(prover, ops)?),
                    ProofStep::Release(proof) => {
                        self.proofs.proofs.push(release(proof, ops)?);
                        return Ok(match self.proofs.next() {
                            Some(v) => {
                                self.job = Job::Proof {
                                    v,
                                    step: ProofStep::Start,
                                };
                                None
                            }
                            None => Some(CardJobEvent::ProofsDone),
                        });
                    }
                };
                (Job::Proof { v, step }, None)
            }
            Job::Open {
                j,
                run,
                slices,
                stage,
            } => {
                let stage = match stage {
                    OpenStage::New => {
                        let h_c = self.h_c.ok_or(CardError::SelfCheck)?;
                        let n = self.n.ok_or(CardError::SelfCheck)?;
                        let secrets = self.secrets().ok_or(CardError::SelfCheck)?;
                        OpenStage::Start(open_start(
                            setup, secrets, &h_c, &n, j as usize, run, rng, ops,
                        )?)
                    }
                    OpenStage::Start(mut start) => {
                        let secrets = self.secrets().ok_or(CardError::SelfCheck)?;
                        if open_step(&mut start, secrets, ops)? {
                            OpenStage::Check(open_finish(start, ops)?)
                        } else {
                            OpenStage::Start(start)
                        }
                    }
                    OpenStage::Check(mut job) => {
                        if check_next(&mut job, ops)? {
                            OpenStage::Carry {
                                job: into_checked(job)?,
                                step: ProofStep::Start,
                            }
                        } else {
                            OpenStage::Check(job)
                        }
                    }
                    OpenStage::Carry { mut job, step } => {
                        let step = match step {
                            ProofStep::Start => {
                                ProofStep::Respond(carry_prover(&mut job, rng, ops)?)
                            }
                            ProofStep::Respond(prover) => ProofStep::Release(respond(prover, ops)?),
                            ProofStep::Release(proof) => {
                                add_proof(&mut job, proof, ops)?;
                                if job.next_carry().is_none() {
                                    self.ready.push(finish_opening(job)?);
                                    return Ok(Some(CardJobEvent::OpeningReady { j }));
                                }
                                ProofStep::Start
                            }
                        };
                        OpenStage::Carry { job, step }
                    }
                };
                (
                    Job::Open {
                        j,
                        run,
                        slices: slices + 1,
                        stage,
                    },
                    None,
                )
            }
        };
        self.job = job;
        Ok(event)
    }
}

// Each slice's work sits in its own frame: inlined into `step` together, the proof
// arithmetic of every stage made one frame of about 13.5 KB on the ESP32-C3.

/// One slice of the encode: `COUNTERS_PER_STEP` counters of an encode, or, once both encodes
/// agree, the card's secrets and the image check. The share is wiped when the stage is dropped.
#[inline(never)]
fn encode_step(stage: &mut EncodeStage, ops: &mut Ops) -> Result<Option<Built>, CardError> {
    stage.slices += 1;
    let Some(card) = stage.job.step() else {
        return Ok(None);
    };
    let card = card?;
    let Some(first) = stage.first.take() else {
        stage.first = Some(card);
        stage.job = EncodeJob::new(&stage.share.0, &stage.root_shared_key)?;
        return Ok(None);
    };
    // The masks and the threshold value can't be checked any other way.
    if first != card {
        return Err(CardError::SelfCheck);
    }
    let secrets = CardSecrets::from_card(
        &stage.share.0,
        &stage.root_shared_key,
        &card,
        stage.key_id,
        stage.access_structure_id,
        ops,
    )?;
    // The secrets hold S_i already, so this costs no second s·G.
    if secrets.image() != stage.share.0.share_image().image {
        return Err(CardError::SelfCheck);
    }
    Ok(Some(Built { secrets, card }))
}

#[inline(never)]
fn seal_step(
    job: &mut SealJob,
    setup: &Setup,
    secrets: &CardSecrets,
    ops: &mut Ops,
) -> Result<Option<Seal>, CardError> {
    job.step(setup, secrets, ops)
}

#[inline(never)]
fn commit_prover<'s>(
    setup: &'s Setup,
    secrets: &CardSecrets,
    seal: &Seal,
    v: usize,
    rng: &mut (impl RngCore + CryptoRng),
    ops: &mut Ops,
) -> Result<Box<CardBitProver<'s>>, CardError> {
    Ok(Box::new(setup.commit_prover(secrets, seal, v, rng, ops)?))
}

#[inline(never)]
fn respond<'s>(
    prover: Box<CardBitProver<'s>>,
    ops: &mut Ops,
) -> Result<Box<Unverified<'s>>, CardError> {
    Ok(Box::new(prover.respond(ops)?))
}

#[inline(never)]
fn release(proof: Box<Unverified<'_>>, ops: &mut Ops) -> Result<[u8; PROOF_LEN], CardError> {
    proof.release(ops)
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn open_start<'s>(
    setup: &'s Setup,
    secrets: &CardSecrets,
    h_c: &[u8; 32],
    n: &[u8; 32],
    j: usize,
    run: u8,
    rng: &mut (impl RngCore + CryptoRng),
    ops: &mut Ops,
) -> Result<Box<OpenStart<'s>>, CardError> {
    Ok(Box::new(OpenStart::new(
        setup, secrets, h_c, n, j, run, rng, ops,
    )?))
}

#[inline(never)]
fn open_step(
    start: &mut OpenStart<'_>,
    secrets: &CardSecrets,
    ops: &mut Ops,
) -> Result<bool, CardError> {
    start.step(secrets, ops)
}

#[inline(never)]
fn open_finish<'s>(
    start: Box<OpenStart<'s>>,
    ops: &mut Ops,
) -> Result<Box<OpenJob<'s>>, CardError> {
    Ok(Box::new(start.finish(ops)?))
}

#[inline(never)]
fn check_next(job: &mut OpenJob<'_>, ops: &mut Ops) -> Result<bool, CardError> {
    job.check_next(ops)
}

#[inline(never)]
fn into_checked(job: Box<OpenJob<'_>>) -> Result<Box<CheckedOpenJob<'_>>, CardError> {
    Ok(Box::new(job.into_checked()?))
}

#[inline(never)]
fn carry_prover<'s>(
    job: &mut CheckedOpenJob<'s>,
    rng: &mut (impl RngCore + CryptoRng),
    ops: &mut Ops,
) -> Result<Box<CardBitProver<'s>>, CardError> {
    Ok(Box::new(job.carry_prover(rng, ops)?))
}

#[inline(never)]
fn add_proof<'s>(
    job: &mut CheckedOpenJob<'s>,
    proof: Box<Unverified<'s>>,
    ops: &mut Ops,
) -> Result<(), CardError> {
    job.add_proof(*proof, ops)
}

#[inline(never)]
fn finish_opening(job: Box<CheckedOpenJob<'_>>) -> Result<Opening, CardError> {
    job.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;

    /// The device round's options (design O11): the word and two of its six closest
    /// lookalikes, never the same one twice, with every pair of the pool and every order
    /// turning up across draws.
    #[test]
    fn round_options_pick_two_of_the_pool() {
        const DRAWS: u32 = 30_000;
        let mut parent = ChaCha20Rng::from_seed([9; 32]);
        let word = 1500;
        let pool = lookalike_pool(word).unwrap();
        let mut pairs = BTreeSet::new();
        let mut first = [0u32; 3];
        for display_id in 1..=DRAWS {
            let ctx = RngCtx {
                share_image: [1; SHARE_IMAGE_LEN],
                phase: Phase::DeviceRound,
                pass: 0,
                position: 2,
                display_id,
                run: 0,
            };
            let mut options_rng = child(tag::DR_OPTIONS, &mut parent, &ctx);
            let (options, _) = shuffled_options(&mut options_rng, word, None).unwrap();
            assert_eq!(options.0.iter().filter(|&&o| o == word).count(), 1);
            let mut picked: Vec<u16> = options.0.iter().copied().filter(|&o| o != word).collect();
            assert!(picked.iter().all(|o| pool.contains(o)));
            assert_ne!(picked[0], picked[1]);
            picked.sort();
            pairs.insert((picked[0], picked[1]));
            first[options.0.iter().position(|&o| o == word).unwrap()] += 1;
        }
        // C(6, 2) pairs.
        assert_eq!(pairs.len(), 15);
        // The word is first, second and third about equally often.
        let expected = DRAWS as f64 / 3.0;
        for count in first {
            assert!(
                (count as f64 - expected).abs() < expected * 0.05,
                "{first:?}"
            );
        }
    }

    /// A given pick keeps its two lookalikes and only the order is drawn.
    #[test]
    fn a_given_pick_keeps_its_lookalikes() {
        let mut rng = ChaCha20Rng::from_seed([4; 32]);
        let word = 1500;
        let pool = lookalike_pool(word).unwrap();
        let (_, picked) = shuffled_options(&mut rng, word, None).unwrap();
        let mut orders = BTreeSet::new();
        for _ in 0..200 {
            let (options, again) = shuffled_options(&mut rng, word, Some(picked)).unwrap();
            assert_eq!(again, picked);
            let mut shown = options.0;
            orders.insert(shown);
            shown.sort();
            let mut want = [word, pool[picked[0] as usize], pool[picked[1] as usize]];
            want.sort();
            assert_eq!(shown, want);
        }
        assert_eq!(orders.len(), 6, "every order turns up");
    }
}
