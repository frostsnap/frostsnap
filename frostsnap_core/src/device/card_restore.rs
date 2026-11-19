//! Restoring a share from its 64-word card on a device that doesn't hold it (spec L1.8, L1.9.6).
//!
//! The user enters words 1–32 on the device and words 33–64 in the app, which sends them to
//! the device ([`CoordinatorRestoration::CardPhoneHalf`](crate::message::CoordinatorRestoration)).
//! Both halves stay in RAM ([`RestorePending`]): an interrupted restore starts again. Once both
//! are in, the device decodes them; nothing about the decode reaches the phone but
//! `RestoreOutcome`'s result at T_dec, and the report is on the device only.
//!
//! Format C′ (spec PK.8): the share index and scalar are bound by the card's proofs, the
//! threshold isn't. So the device decodes and repairs against F whatever threshold the card
//! claims, and a threshold that differs from F's, or doesn't read, is a warning in the report.
//!
//! **The masks gate the restore** (owner B7, 2 Nov 2026; reverses B4 for this restore). Words
//! 33–64 come from the phone, which can send any of them, and a phone half chosen to decode to
//! s ⊕ Δ would turn the decode into an oracle on s (review/audit-secrecy.md F1, F2). So a decode
//! whose self-checking masks fail, or with F whose wallet-bound masks fail, is "couldn't read"
//! like any other failure: nothing is loaded, no report but the failure is shown, no hold is
//! offered, and the phone hears the same `CouldNotRead` at the same T_dec whatever the target
//! or the reason. With the masks passing, the codeword is the card's own (the masks and the
//! device half leave a phone half no freedom), so nothing the phone sends can steer the share,
//! its index or its threshold. The work is gated too: a candidate is taken on the self masks
//! alone, and only the one they confirm is checked against F, so whether s′·G is in a target
//! the phone chose never decides how long the device works (review/secfix-attack.md B1). And
//! the work is constant (owner R2-1, review RA-2): every decode slice until T_dec does the
//! same work whatever the words, with no early end at a hit and stand-in work once the search
//! is over, so the latency of the device's replies shows neither where the slices end nor
//! when the decode is done ([`RestoreSearch`]). The search is a fixed number of slices, not a
//! time, so starving the decode of slices delays the outcome but never changes it (review B15
//! secrecy S-2). An offline repair tool that has all 64 words
//! on one trusted machine may ignore the masks (spec PK.8); this device never does.
//!
//! **Another wallet's card** (owner B11, 2 Nov 2026; spec PK-O8): one result is told apart from
//! "couldn't read", `WrongWallet`, and only when the card decodes uniquely and its self-checking
//! masks pass, so it's a genuine card, but s·G isn't the phone's F at the card's index or the
//! wallet-bound masks don't match F. Both checks are against the phone's own public F, so the
//! result tells the phone nothing about s it didn't choose to ask about F, and it releases
//! nothing decoded. Every self-mask failure stays the one `CouldNotRead`, frame for frame, and
//! the self masks still come before the target, for the work too. A card that needs a repair to
//! read never gets `WrongWallet`: the repair runs against F, so another wallet's card with too
//! many wrong words is "couldn't read".

use super::card::{redacted_secret, screen, to_coordinator, to_user, wipe, CardScreen, ToUserCard};
use super::restoration::{EnterBackupPhase, PhysicalBackup, WipedShare};
use super::*;
use crate::message::card::{CardHalfEntries, DeviceCard, RestoreResult};
use crate::EnterPhysicalId;
use frost_backup::verifiable_card::{
    codec::{
        CanonicalCheck, DecodeError, Decoded, Entries, IndexSet, RestoreSearch, SearchEnd,
        SearchStage, Work,
    },
    layout::{WordMask, COEFFS, FORMAT_ID, HALF, WORDS},
};

/// A card's threshold field that doesn't read (its top 3 bits aren't 0), as a restored card's
/// [`PhysicalBackup::Card`] records it: that record keeps its shape on flash, where `None` is
/// "not recorded" and a readable threshold is below 256.
pub const UNREADABLE_THRESHOLD: u16 = 0x100;

/// The fixed decode time T_dec: the report waits for it whatever the decode took, and the
/// device works the whole time, in slices that each do the same work whatever the words
/// (owner R2-1), so neither where the slices end nor when the decode is done shows the phone
/// anything. With repair capped at 2 wrong words and the syndrome prefilter, the real search
/// for a wallet of a few shares is a few dozen slices (about 1 s at the slice time estimated
/// below); the rest is stand-in work.
pub const RESTORE_DECODE_MS: u64 = 10_000;
/// The search slices every decode runs before its one read against F, then stand-in slices
/// until T_dec. A search that hasn't found the card by then reports failure. It's a count of
/// slices, not a time (review B15 secrecy S-2): whatever starves the decode of slices (a phone
/// keeping a reply in the outbox, or anything else on the device) only delays the outcome, and
/// never cuts the search short, which would make the outcome say how far the search got.
///
/// At about 25 ms a slice on the ESP32-C3 (estimated, R2 follow-ups §1.5) it's about 6.4 s,
/// within T_dec. The slowest repair, 2 wrong words at the last positions in search order and 1
/// unreadable, reads about 1953·k/256 sets for a k-share wallet, one a slice, so its slice
/// count varies from card to card. On the host, 40 cards a size: 32-44 slices at 3 shares,
/// 124-217 at 20, 199-270 at 30 (up to 8% of these cards over 256) and 210-287 at 32 (20-30%
/// over). So 256 slices cover that repair for every card up to about 28 shares; past that, some
/// of these worst-position cards read as `CouldNotRead`. Spec PK-O26 retunes the count from a
/// high percentile of the read count once the device bench gives the slice time.
pub const RESTORE_SEARCH_SLICES: u32 = 256;
/// The largest wallet threshold the device decodes against. Reading the card against F
/// evaluates F at the card's index and hashes F into the wallet-bound masks, once per decode,
/// so a larger F would make that one step long. A larger F from the app is dropped and the
/// card is decoded as in a fresh recovery, with the R8 warning if the card records another
/// threshold. No card records t > 255, and wallets are rarely above t = 3 (design §9 R22).
const MAX_TARGET_THRESHOLD: usize = 8;

/// What the user entered for each word: a word index, or `None` for "Can't read this word". Its
/// `Debug` shows none of them and it's wiped when dropped.
#[derive(Clone, PartialEq)]
pub struct CardEntries(pub Entries);

redacted_secret!(CardEntries, [None; WORDS]);

impl CardEntries {
    /// The 64 entries from the device's words 1–32 and the app's words 33–64.
    pub fn from_halves(device: &[Option<u16>; HALF], phone: &[Option<u16>; HALF]) -> Self {
        let mut out = Self([None; WORDS]);
        out.0[..HALF].copy_from_slice(device);
        out.0[HALF..].copy_from_slice(phone);
        out
    }
}

/// A card restore in progress, in RAM only: the two halves as they stand, a generation that
/// every change of either bumps, and the generation a running decode started with. Its `Debug`
/// shows no word and it's wiped when dropped.
pub struct RestorePending {
    pub phase: CardRestorePhase,
    device_half: Option<[Option<u16>; HALF]>,
    phone_half: Option<[Option<u16>; HALF]>,
    generation: u32,
    decoding: Option<u32>,
}

impl RestorePending {
    fn new(phase: CardRestorePhase) -> Self {
        Self {
            phase,
            device_half: None,
            phone_half: None,
            generation: 0,
            decoding: None,
        }
    }

    fn state(&self) -> DeviceCard {
        DeviceCard::RestoreState {
            device_half_in: self.device_half.is_some(),
            phone_half_in: self.phone_half.is_some(),
            decoding: self.decoding.is_some(),
        }
    }

    fn wipe_halves(&mut self) {
        for half in [&mut self.device_half, &mut self.phone_half] {
            if let Some(half) = half.as_mut() {
                wipe(half, [None; HALF]);
            }
            *half = None;
        }
    }
}

impl Clone for RestorePending {
    fn clone(&self) -> Self {
        Self {
            phase: self.phase.clone(),
            device_half: self.device_half,
            phone_half: self.phone_half,
            generation: self.generation,
            decoding: self.decoding,
        }
    }
}

impl PartialEq for RestorePending {
    fn eq(&self, other: &Self) -> bool {
        self.phase.enter_physical_id == other.phase.enter_physical_id
            && self.device_half == other.device_half
            && self.phone_half == other.phone_half
            && self.generation == other.generation
            && self.decoding == other.decoding
    }
}

impl core::fmt::Debug for RestorePending {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestorePending")
            .field("enter_physical_id", &self.phase.enter_physical_id)
            .field("device_half_in", &self.device_half.is_some())
            .field("phone_half_in", &self.phone_half.is_some())
            .field("generation", &self.generation)
            .field("decoding", &self.decoding)
            .finish()
    }
}

impl Drop for RestorePending {
    fn drop(&mut self) {
        self.wipe_halves();
    }
}

/// A card restore the app asked for (spec `CardRestoreBegin`).
#[derive(Clone, Debug)]
pub struct CardRestorePhase {
    pub enter_physical_id: EnterPhysicalId,
    /// F, when the app knows the wallet.
    pub target: Option<SharedKey>,
    /// The card format the app's record of the key gives (spec PK.8).
    pub recorded_format: Option<u8>,
    /// The wallet's share indices, for the repair prefilter (build plan A6).
    pub share_indices: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreFailure {
    /// "Couldn't read the card": the words don't decode, or decode to a card whose masks fail
    /// (owner B7). One failure, so the two look the same on both screens.
    DontFit,
    /// Four or more words couldn't be read.
    TooManyUnreadable,
    /// The app's record says the card is a format this firmware doesn't know.
    NeedsNewerFirmware,
    /// A genuine card for another wallet (owner B11): it decoded uniquely and its self-checking
    /// masks passed, but s·G isn't F's image at its index, or its wallet-bound masks aren't F's.
    /// Nothing decoded is released. The restore ends.
    WrongWallet,
}

/// A decode's result. A decoded share and its codeword are wiped when it's dropped.
#[derive(Clone, Debug)]
pub enum RestoreOutcome {
    Decoded(Box<Decoded>),
    Failed(RestoreFailure),
}

impl Drop for RestoreOutcome {
    fn drop(&mut self) {
        if let RestoreOutcome::Decoded(decoded) = self {
            wipe_decoded(decoded);
        }
    }
}

fn wipe_decoded(decoded: &mut Decoded) {
    wipe(&mut decoded.share.share, Scalar::zero());
    wipe(&mut decoded.codeword.coeffs, [0; COEFFS]);
    // A function of s.
    wipe(&mut decoded.masks.ctr, None);
}

/// The canonical check of a card against an F the device dropped as too large to decode
/// against, whose length isn't the card's threshold: only the threshold is known, and it
/// differs, so the report warns (R8) as for any F of another length.
const DROPPED_TARGET_DIFFERS: CanonicalCheck = CanonicalCheck {
    masks: false,
    threshold_word: false,
    hidden: false,
    counter: false,
    threshold: false,
};

/// Whether a card's threshold, as decoded, differs from the wallet's `threshold`: one that reads
/// and is recorded (not 0) and isn't it. One that doesn't read is reported on its own.
fn threshold_differs(card: Option<u8>, threshold: usize) -> bool {
    card.is_some_and(|t| t != 0 && t as usize != threshold)
}

/// What the device reports about a decoded card, on its own screen only. Every decoded card's
/// masks passed (owner B7), so a restored share is the card's own; its warnings never refused
/// it (spec PK.3, PK.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreFindings {
    pub index: ShareIndex,
    /// The threshold the card claims, which the proofs don't bind: `Some(0)` when not recorded,
    /// `None` when it doesn't read (a lying chip, or another format).
    pub threshold: Option<u8>,
    /// The card's threshold reads and isn't the threshold of the wallet record the app sent
    /// (F's length, also when F was too large to decode against). The masks bind the threshold
    /// value to the card, so only the chip that wrote the card can have garbled it.
    pub threshold_differs: bool,
    /// Words whose entries didn't fit the decoded card, bit j for word j + 1. The report names
    /// a word of 33–64 only by its number.
    pub corrected: WordMask,
    /// Words marked "Can't read this word".
    pub erased: WordMask,
    /// Whether the card is what an honest device writes for the wallet record the phone sent,
    /// when it sent one: its masks passed, so a mismatch is the threshold value or the counter
    /// a lying chip picked (spec L1.8.7).
    pub canonical: Option<bool>,
}

/// A decoded share waiting for the user's hold. Its `Debug` shows only the share index and the
/// share is wiped when it's dropped ([`WipedShare`]).
#[derive(Clone)]
pub struct CardRestoreDecoded {
    pub phase: CardRestorePhase,
    pub backup: PhysicalBackup,
}

impl core::fmt::Debug for CardRestoreDecoded {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CardRestoreDecoded")
            .field("index", &self.backup.share_image().index)
            .finish_non_exhaustive()
    }
}

// It travels boxed (`ToUserCard::RestoreReport`), so the size of `Restored` costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum CardRestoreReport {
    Restored {
        decoded: CardRestoreDecoded,
        findings: RestoreFindings,
    },
    /// The device already holds this share, or has a card record for it.
    AlreadyHeld,
    Failed(RestoreFailure),
}

/// The decode of a card restore, a slice per poll, from the user's entries until T_dec
/// (owner R2-1): a unique decode, then with F a repair of up to
/// [`MAX_REPAIR_WRONG`](frost_backup::verifiable_card::codec::MAX_REPAIR_WRONG) wrong words
/// (e + f ≤ 3), as [`RestoreSearch`] slices that each do the same work whatever the words.
/// After exactly [`RESTORE_SEARCH_SLICES`] of them the search ends with one read against F,
/// which costs the same whether a card was found or not, and the slices after it are
/// stand-ins. The outcome comes out at the first slice at or past T_dec once that read is done.
/// F is used whatever threshold the card claims: the threshold isn't bound by the proofs, so it
/// only warns (spec PK.3).
pub struct RestoreJob {
    search: Box<RestoreSearch>,
    /// F, when the app sent one small enough to decode against.
    poly: Option<Vec<Point<Normal, Public, Zero>>>,
    /// The threshold of an F the app sent but that's too large to decode against: the card is
    /// still checked against it for the R8 warning.
    dropped_threshold: Option<usize>,
    recorded_format: Option<u8>,
    /// The search's result, from its finish until T_dec.
    outcome: Option<RestoreOutcome>,
    /// Search slices run, up to [`RESTORE_SEARCH_SLICES`].
    slices: u32,
    finished: bool,
    reported: bool,
    elapsed_ms: u64,
}

impl core::fmt::Debug for RestoreJob {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestoreJob")
            .field("elapsed_ms", &self.elapsed_ms)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl RestoreJob {
    pub fn new(
        entries: &CardEntries,
        target: Option<&SharedKey>,
        recorded_format: Option<u8>,
        share_indices: Option<&[u8]>,
    ) -> Self {
        let (target, dropped_threshold) = match target {
            Some(key) if key.threshold() > MAX_TARGET_THRESHOLD => (None, Some(key.threshold())),
            target => (target, None),
        };
        let repair = target.map(|_| IndexSet::new(share_indices));
        Self {
            search: Box::new(RestoreSearch::new(&entries.0, repair)),
            poly: target.map(|key| key.point_polynomial().to_vec()),
            dropped_threshold,
            recorded_format,
            outcome: None,
            slices: 0,
            finished: false,
            reported: false,
            elapsed_ms: 0,
        }
    }

    /// One slice, given the time since the user's entries were taken: one of the
    /// [`RESTORE_SEARCH_SLICES`] search slices, then the finish, then stand-ins. `Some` exactly
    /// once, at the first slice at or past [`RESTORE_DECODE_MS`] once the finish ran. The
    /// clock only says when to report, never how much of the search runs (review B15 secrecy
    /// S-2).
    pub fn step(&mut self, elapsed_ms: u64) -> Option<RestoreOutcome> {
        self.elapsed_ms = elapsed_ms;
        if self.reported {
            return None;
        }
        if self.slices < RESTORE_SEARCH_SLICES {
            self.slices += 1;
            self.search.slice();
        } else if !self.finished {
            self.finished = true;
            let end = self.search.finish(self.poly.as_deref());
            self.outcome = Some(self.outcome_of(end));
        } else {
            self.search.slice();
        }
        if self.finished && elapsed_ms >= RESTORE_DECODE_MS {
            self.reported = true;
            return self.outcome.take();
        }
        None
    }

    /// What the device reports for how the search ended.
    fn outcome_of(&self, end: SearchEnd) -> RestoreOutcome {
        let failed = RestoreOutcome::Failed;
        let target = self.poly.is_some();
        let outcome = match end {
            SearchEnd::Refused(DecodeError::TooManyErasures) => {
                failed(RestoreFailure::TooManyUnreadable)
            }
            SearchEnd::Refused(_) | SearchEnd::NotFound => failed(RestoreFailure::DontFit),
            // Owner B11: a genuine card (it decoded uniquely and its self masks passed) that
            // isn't for the phone's F. The search checks s·G against F only for a codeword the
            // self masks confirmed (secfix-attack B1), so `NotInTarget` always means they
            // passed.
            SearchEnd::Read(SearchStage::Unique, Err(DecodeError::NotInTarget)) if target => {
                failed(RestoreFailure::WrongWallet)
            }
            SearchEnd::Read(SearchStage::Unique, Ok(mut decoded))
                if decoded.masks.self_ok && decoded.masks.wallet == Some(false) =>
            {
                wipe_decoded(&mut decoded);
                failed(RestoreFailure::WrongWallet)
            }
            // A repair runs against F, so another wallet's card with too many wrong words is
            // "couldn't read".
            SearchEnd::Read(_, Err(_)) => failed(RestoreFailure::DontFit),
            SearchEnd::Read(_, Ok(mut decoded)) => {
                if let Some(dropped) = self.dropped_threshold {
                    if threshold_differs(decoded.threshold, dropped) {
                        decoded.canonical = Some(DROPPED_TARGET_DIFFERS);
                    }
                }
                RestoreOutcome::Decoded(Box::new(decoded))
            }
        };
        // The app's record names a format this firmware doesn't read. The format is public,
        // the phone's own, so the search ran as for any other card and only the result
        // differs.
        if self.recorded_format.is_some_and(|f| f != FORMAT_ID) {
            return failed(RestoreFailure::NeedsNewerFirmware);
        }
        outcome
    }

    /// The fraction of the decode done, for the progress bar: of T_dec, or of the search and
    /// its finish when they take longer.
    pub fn progress(&self) -> f32 {
        let time = self.elapsed_ms as f32 / RESTORE_DECODE_MS as f32;
        let search =
            (self.slices + self.finished as u32) as f32 / (RESTORE_SEARCH_SLICES + 1) as f32;
        time.min(search).min(1.0)
    }

    /// The work the decode has done so far, to check that it doesn't depend on the words.
    pub fn work(&self) -> Work {
        *self.search.work()
    }

    /// Whether the search has nothing left to do (a card was found, or every set of words
    /// was tried). It doesn't change what a slice does.
    pub fn search_done(&self) -> bool {
        self.search.is_done()
    }
}

impl<S: NonceStreamSlot + core::fmt::Debug> FrostSigner<S> {
    /// `EnterCard`: a restore starts, replacing any earlier one. The firmware opens the entry of
    /// words 1–32, and the phone gets the acknowledgement it waits for before it sends its half.
    pub(crate) fn card_restore_enter(&mut self, phase: CardRestorePhase) -> Vec<DeviceSend> {
        let pending = RestorePending::new(phase.clone());
        let state = pending.state();
        self.card.tmp_restore = Some(pending);
        vec![
            to_user(ToUserCard::RestoreEntry(phase)),
            to_coordinator(state),
        ]
    }

    /// `CardPhoneHalf`: the app's words 33–64 for the restore `enter_physical_id` names. One for
    /// another restore, or with none running, is dropped.
    pub(crate) fn card_restore_phone_half(
        &mut self,
        enter_physical_id: EnterPhysicalId,
        words: CardHalfEntries,
    ) -> Vec<DeviceSend> {
        match self.card.tmp_restore.as_mut() {
            Some(pending) if pending.phase.enter_physical_id == enter_physical_id => {
                pending.phone_half = Some(words.0);
            }
            _ => return vec![],
        }
        self.card_restore_changed(false)
    }

    /// The user confirmed words 1–32 on the device.
    #[inline(never)]
    pub fn card_restore_device_half(
        &mut self,
        phase: &CardRestorePhase,
        mut entries: [Option<u16>; HALF],
    ) -> Vec<DeviceSend> {
        match self.card.tmp_restore.as_mut() {
            Some(pending) if pending.phase.enter_physical_id == phase.enter_physical_id => {
                pending.device_half = Some(entries);
            }
            _ => {
                wipe(&mut entries, [None; HALF]);
                return vec![];
            }
        }
        wipe(&mut entries, [None; HALF]);
        self.card_restore_changed(true)
    }

    /// The user opened "Edit words 1–32" on the device: that half is out until it's confirmed
    /// again, and a decode running now is discarded when it ends.
    pub fn card_restore_edit_device_half(&mut self) -> Vec<DeviceSend> {
        match self.card.tmp_restore.as_mut() {
            Some(pending) => {
                if let Some(half) = pending.device_half.as_mut() {
                    wipe(half, [None; HALF]);
                }
                pending.device_half = None;
            }
            None => return vec![],
        }
        self.card_restore_changed(true)
    }

    /// A half changed: the generation moves on, the phone hears the new state, and a decode
    /// starts if both halves are in and none is running. `from_device`: the device's own half
    /// changed, so its screen is the entry's and nothing more is shown.
    fn card_restore_changed(&mut self, from_device: bool) -> Vec<DeviceSend> {
        let Some(pending) = self.card.tmp_restore.as_mut() else {
            return vec![];
        };
        pending.generation = pending.generation.wrapping_add(1);
        let mut out = vec![];
        match (&pending.device_half, &pending.phone_half, pending.decoding) {
            (Some(device), Some(phone), None) => {
                pending.decoding = Some(pending.generation);
                out.push(to_user(ToUserCard::RestoreDecode {
                    phase: pending.phase.clone(),
                    generation: pending.generation,
                    entries: Box::new(CardEntries::from_halves(device, phone)),
                }));
                out.push(screen(CardScreen::RestoreChecking));
            }
            (Some(_), None, None) if from_device => {
                out.push(screen(CardScreen::WaitingForPhoneHalf));
            }
            _ => {}
        }
        out.push(to_coordinator(pending.state()));
        out
    }

    /// The decode of `generation` is done and T_dec has passed: what the device reports. A
    /// decode whose halves changed while it ran is discarded and the current halves are decoded
    /// instead, so exactly one `RestoreOutcome` follows the last change. A decode whose masks
    /// fail is "Couldn't read the card" (owner B7), checked before the device looks at whether
    /// it holds the share. A share this device already holds, or has a card record for, is
    /// refused, and nothing is loaded.
    #[inline(never)]
    pub fn card_restore_decoded(
        &mut self,
        generation: u32,
        outcome: RestoreOutcome,
    ) -> Vec<DeviceSend> {
        // `outcome` wipes the share and its codeword when it's dropped, at the end.
        let Some(pending) = self.card.tmp_restore.as_mut() else {
            return vec![];
        };
        if pending.decoding != Some(generation) {
            return vec![];
        }
        pending.decoding = None;
        if pending.generation != generation {
            return self.card_restore_changed(false);
        }
        let phase = pending.phase.clone();
        let state = pending.state();
        let wallet_threshold = phase.target.as_ref().map(SharedKey::threshold);
        let report = match &outcome {
            RestoreOutcome::Failed(failure) => CardRestoreReport::Failed(*failure),
            // Owner B7, the one gate: a decode whose masks fail is "words don't fit", however
            // it was found, so nothing decoded from it is reported, held or loaded. Against a
            // target the codec already refused it before s·G (secfix-attack B1); this catches
            // a cold decode, and any caller that builds a `Decoded` itself.
            RestoreOutcome::Decoded(decoded) if !decoded.masks.passed() => {
                CardRestoreReport::Failed(RestoreFailure::DontFit)
            }
            RestoreOutcome::Decoded(decoded) => {
                if self.holds_share_image(decoded.share.share_image()) {
                    CardRestoreReport::AlreadyHeld
                } else {
                    let findings = RestoreFindings {
                        index: decoded.share.index,
                        threshold: decoded.threshold,
                        threshold_differs: wallet_threshold
                            .is_some_and(|t| threshold_differs(decoded.threshold, t)),
                        corrected: decoded.codeword.corrected,
                        erased: decoded.codeword.erased,
                        canonical: decoded.canonical.map(|check| check.is_canonical()),
                    };
                    // The record's shape on flash: `None` not recorded, a readable threshold,
                    // or [`UNREADABLE_THRESHOLD`].
                    let threshold = match decoded.threshold {
                        Some(0) => None,
                        Some(t) => Some(t as u16),
                        None => Some(UNREADABLE_THRESHOLD),
                    };
                    CardRestoreReport::Restored {
                        decoded: CardRestoreDecoded {
                            phase,
                            backup: PhysicalBackup::Card {
                                share: WipedShare(decoded.share),
                                threshold,
                            },
                        },
                        findings,
                    }
                }
            }
        };
        let result = match &report {
            CardRestoreReport::Restored { .. } => RestoreResult::Restored,
            CardRestoreReport::AlreadyHeld => RestoreResult::AlreadyHeld,
            CardRestoreReport::Failed(RestoreFailure::NeedsNewerFirmware) => {
                RestoreResult::NeedsNewerFirmware
            }
            CardRestoreReport::Failed(RestoreFailure::WrongWallet) => RestoreResult::WrongWallet,
            CardRestoreReport::Failed(
                RestoreFailure::DontFit | RestoreFailure::TooManyUnreadable,
            ) => RestoreResult::CouldNotRead,
        };
        vec![
            to_coordinator(DeviceCard::RestoreOutcome { result }),
            to_coordinator(state),
            to_user(ToUserCard::RestoreReport(Box::new(report))),
        ]
    }

    /// The user held "Hold to restore this key": the share is loaded, as a 25-word backup is,
    /// and the coordinator told its image. The halves are no longer needed.
    #[inline(never)]
    pub fn card_restore_confirmed(&mut self, decoded: CardRestoreDecoded) -> Vec<DeviceSend> {
        let share_image = decoded.backup.share_image();
        if self.holds_share_image(share_image) {
            return vec![to_user(ToUserCard::RestoreReport(Box::new(
                CardRestoreReport::AlreadyHeld,
            )))];
        }
        self.card.tmp_restore = None;
        let phase = EnterBackupPhase {
            enter_physical_id: decoded.phase.enter_physical_id,
        };
        self.load_physical_backup(phase, decoded.backup.clone())
    }

    /// The user left the restore from an exit page (the key is already held, or the card needs
    /// newer firmware): its halves are wiped and debug output may resume.
    pub fn card_restore_left(&mut self) {
        self.card.tmp_restore = None;
    }

    /// Whether a card restore holds a half in RAM: the firmware mutes its debug output while one
    /// does.
    pub fn card_restore_active(&self) -> bool {
        self.card.tmp_restore.is_some()
    }
}
