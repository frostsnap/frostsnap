//! The verifiable backup card on the device: the persisted card record, its one transition rule
//! ([`State::check`]), the one output gate ([`card_gate`]) and the handlers for card messages and
//! card screens.
//!
//! This build writes the 64-word split card in format C′ (spec Part 4). The flow (spec PK.7,
//! owner B1):
//! 1. The device encodes the card and seals it (311 commitments, H(P) of words 33–64, the coin
//!    commitment), and persists `SealedC` with the hash of its 64 words before any word is shown.
//!    The phone pulls the points and words 33–64 (`PullPhoneHalf`), pins h_C and sends
//!    `WriteStart{h_c}`.
//! 2. The device shows words 1–32 and quizzes the user on them: WRITING, SELF_CHECK, then
//!    CARD_CHECKED. No seal, proof or opening job runs while it does.
//! 3. The user confirms words 1–32 on the phone, whose `DeviceHalfWritten{h_c}` moves
//!    CARD_CHECKED to PROVING. Only then are the 311 bit proofs computed, while the user writes
//!    words 33–64 off the phone; the device shows no word meanwhile (owner B2).
//! 4. Owner B8: the phone's N (`OpenStart`) comes right after its confirm, in PROVING: n_tag
//!    and D are persisted before the coin leaves, and the 5 carry openings of D are computed
//!    after the bit proofs and sent as the phone asks, while words 33–64 are written.
//! 5. Then each side quizzes 5 words of the other's half: the device round, after the phone's
//!    `FinalBegin` (only once N is taken), on 5 of words 33–64 the device drew (E); and the
//!    phone round on 5 of words 1–32 from the coin toss (D). So each screen sees at most 37
//!    words, and any 37 are fully masked.
//!
//! Every re-encode is checked against the persisted hash of the card's words before any word
//! or P leaves the chip (spec PK.6 (2)). A record sealed in a card format other than this
//! firmware's replays as [`CardState::OtherFormat`], and every card request on one is refused
//! with `Unsupported`.
//!
//! The record holds nothing secret. The words, the seal's secrets and the running jobs live in a
//! [`CardSession`], which the firmware holds outside `FrostSigner` (it can't be `Clone` or
//! `PartialEq`). Every handler is infallible: refusals are messages, never `Err`.
//!
//! The device takes the wallet polynomial F from the request (`CardBegin::root_shared_key`),
//! as `DisplayBackup` does. It is bound by the device's own record: the access structure
//! derived from F must exist on the device, F's image at the share's index must be the stored
//! share image, and the share decrypts only under F's decryption contribution.

use super::card_restore::RestorePending;
use super::card_session::{
    BusyTask, CardSession, CardSessionStart, CardUiStep, DrScreen, ProofBuffer, RoundScreen,
};
use super::*;
use crate::message::card::*;
use frost_backup::verifiable_card::{
    layout::{MetaError, COMMITTED, DEVICE_HALF, HALF, PHONE_HALF, WORDS},
    phone,
    proof::{
        self, Ops, CHECK_BOXES, FORMAT_ID, MAX_RUNS, POINTS_PER_FRAME, PROOFS_PER_FRAME,
        PROOF_VERSION,
    },
};

/// The device half's word count, as the record's `u8` positions count it.
const BOXES: u8 = HALF as u8;
/// Every box of D released.
const ALL_RELEASED: u8 = (1 << CHECK_BOXES) - 1;

/// Where a share came from. A share with no record is a legacy share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum ShareOrigin {
    Keygen { card: bool },
    Restored { card: bool },
}

/// Append-only: it's persisted in `Verdict` and sent in `CardStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum VerdictReason {
    PhoneRoundPassed,
    PhoneRoundFailed,
    StoppedBeforeN,
    /// Retired and never written; it holds the wire number of the reason after it.
    _LifetimeCap,
    /// The device round failed.
    DeviceRoundFailed,
}

/// The device round's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum DrResult {
    Pending,
    Passed,
    Failed,
}

/// The device round (spec L2.7.7): E, the current index k into it, and the attempt at E[k].
/// Its `Debug` never shows E.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceRoundRec {
    pub e: [u8; CHECK_BOXES],
    pub k: u8,
    pub attempt: u8,
    pub result: DrResult,
}

impl core::fmt::Debug for DeviceRoundRec {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceRoundRec")
            .field("k", &self.k)
            .field("attempt", &self.attempt)
            .field("result", &self.result)
            .finish_non_exhaustive()
    }
}

/// Device card mutations. Append-only; each is pinned in `tests/device_backward_compat.rs`.
/// Its `Debug` never shows E.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode, frostsnap_macros::Kind)]
pub enum CardMutation {
    /// Insert-only: it applies only when the share has no record.
    Origin {
        share_image: ShareImage,
        origin: ShareOrigin,
    },
    /// The device's self-check of words 1–32 passed: SELF_CHECK → CARD_CHECKED.
    CardChecked { share_image: ShareImage },
    /// The phone's N arrived, in PROVING (owner B8).
    PhoneRound {
        share_image: ShareImage,
        n_tag: [u8; 32],
        d: [u8; CHECK_BOXES],
    },
    /// A run of the opening of word j, a box of D.
    OpenRun { share_image: ShareImage, j: u8 },
    /// The opening of box j of D left the chip: the gate's dots.
    Released { share_image: ShareImage, j: u8 },
    Verdict {
        share_image: ShareImage,
        passed: bool,
        reason: VerdictReason,
    },
    /// PROVING → CHECKING with E ⊂ 32..64 drawn, once N is taken.
    DeviceRoundStarted {
        share_image: ShareImage,
        e: [u8; CHECK_BOXES],
    },
    /// The next word of E, or the second look at this one.
    DeviceRoundStep {
        share_image: ShareImage,
        k: u8,
        attempt: u8,
    },
    /// A fail is FAILED(DeviceRoundFailed) in the same entry.
    DeviceRoundResult { share_image: ShareImage, pass: bool },
    /// SEALED → WRITING, before the first of words 1–32 is drawn.
    SplitWriteStarted { share_image: ShareImage },
    /// WRITING → SELF_CHECK, before the quiz is shown: the user's hold on the device after the
    /// last page of words 1–32.
    DeviceHalfWritten { share_image: ShareImage },
    /// UNSEALED → SEALED for a C′ card (spec PK.6 (2)). `words_hash` is TH(c1/words, L ‖ the 64
    /// words): every later encode of the card is checked against it before any word or P leaves
    /// the chip. One whose `format_id` isn't this firmware's replays as `OtherFormat`.
    SealedC {
        share_image: ShareImage,
        sealed: CardSealed,
        words_hash: [u8; 32],
    },
    /// CARD_CHECKED → PROVING, on the phone's `DeviceHalfWritten{h_c}` (owner B1): the user
    /// confirmed words 1–32 on the phone, and the device's own quiz on them passed. Spec PK.7
    /// calls this record `DeviceHalfWritten`; the device's own hold above already has that name.
    ProvingStarted { share_image: ShareImage },
}

impl core::fmt::Debug for CardMutation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use CardMutation::*;
        let mut d = f.debug_struct(self.kind());
        d.field("share_image", &self.share_image());
        match self {
            Origin { origin, .. } => d.field("origin", origin),
            SealedC {
                sealed, words_hash, ..
            } => d.field("sealed", sealed).field("words_hash", words_hash),
            CardChecked { .. }
            | SplitWriteStarted { .. }
            | DeviceHalfWritten { .. }
            | ProvingStarted { .. } => &mut d,
            PhoneRound {
                n_tag, d: boxes, ..
            } => d.field("n_tag", n_tag).field("d", boxes),
            OpenRun { j, .. } | Released { j, .. } => d.field("j", j),
            Verdict { passed, reason, .. } => d.field("passed", passed).field("reason", reason),
            DeviceRoundStarted { .. } => return d.finish_non_exhaustive(),
            DeviceRoundStep { k, attempt, .. } => d.field("k", k).field("attempt", attempt),
            DeviceRoundResult { pass, .. } => d.field("pass", pass),
        };
        d.finish()
    }
}

impl CardMutation {
    pub fn share_image(&self) -> ShareImage {
        use CardMutation::*;
        match self {
            Origin { share_image, .. }
            | CardChecked { share_image }
            | PhoneRound { share_image, .. }
            | OpenRun { share_image, .. }
            | Released { share_image, .. }
            | Verdict { share_image, .. }
            | DeviceRoundStarted { share_image, .. }
            | DeviceRoundStep { share_image, .. }
            | DeviceRoundResult { share_image, .. }
            | SplitWriteStarted { share_image }
            | DeviceHalfWritten { share_image }
            | SealedC { share_image, .. }
            | ProvingStarted { share_image } => *share_image,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CardState {
    Unsealed,
    Sealed,
    /// Words 1–32 on the device screen. Nothing per word is persisted: a resume starts at word 1.
    Writing,
    /// The device's quiz on words 1–32. Nothing per item is persisted.
    SelfCheck,
    /// The device's self-check passed; the phone hasn't confirmed words 1–32 yet.
    CardChecked,
    /// The user confirmed words 1–32 on the phone: the bit proofs are computed while the phone
    /// half is written, and the device shows no word (owner B1, B2).
    Proving,
    /// The device round, then (after N) the phone round.
    Checking,
    Passed,
    Failed(VerdictReason),
    /// Sealed in a card format other than this firmware's: every card request is refused with
    /// `Unsupported` (design O8).
    OtherFormat,
}

impl CardState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, CardState::Passed | CardState::Failed(_))
    }

    /// UNSEALED through PROVING: the session holds the seal's secrets and may need the seal.
    pub(super) fn before_checking(&self) -> bool {
        matches!(
            self,
            CardState::Unsealed
                | CardState::Sealed
                | CardState::Writing
                | CardState::SelfCheck
                | CardState::CardChecked
                | CardState::Proving
        )
    }

    /// Words 33–64 are served (`PullPhoneHalf`): SEALED through CHECKING, before the verdict, so
    /// a phone that lost them can recover them, even mid D check (build plan A10).
    fn serves_p(&self) -> bool {
        matches!(
            self,
            CardState::Sealed
                | CardState::Writing
                | CardState::SelfCheck
                | CardState::CardChecked
                | CardState::Proving
                | CardState::Checking
        )
    }

    /// A `WriteStart` resumes the screen of this state: the writing pages, the quiz, "Now use
    /// your phone", or the proving screen.
    fn resumes_writing(&self) -> bool {
        matches!(
            self,
            CardState::Writing | CardState::SelfCheck | CardState::CardChecked | CardState::Proving
        )
    }

    /// The user may be writing or quizzing words 1–32 on the device: no seal, proof or opening
    /// job may run, so their timing can't show when the user touched the device (spec PK.7).
    fn writing_device_half(&self) -> bool {
        matches!(
            self,
            CardState::Writing | CardState::SelfCheck | CardState::CardChecked
        )
    }

    fn report(&self) -> CardStateReport {
        match self {
            // A status is only sent once sealed, and never for another format's record.
            CardState::Unsealed | CardState::Sealed | CardState::OtherFormat => {
                CardStateReport::Sealed
            }
            CardState::Writing => CardStateReport::Writing,
            CardState::SelfCheck => CardStateReport::SelfCheck,
            CardState::CardChecked => CardStateReport::CardChecked,
            CardState::Proving => CardStateReport::Proving,
            CardState::Checking => CardStateReport::Checking,
            CardState::Passed => CardStateReport::Passed,
            CardState::Failed(_) => CardStateReport::Failed,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardRecord {
    pub origin: ShareOrigin,
    pub sealed: Option<CardSealed>,
    /// The hash of the card's 64 words, from `SealedC`.
    pub words_hash: Option<[u8; 32]>,
    pub state: CardState,
    /// Kept after the verdict, so a repeated `PhoneRound` gets the verdict again.
    pub n_tag: Option<[u8; 32]>,
    pub d: Option<[u8; CHECK_BOXES]>,
    /// Runs per word of D's openings (words 1–32). Words 33–64 are never opened.
    pub runs: [u8; WORDS],
    /// Bit k: the opening of d[k] was released.
    pub released: u8,
    pub device_round: Option<DeviceRoundRec>,
}

impl CardRecord {
    fn new(origin: ShareOrigin) -> Self {
        Self {
            origin,
            sealed: None,
            words_hash: None,
            state: CardState::Unsealed,
            n_tag: None,
            d: None,
            runs: [0; WORDS],
            released: 0,
            device_round: None,
        }
    }

    /// k with d[k] = j.
    fn d_position(&self, j: u8) -> Option<usize> {
        self.d?.iter().position(|&b| b == j)
    }

    fn origin_refusal(&self) -> Result<(), CardRefusal> {
        match self.origin {
            ShareOrigin::Keygen { card: true } => Ok(()),
            ShareOrigin::Keygen { card: false } => Err(CardRefusal::NotCardKey),
            ShareOrigin::Restored { .. } => Err(CardRefusal::NotKeygenOrigin),
        }
    }

    /// The device round passed: the phone round may start.
    pub fn device_round_passed(&self) -> bool {
        self.device_round
            .is_some_and(|dr| dr.result == DrResult::Passed)
    }

    /// The gate's dots.
    pub fn dots(&self) -> u8 {
        self.released.count_ones() as u8
    }

    /// Runs of D's openings so far.
    fn d_runs(&self) -> u8 {
        self.runs[DEVICE_HALF].iter().sum()
    }

    fn status(&self, share_image: ShareImage) -> Option<CardStatus> {
        let check = match self.state {
            // Owner B8: N is taken in PROVING, before the device round.
            CardState::Proving => match self.n_tag {
                Some(n_tag) => CardCheckReport::Started {
                    n_tag,
                    runs: self.d_runs(),
                },
                None => CardCheckReport::None,
            },
            CardState::Checking => match (self.n_tag, self.device_round.map(|dr| dr.result)) {
                (Some(n_tag), Some(DrResult::Pending)) => {
                    CardCheckReport::StartedDeviceRoundPending {
                        n_tag,
                        runs: self.d_runs(),
                    }
                }
                (Some(n_tag), _) => CardCheckReport::Started {
                    n_tag,
                    runs: self.d_runs(),
                },
                (None, Some(DrResult::Pending)) => CardCheckReport::DeviceRoundPending,
                (None, Some(DrResult::Passed)) => CardCheckReport::DeviceRoundPassed,
                // A failed round is FAILED, so this can't be.
                (None, _) => return None,
            },
            CardState::Passed => CardCheckReport::Ended { passed: true },
            CardState::Failed(_) => CardCheckReport::Ended { passed: false },
            _ => CardCheckReport::None,
        };
        let reason = match self.state {
            CardState::Passed => Some(VerdictReason::PhoneRoundPassed),
            CardState::Failed(reason) => Some(reason),
            _ => None,
        };
        Some(CardStatus {
            share_image,
            format_id: FORMAT_ID,
            proof_version: PROOF_VERSION,
            sealed: self.sealed?,
            state: self.state.report(),
            check,
            reason,
        })
    }
}

/// A later check asked for by `Begin` on a PASSED record: the share, until `Recheck` opens a
/// session for it.
#[derive(Clone, Debug, PartialEq)]
struct RecheckBegin {
    share_image: ShareImage,
    begin: CardBegin,
}

/// The card records, keyed by share image. The share slot is replayed after the log, so a
/// record never depends on its share having loaded; a record whose share is no longer held is
/// inert.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub records: BTreeMap<ShareImage, CardRecord>,
    /// The share whose verdict screen is up. That screen can come with no session (it's found
    /// by n_tag), and its acks lead to the next-step pages.
    tmp_verdict_shown: Option<ShareImage>,
    /// The last `Begin` on a PASSED record, for a `Recheck`.
    tmp_recheck_begin: Option<RecheckBegin>,
    /// A `Recheck` that opened a session: its hash, until the session is opened.
    tmp_recheck_hash: Option<(ShareImage, Option<[u8; 32]>)>,
    /// A card restore in progress: its halves, held in RAM only.
    pub(super) tmp_restore: Option<RestorePending>,
}

impl State {
    pub fn clear_tmp_data(&mut self) {
        self.tmp_verdict_shown = None;
        self.tmp_recheck_begin = None;
        self.tmp_recheck_hash = None;
        self.tmp_restore = None;
    }

    /// THE transition rule. Replay ([`apply`](Self::apply)) and live mutations
    /// ([`FrostSigner::card_mutate`]) both go through it, and no handler checks any of it
    /// again. `Ok` for a mutation that would be a no-op (an `Origin` for a share that has a
    /// record, the same `SealedC` again, a box released again).
    pub fn check(&self, m: &CardMutation) -> Result<(), CardRefusal> {
        use CardRefusal::*;
        let rec = match (m, self.records.get(&m.share_image())) {
            (CardMutation::Origin { .. }, _) => return Ok(()),
            (_, None) => return Err(NotKeygenOrigin),
            (_, Some(rec)) => rec,
        };
        if rec.state == CardState::OtherFormat {
            return Err(Unsupported);
        }
        if rec.state.is_terminal() {
            return Err(Closed);
        }
        let ok_or = |ok: bool, refusal| if ok { Ok(()) } else { Err(refusal) };
        let dr = rec.device_round;
        match *m {
            CardMutation::Origin { .. } => Ok(()),
            CardMutation::SealedC {
                sealed, words_hash, ..
            } => {
                rec.origin_refusal()?;
                if sealed.format_id != FORMAT_ID {
                    return Err(Unsupported);
                }
                match (rec.sealed, rec.words_hash) {
                    (Some(stored), Some(hash)) if stored == sealed && hash == words_hash => Ok(()),
                    (None, _) if rec.state == CardState::Unsealed => Ok(()),
                    _ => Err(WrongState),
                }
            }
            CardMutation::SplitWriteStarted { .. } => {
                rec.origin_refusal()?;
                let format = rec.sealed.map(|sealed| sealed.format_id) == Some(FORMAT_ID);
                ok_or(rec.state == CardState::Sealed && format, WrongState)
            }
            CardMutation::DeviceHalfWritten { .. } => {
                ok_or(rec.state == CardState::Writing, WrongState)
            }
            CardMutation::CardChecked { .. } => {
                ok_or(rec.state == CardState::SelfCheck, WrongState)
            }
            CardMutation::ProvingStarted { .. } => {
                ok_or(rec.state == CardState::CardChecked, WrongState)
            }
            CardMutation::PhoneRound { d, .. } => {
                // Owner B8: in PROVING, before the device round.
                if !(rec.state == CardState::Proving && rec.n_tag.is_none()) {
                    return Err(WrongState);
                }
                let distinct = d
                    .iter()
                    .enumerate()
                    .all(|(k, &j)| j < BOXES && !d[..k].contains(&j));
                ok_or(distinct, BadRequest)
            }
            CardMutation::OpenRun { j, .. } => {
                if !matches!(rec.state, CardState::Checking | CardState::Proving) {
                    return Err(WrongState);
                }
                rec.d_position(j).ok_or(NotInD)?;
                ok_or(rec.runs[j as usize] < MAX_RUNS, RunCap)
            }
            CardMutation::Released { j, .. } => {
                if !matches!(rec.state, CardState::Checking | CardState::Proving) {
                    return Err(WrongState);
                }
                rec.d_position(j).ok_or(NotInD)?;
                ok_or(rec.runs[j as usize] >= 1, WrongState)
            }
            CardMutation::Verdict { passed, reason, .. } => {
                let ok = match (passed, reason, &rec.state) {
                    (true, VerdictReason::PhoneRoundPassed, CardState::Checking) => {
                        rec.released == ALL_RELEASED
                            && rec.n_tag.is_some()
                            && rec.device_round_passed()
                    }
                    (
                        false,
                        VerdictReason::PhoneRoundFailed,
                        CardState::Checking | CardState::Proving,
                    ) => rec.n_tag.is_some(),
                    (false, VerdictReason::StoppedBeforeN, CardState::CardChecked) => true,
                    (false, VerdictReason::StoppedBeforeN, CardState::Proving) => {
                        rec.n_tag.is_none()
                    }
                    _ => false,
                };
                ok_or(ok, WrongState)
            }
            CardMutation::DeviceRoundStarted { e, .. } => {
                // Owner B8: the device round runs only once N is taken.
                if rec.state != CardState::Proving || rec.n_tag.is_none() {
                    return Err(WrongState);
                }
                let distinct = e
                    .iter()
                    .enumerate()
                    .all(|(k, &j)| PHONE_HALF.contains(&(j as usize)) && !e[..k].contains(&j));
                ok_or(distinct, BadRequest)
            }
            CardMutation::DeviceRoundStep { k, attempt, .. } => match dr {
                Some(dr) if rec.state == CardState::Checking && dr.result == DrResult::Pending => {
                    let again = k == dr.k && dr.attempt == 0 && attempt == 1;
                    let next = k == dr.k + 1 && (k as usize) < CHECK_BOXES && attempt == 0;
                    ok_or(again || next, BadRequest)
                }
                _ => Err(WrongState),
            },
            CardMutation::DeviceRoundResult { pass, .. } => match dr {
                Some(dr) if rec.state == CardState::Checking && dr.result == DrResult::Pending => {
                    let ok = if pass {
                        dr.k as usize == CHECK_BOXES - 1
                    } else {
                        dr.attempt == 1
                    };
                    ok_or(ok, BadRequest)
                }
                _ => Err(WrongState),
            },
        }
    }

    /// Applies a mutation that passes [`check`](Self::check). It returns the mutation to log,
    /// or `None` for one that fails the rule (a bad log entry is refused, never applied) or
    /// changes nothing.
    ///
    /// A `SealedC` of another card format replays as `OtherFormat`.
    pub fn apply(&mut self, m: CardMutation) -> Option<CardMutation> {
        let share_image = m.share_image();
        if let CardMutation::SealedC { sealed, .. } = m {
            if sealed.format_id != FORMAT_ID {
                let rec = self.records.get_mut(&share_image)?;
                rec.sealed.get_or_insert(sealed);
                rec.state = CardState::OtherFormat;
                return Some(m);
            }
        }
        self.check(&m).ok()?;
        if let CardMutation::Origin { origin, .. } = m {
            if self.records.contains_key(&share_image) {
                return None;
            }
            self.records.insert(share_image, CardRecord::new(origin));
            return Some(m);
        }
        let rec = self.records.get_mut(&share_image)?;
        match m {
            CardMutation::Origin { .. } => return None,
            CardMutation::SealedC {
                sealed, words_hash, ..
            } => {
                if rec.sealed.is_some() {
                    return None;
                }
                rec.sealed = Some(sealed);
                rec.words_hash = Some(words_hash);
                rec.state = CardState::Sealed;
            }
            CardMutation::SplitWriteStarted { .. } => rec.state = CardState::Writing,
            CardMutation::DeviceHalfWritten { .. } => rec.state = CardState::SelfCheck,
            CardMutation::CardChecked { .. } => rec.state = CardState::CardChecked,
            CardMutation::ProvingStarted { .. } => rec.state = CardState::Proving,
            // Owner B8: PROVING stays PROVING; the device round moves it to CHECKING.
            CardMutation::PhoneRound { n_tag, d, .. } => {
                rec.n_tag = Some(n_tag);
                rec.d = Some(d);
            }
            CardMutation::OpenRun { j, .. } => rec.runs[j as usize] += 1,
            CardMutation::Released { j, .. } => {
                let bit = 1 << rec.d_position(j)?;
                if rec.released & bit != 0 {
                    return None;
                }
                rec.released |= bit;
            }
            CardMutation::Verdict { passed, reason, .. } => {
                rec.state = if passed {
                    CardState::Passed
                } else {
                    CardState::Failed(reason)
                };
            }
            CardMutation::DeviceRoundStarted { e, .. } => {
                rec.device_round = Some(DeviceRoundRec {
                    e,
                    k: 0,
                    attempt: 0,
                    result: DrResult::Pending,
                });
                rec.state = CardState::Checking;
            }
            CardMutation::DeviceRoundStep { k, attempt, .. } => {
                let dr = rec.device_round.as_mut()?;
                dr.k = k;
                dr.attempt = attempt;
            }
            CardMutation::DeviceRoundResult { pass, .. } => {
                let dr = rec.device_round.as_mut()?;
                if pass {
                    dr.result = DrResult::Passed;
                } else {
                    dr.result = DrResult::Failed;
                    rec.state = CardState::Failed(VerdictReason::DeviceRoundFailed);
                }
            }
        }
        Some(m)
    }
}

/// An output that leaves the chip: a `DeviceCard` message, or a screen with a card word on it.
/// Each passes [`card_gate`] first.
pub enum CardOutput<'a> {
    /// A session opens. It isn't refused as `Closed`: a terminal Begin gets the status.
    SessionOpen {
        format_id: u8,
        proof_version: u8,
        index: ShareIndex,
        threshold: usize,
    },
    Status,
    CommitPoints {
        from: u16,
        count: u8,
    },
    BitProofs {
        from: u16,
        count: u8,
    },
    CheckDone,
    Coin {
        n: &'a [u8; 32],
    },
    /// An opening of a box of D, in the phone round.
    Opening {
        n: &'a [u8; 32],
        j: u8,
    },
    /// Words 33–64 to the phone (`PhoneHalf`). No word of 1–32 is ever in it.
    PhoneHalf,
    /// None for the reply to a stop before N.
    Verdict {
        n: Option<&'a [u8; 32]>,
    },
    /// Words 1–32 on the device screen: the writing pages and the self-check quiz.
    DeviceHalfScreen,
    /// The device round's answer screen for word j of E.
    DeviceRoundScreen {
        j: u8,
    },
    /// `DeviceRound{pass}`.
    DeviceRoundVerdict,
    /// One of the verdict screens.
    VerdictScreen {
        screen: CardScreen,
    },
    /// A later check's words 33–64 (and D's words) to the phone that holds a hash over them.
    RecheckWords {
        words_hash: Option<&'a [u8; 32]>,
    },
    /// A later check's quiz on the device: words 1–32 and E.
    RecheckScreen,
    /// Today's 25-word `DisplayBackup` and `CheckBackup`.
    LegacyDisplay,
    /// The phone is told the gate screen is up.
    GateShown,
}

/// What the gate needs from the session.
pub struct SessionFacts<'a> {
    /// `WriteStart{h_C}` matched the record in this session.
    pub write_started: bool,
    /// This session's seal is in RAM and matched the record.
    pub seal_ready: bool,
    pub proofs: &'a ProofBuffer,
    /// The gate screen is up.
    pub at_gate: bool,
    /// The card's words, once the session's encode job built them and they matched the
    /// record's words hash.
    pub words: Option<&'a [u16; WORDS]>,
}

/// Which of the phone's hashes a later check's `words_hash` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecheckMatch {
    /// h37: words 33–64 and D's words.
    H37,
    /// h32: words 33–64 only.
    H32,
}

/// The phone's hash over the words it was given, recomputed from the device's own card: h37 if
/// D is drawn and it matches, else h32 (design §2.7). `None` for no match.
pub fn recheck_match(
    rec: &CardRecord,
    words: &[u16; WORDS],
    words_hash: Option<&[u8; 32]>,
) -> Option<RecheckMatch> {
    let words_hash = words_hash?;
    let h_c = rec.sealed?.h_c;
    let mut phone_half: [u16; HALF] = words[PHONE_HALF].try_into().expect("32 words");
    let found = if let Some(d) = rec.d {
        let mut d_words = d.map(|j| words[j as usize]);
        let h37 = phone::h37(&h_c, &phone_half, &d, &d_words);
        wipe(&mut d_words, [0; CHECK_BOXES]);
        (h37 == *words_hash).then_some(RecheckMatch::H37)
    } else {
        None
    };
    let found = found
        .or_else(|| (phone::h32(&h_c, &phone_half) == *words_hash).then_some(RecheckMatch::H32));
    wipe(&mut phone_half, [0; HALF]);
    found
}

/// The one boundary (spec L1.6.2): whether `out` may leave the chip, given the share's record
/// and the session. It's pure; every card output is built only after it passed.
///
/// It holds the 37-word bound: words 1–32 go only to the device screen (and the 5 of D to the
/// phone, at the gate), words 33–64 only to the phone (and the 5 of E to the device screen).
pub fn card_gate(
    rec: Option<&CardRecord>,
    s: Option<&SessionFacts>,
    out: &CardOutput,
    ops: &mut Ops,
) -> Result<(), CardRefusal> {
    use CardRefusal::*;
    use CardState as St;
    if let CardOutput::LegacyDisplay = out {
        return match rec.map(|rec| rec.origin) {
            Some(ShareOrigin::Keygen { card: true })
            | Some(ShareOrigin::Restored { card: true }) => Err(WrongState),
            _ => Ok(()),
        };
    }
    let rec = rec.ok_or(NotKeygenOrigin)?;
    if rec.state == St::OtherFormat {
        return Err(Unsupported);
    }
    rec.origin_refusal()?;
    let terminal = rec.state.is_terminal();
    let dr = rec.device_round.map(|dr| dr.result);
    match out {
        CardOutput::SessionOpen {
            format_id,
            proof_version,
            index,
            threshold,
        } => {
            if *format_id != FORMAT_ID || *proof_version != PROOF_VERSION {
                return Err(Unsupported);
            }
            frost_backup::verifiable_card::layout::Meta::new(*index, *threshold)
                .map(|_| ())
                .map_err(|e| match e {
                    MetaError::MetadataRange => MetadataRange,
                    MetaError::ThresholdOne => ThresholdOne,
                })
        }
        CardOutput::Status => rec.sealed.map(|_| ()).ok_or(Busy),
        CardOutput::Verdict { n } => {
            if !terminal {
                return Err(WrongState);
            }
            match n {
                Some(n) => n_matches(rec, n, ops),
                None => matches!(rec.state, St::Failed(VerdictReason::StoppedBeforeN))
                    .then_some(())
                    .ok_or(WrongState),
            }
        }
        CardOutput::VerdictScreen { screen } => {
            let verdict = matches!(
                screen,
                CardScreen::Verdict { .. }
                    | CardScreen::CardDone
                    | CardScreen::PhoneRoundFailed
                    | CardScreen::NextStep
                    | CardScreen::Stopped
                    | CardScreen::StoppedNextStep
                    | CardScreen::DeviceRoundFailed
                    | CardScreen::DeviceRoundNextStep
            );
            (terminal && verdict).then_some(()).ok_or(WrongState)
        }
        // A failed round's verdict goes out after the FAILED it reports is persisted.
        CardOutput::DeviceRoundVerdict
            if rec.state == St::Failed(VerdictReason::DeviceRoundFailed) =>
        {
            Ok(())
        }
        // A later check reads the persisted D and E, and only a phone that holds a hash over
        // words 33–64 gets them again (design §3.9).
        CardOutput::RecheckWords { words_hash } => {
            if rec.state != St::Passed {
                return Err(WrongState);
            }
            let words = s.ok_or(NoSession)?.words.ok_or(Busy)?;
            recheck_match(rec, words, *words_hash)
                .map(|_| ())
                .ok_or(NoRecord)
        }
        CardOutput::RecheckScreen => {
            if rec.state != St::Passed || rec.device_round.is_none() {
                return Err(WrongState);
            }
            s.ok_or(NoSession)?.words.map(|_| ()).ok_or(Busy)
        }
        _ if terminal => Err(Closed),
        // While words 1–32 are written and quizzed the seal is served from RAM only: no seal job
        // may run then (spec PK.7, build plan A4), so `Busy`, which starts one, is for SEALED and
        // PROVING alone.
        CardOutput::CommitPoints { from, count } => {
            if !rec.state.before_checking() || rec.state == St::Unsealed {
                return Err(WrongState);
            }
            if *count as usize > POINTS_PER_FRAME || *from as usize + *count as usize > COMMITTED {
                return Err(BadRequest);
            }
            match (
                s.ok_or(NoSession)?.seal_ready,
                rec.state.writing_device_half(),
            ) {
                (true, _) => Ok(()),
                (false, true) => Err(WrongState),
                (false, false) => Err(Busy),
            }
        }
        // In PROVING only (owner B1): the user has written and quizzed words 1–32 and confirmed
        // them on the phone, and the device shows no word while the proofs run (owner B2), so
        // their timing depends on nothing the user does with a word on the device. Before PROVING
        // it's `Busy`, and the handler arms nothing (build plan A4).
        CardOutput::BitProofs { from, count } => {
            match rec.state {
                St::Proving => {}
                St::Checking => return Err(WrongState),
                _ => return Err(Busy),
            }
            if *count as usize > PROOFS_PER_FRAME || *from as usize + *count as usize > COMMITTED {
                return Err(BadRequest);
            }
            let s = s.ok_or(NoSession)?;
            (*from..*from + *count as u16)
                .all(|v| s.proofs.has(v))
                .then_some(())
                .ok_or(Busy)
        }
        CardOutput::CheckDone => (rec.state == St::CardChecked)
            .then_some(())
            .ok_or(WrongState),
        // Owner B8: N is taken in PROVING (the device's quiz passed and the phone confirmed
        // words 1–32), and the coin is sent again with the same N up to the verdict.
        CardOutput::Coin { n } => {
            if !n_phase(rec.state.clone(), dr) {
                return Err(WrongState);
            }
            n_matches(rec, n, ops)?;
            s.ok_or(NoSession).map(|_| ())
        }
        // Owner B8: openings leave the chip from PROVING on, computed after the bit proofs while
        // words 33–64 are written; the device shows no word then. The phone holds them and asks
        // a question only once the gate is up (`GateShown`, spec §0.6.1.2 item 2).
        CardOutput::Opening { n, j } => {
            if !n_phase(rec.state.clone(), dr) {
                return Err(WrongState);
            }
            n_matches(rec, n, ops)?;
            rec.d_position(*j).ok_or(NotInD)?;
            s.ok_or(NoSession).map(|_| ())
        }
        // Words 33–64 only, from SEALED through CHECKING (build plan A10). The phone needs them
        // to pin, so `WriteStart` isn't required; `words` is set only once the encode matched
        // the persisted words hash (spec PK.6 (2)).
        CardOutput::PhoneHalf => {
            if !rec.state.serves_p() {
                return Err(WrongState);
            }
            s.ok_or(NoSession)?.words.map(|_| ()).ok_or(Busy)
        }
        // After `OpenAccepted` (addendum §1.2 row 14): `OpenStart` sends it if the gate is
        // already up.
        CardOutput::GateShown => {
            let at_gate = s.ok_or(NoSession)?.at_gate;
            let n_taken = rec.n_tag.is_some();
            (rec.state == St::Checking && dr == Some(DrResult::Passed) && n_taken && at_gate)
                .then_some(())
                .ok_or(WrongState)
        }
        // `write_started` needs `Sealed` persisted and the phone's h_C to match: the phone
        // pinned the seal before any word is shown.
        CardOutput::DeviceHalfScreen => {
            if !matches!(rec.state, St::Writing | St::SelfCheck) {
                return Err(WrongState);
            }
            let s = s.ok_or(NoSession)?;
            if !s.write_started {
                return Err(WrongState);
            }
            s.words.map(|_| ()).ok_or(Busy)
        }
        CardOutput::DeviceRoundScreen { j } => {
            let on_screen = match (rec.device_round, &rec.state) {
                (Some(dr), St::Checking) if dr.result == DrResult::Pending => {
                    dr.e.get(dr.k as usize) == Some(j)
                }
                _ => false,
            };
            if !on_screen {
                return Err(WrongState);
            }
            s.ok_or(NoSession)?.words.map(|_| ()).ok_or(Busy)
        }
        CardOutput::DeviceRoundVerdict => (rec.state == St::Checking
            && dr == Some(DrResult::Passed))
        .then_some(())
        .ok_or(WrongState),
        CardOutput::LegacyDisplay => unreachable!("handled first"),
    }
}

/// Where N and the openings are served (owner B8): PROVING, or CHECKING with the device round
/// running or passed.
fn n_phase(state: CardState, dr: Option<DrResult>) -> bool {
    match state {
        CardState::Proving => true,
        CardState::Checking => matches!(dr, Some(DrResult::Pending | DrResult::Passed)),
        _ => false,
    }
}

/// The phone's N is the one this check was started with.
fn n_matches(rec: &CardRecord, n: &[u8; 32], ops: &mut Ops) -> Result<(), CardRefusal> {
    match rec.n_tag {
        Some(n_tag) if n_tag == proof::n_tag(n, ops) => Ok(()),
        Some(_) => Err(CardRefusal::AlreadyUsed),
        None => Err(CardRefusal::WrongState),
    }
}

/// Messages to the firmware about the card.
#[derive(Clone, Debug)]
pub enum ToUserCard {
    /// Open a session: the firmware drops any old one first, then calls
    /// [`CardSessionStart::open`] and [`FrostSigner::card_session_opened`].
    OpenSession(Box<CardSessionStart>),
    Screen(CardScreen),
    CloseSession,
    /// The phone asked for a card restore (`EnterCard`): the firmware opens its entry of words
    /// 1–32, and calls [`FrostSigner::card_restore_device_half`] once the user confirms them.
    RestoreEntry(super::card_restore::CardRestorePhase),
    /// Both halves are in: the firmware starts the decode
    /// ([`RestoreJob`](super::card_restore::RestoreJob)) and T_dec from now, and passes
    /// `generation` back to [`FrostSigner::card_restore_decoded`].
    RestoreDecode {
        phase: super::card_restore::CardRestorePhase,
        generation: u32,
        entries: Box<super::card_restore::CardEntries>,
    },
    RestoreReport(Box<super::card_restore::CardRestoreReport>),
}

/// The line under a key's name on the home screen (spec §0.7.5, design §7.5 D21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeLine {
    /// "Backup not checked": a card share in any state but PASSED.
    NotChecked,
    /// "Backup failed: don't use".
    Failed,
}

/// Overwrites a secret in place, as `frost_backup`'s card code does. `black_box` makes the
/// compiler treat the cleared value as read, so the write isn't dropped as dead.
pub(crate) fn wipe<T: Copy>(secret: &mut T, cleared: T) {
    *secret = cleared;
    core::hint::black_box(secret);
}

/// The 64 words of a card, word j + 1 holding `0[j]`. It stays inside core: only a half is ever
/// put into a screen. Its `Debug` shows none of them and it's wiped when dropped.
#[derive(Clone, PartialEq)]
pub struct CardWords(pub [u16; WORDS]);

/// The device round's three options: one word and two of its six closest lookalikes, in a
/// fresh order. Its `Debug` shows none of them and it's wiped when dropped.
#[derive(Clone, PartialEq)]
pub struct CardWords3(pub [u16; 3]);

/// A quiz on the device: (word number, word) per item. Its `Debug` shows none of them and it's
/// wiped when dropped.
#[derive(Clone, PartialEq)]
pub struct CardQuizItems(pub Vec<(u8, u16)>);

macro_rules! redacted_secret {
    ($name:ident, $cleared:expr) => {
        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}(<redacted>)", stringify!($name))
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                $crate::device::card::wipe(&mut self.0, $cleared);
            }
        }
    };
}

redacted_secret!(CardWords, [0; WORDS]);
redacted_secret!(CardWords3, [0; 3]);
pub(crate) use redacted_secret;

impl core::fmt::Debug for CardQuizItems {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "CardQuizItems(<redacted>)")
    }
}

impl Drop for CardQuizItems {
    fn drop(&mut self) {
        for item in self.0.iter_mut() {
            wipe(item, (0, 0));
        }
    }
}

/// The card screens. Core emits one per step; the next one comes from the step's
/// [`CardUserAction`]. Its `Debug` shows the variant name only: never a word.
#[derive(Clone, PartialEq)]
pub enum CardScreen {
    Preparing {
        task: BusyTask,
    },
    /// "Look at your phone".
    WaitingForPhone,
    /// Words 1–32 to write: the standard backup display. Ends with
    /// [`CardUserAction::DeviceHalfWritten`].
    DeviceHalf {
        words: CardWords32,
    },
    /// The device's quiz on words 1–32, the standard check quiz, retry until right. Ends
    /// with [`CardUserAction::SelfCheckPassed`].
    SelfCheck {
        items: CardQuizItems,
        seed: u32,
    },
    /// "Now use your phone": the device's quiz passed, and the phone asks the user to confirm
    /// words 1–32, then shows words 33–64.
    PhoneHalfNext,
    /// A later check: words 1–32 then the 5 words of E, the standard check quiz, retry until
    /// right. Ends with [`CardUserAction::RecheckPassed`].
    Recheck {
        items: CardQuizItems,
        seed: u32,
    },
    /// "Backup checked", after a later check.
    RecheckDone,
    /// Restore: words 1–32 are entered; waiting for the app's words 33–64.
    WaitingForPhoneHalf,
    RestoreChecking,
    /// "Look at your phone. It starts the final check."
    CardFinishedWaiting,
    DeviceRoundIntro,
    DeviceRoundAnswer {
        j: u8,
        k: u8,
        options: CardWords3,
        display_id: u32,
    },
    DeviceRoundMismatch {
        j: u8,
    },
    DeviceRoundFailed,
    DeviceRoundNextStep,
    Gate {
        dots: u8,
        preparing: bool,
    },
    Verdict {
        passed: bool,
    },
    CardDone,
    PhoneRoundFailed,
    /// A stop before N, on either screen.
    Stopped,
    /// The next step after a stop before N: "Set this wallet up again."
    StoppedNextStep,
    /// The next step after a failed phone round.
    NextStep,
    RedisplayRefused,
    Refused {
        reason: CardRefusal,
    },
    DeviceError,
}

impl core::fmt::Debug for CardScreen {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            CardScreen::Preparing { .. } => "Preparing",
            CardScreen::WaitingForPhone => "WaitingForPhone",
            CardScreen::DeviceHalf { .. } => "DeviceHalf",
            CardScreen::SelfCheck { .. } => "SelfCheck",
            CardScreen::PhoneHalfNext => "PhoneHalfNext",
            CardScreen::Recheck { .. } => "Recheck",
            CardScreen::RecheckDone => "RecheckDone",
            CardScreen::WaitingForPhoneHalf => "WaitingForPhoneHalf",
            CardScreen::RestoreChecking => "RestoreChecking",
            CardScreen::CardFinishedWaiting => "CardFinishedWaiting",
            CardScreen::DeviceRoundIntro => "DeviceRoundIntro",
            CardScreen::DeviceRoundAnswer { .. } => "DeviceRoundAnswer",
            CardScreen::DeviceRoundMismatch { .. } => "DeviceRoundMismatch",
            CardScreen::DeviceRoundFailed => "DeviceRoundFailed",
            CardScreen::DeviceRoundNextStep => "DeviceRoundNextStep",
            CardScreen::Gate { .. } => "Gate",
            CardScreen::Verdict { .. } => "Verdict",
            CardScreen::CardDone => "CardDone",
            CardScreen::PhoneRoundFailed => "PhoneRoundFailed",
            CardScreen::Stopped => "Stopped",
            CardScreen::StoppedNextStep => "StoppedNextStep",
            CardScreen::NextStep => "NextStep",
            CardScreen::RedisplayRefused => "RedisplayRefused",
            CardScreen::Refused { .. } => "Refused",
            CardScreen::DeviceError => "DeviceError",
        };
        f.write_str(name)
    }
}

/// What the user did on a card screen.
#[derive(Clone, Debug)]
pub enum CardUserAction {
    /// The hold "I've written words 1–32".
    DeviceHalfWritten,
    /// The last item of the self-check quiz was picked right.
    SelfCheckPassed,
    /// The last item of a later check's quiz was picked right.
    RecheckPassed,
    DeviceRoundIntroDone,
    /// An index into the options; `None` is "None of these".
    DeviceRoundPick {
        display_id: u32,
        pick: Option<u8>,
    },
    DeviceRoundMismatchAck,
    VerdictAck,
    FailAck,
}

pub(crate) fn to_coordinator(message: DeviceCard) -> DeviceSend {
    DeviceSend::ToCoordinator(Box::new(DeviceToCoordinatorMessage::Card(message)))
}

pub(crate) fn to_user(message: ToUserCard) -> DeviceSend {
    DeviceSend::ToUser(Box::new(DeviceToUserMessage::Card(Box::new(message))))
}

pub(crate) fn screen(screen: CardScreen) -> DeviceSend {
    to_user(ToUserCard::Screen(screen))
}

/// A screen in this session: it records which screen family is up.
fn show(session: &mut CardSession<'_>, shown: CardScreen) -> DeviceSend {
    session.ui_at = match shown {
        CardScreen::Gate { .. } => CardUiStep::Gate,
        CardScreen::Preparing { .. } => CardUiStep::Preparing,
        CardScreen::DeviceHalf { .. } => CardUiStep::DeviceHalf,
        CardScreen::SelfCheck { .. } => CardUiStep::SelfCheck,
        CardScreen::PhoneHalfNext => CardUiStep::PhoneHalf,
        CardScreen::Recheck { .. } => CardUiStep::Recheck,
        CardScreen::DeviceRoundIntro => CardUiStep::DeviceRound {
            screen: RoundScreen::Intro,
        },
        CardScreen::DeviceRoundAnswer { .. } => CardUiStep::DeviceRound {
            screen: RoundScreen::Answer,
        },
        CardScreen::DeviceRoundMismatch { .. } => CardUiStep::DeviceRound {
            screen: RoundScreen::Mismatch,
        },
        _ => CardUiStep::Other,
    };
    screen(shown)
}

/// The verdict screen for a FAILED record.
fn failed_screen(reason: VerdictReason) -> CardScreen {
    match reason {
        VerdictReason::StoppedBeforeN => CardScreen::Stopped,
        VerdictReason::DeviceRoundFailed => CardScreen::DeviceRoundFailed,
        VerdictReason::_LifetimeCap
        | VerdictReason::PhoneRoundPassed
        | VerdictReason::PhoneRoundFailed => CardScreen::PhoneRoundFailed,
    }
}

pub(crate) fn refused(reason: CardRefusal) -> Vec<DeviceSend> {
    vec![to_coordinator(DeviceCard::Refused { reason })]
}

/// The three opening frames of a finished opening: the word and its 77 Q's, then its carry
/// proofs in two parts.
fn opening_parts(opening: &proof::Opening) -> Result<Vec<DeviceSend>, CardRefusal> {
    let (j, y, carries) = opening.carries_part();
    let mut parts = vec![to_coordinator(DeviceCard::OpeningPart(
        CardOpeningPart::Carries {
            j,
            y,
            carries: carries.to_vec(),
        },
    ))];
    for part in 1..=2 {
        let (first, proofs) = opening.proofs_part(part).ok_or(CardRefusal::Busy)?;
        parts.push(to_coordinator(DeviceCard::OpeningPart(
            CardOpeningPart::Proofs {
                j,
                first,
                proofs: proofs.to_vec(),
            },
        )));
    }
    Ok(parts)
}

impl<S: NonceStreamSlot + core::fmt::Debug> FrostSigner<S> {
    pub fn card_record(&self, share_image: &ShareImage) -> Option<&CardRecord> {
        self.card.records.get(share_image)
    }

    /// The line under the key's name on the home screen (spec §0.7.5, design §7.5): "Backup not
    /// checked" for a card share in any state but PASSED, "Backup failed: don't use" once FAILED.
    /// `None` for a share made without the card option, a restored share, a share with no record
    /// and a card in another format.
    pub fn card_home_line(&self, share_image: ShareImage) -> Option<HomeLine> {
        let rec = self.card_record(&share_image)?;
        match (rec.origin, &rec.state) {
            (_, CardState::OtherFormat) => None,
            (ShareOrigin::Keygen { card: true }, CardState::Passed) => None,
            (ShareOrigin::Keygen { card: true }, CardState::Failed(_)) => Some(HomeLine::Failed),
            (ShareOrigin::Keygen { card: true }, _) => Some(HomeLine::NotChecked),
            _ => None,
        }
    }

    /// A refusal the user sees too: the reply plus its screen.
    pub fn card_refusal(&self, reason: CardRefusal) -> Vec<DeviceSend> {
        vec![
            to_coordinator(DeviceCard::Refused { reason }),
            screen(CardScreen::Refused { reason }),
        ]
    }

    /// Stages a card mutation that passes the transition rule. A job is armed or an output
    /// built only after `Ok`.
    pub(crate) fn card_mutate(&mut self, m: CardMutation) -> Result<(), CardRefusal> {
        self.card.check(&m)?;
        self.mutate(Mutation::Card(m));
        Ok(())
    }

    fn card_state(&self, share_image: &ShareImage) -> Option<CardState> {
        self.card_record(share_image).map(|rec| rec.state.clone())
    }

    /// Runs the gate and builds the output. Nothing a card flow outputs is built elsewhere,
    /// except the screens with words: those are drawn by the caller, and shown only once the
    /// gate passed.
    fn card_emit(
        &self,
        share_image: ShareImage,
        out: CardOutput,
        session: Option<&CardSession<'_>>,
    ) -> Result<Vec<DeviceSend>, CardRefusal> {
        let mut ops = Ops::default();
        let rec = self.card_record(&share_image);
        let facts = session.map(|session| session.facts());
        card_gate(rec, facts.as_ref(), &out, &mut ops)?;
        // The gate passed, so there's a record and a session wherever one is needed.
        let rec = rec.ok_or(CardRefusal::NotKeygenOrigin);
        let session = session.ok_or(CardRefusal::NoSession);
        Ok(match out {
            CardOutput::SessionOpen { .. }
            | CardOutput::LegacyDisplay
            | CardOutput::DeviceHalfScreen
            | CardOutput::RecheckScreen
            | CardOutput::DeviceRoundScreen { .. } => vec![],
            CardOutput::Status => {
                let status = rec?.status(share_image).ok_or(CardRefusal::Busy)?;
                vec![to_coordinator(DeviceCard::Status(Box::new(status)))]
            }
            CardOutput::CommitPoints { from, count } => {
                let seal = session?.seal.as_ref().ok_or(CardRefusal::Busy)?;
                let range = from as usize..from as usize + count as usize;
                vec![to_coordinator(DeviceCard::CommitPoints {
                    from,
                    points: seal.commitments[range].to_vec(),
                })]
            }
            CardOutput::BitProofs { from, count } => {
                let proofs = &session?.proofs;
                let proofs = (from..from + count as u16)
                    .map(|v| proofs.get(v).copied().ok_or(CardRefusal::Busy))
                    .collect::<Result<Vec<_>, _>>()?;
                vec![to_coordinator(DeviceCard::BitProofs { from, proofs })]
            }
            CardOutput::CheckDone => vec![to_coordinator(DeviceCard::CheckDone)],
            CardOutput::GateShown => vec![to_coordinator(DeviceCard::GateShown)],
            CardOutput::DeviceRoundVerdict => vec![to_coordinator(DeviceCard::DeviceRound {
                pass: rec?.device_round_passed(),
            })],
            CardOutput::Coin { .. } => vec![to_coordinator(DeviceCard::OpenAccepted {
                coin: session?.secrets().ok_or(CardRefusal::Busy)?.coin(),
            })],
            CardOutput::Opening { j, .. } => {
                opening_parts(session?.ready_opening(j).ok_or(CardRefusal::Busy)?)?
            }
            CardOutput::PhoneHalf => {
                let words = session?.words().ok_or(CardRefusal::Busy)?;
                vec![to_coordinator(DeviceCard::PhoneHalf(CardWords32(
                    words[PHONE_HALF].try_into().expect("32 words"),
                )))]
            }
            CardOutput::Verdict { .. } => vec![to_coordinator(DeviceCard::Verdict {
                passed: rec?.state == CardState::Passed,
            })],
            CardOutput::VerdictScreen { screen: s } => vec![screen(s)],
            CardOutput::RecheckWords { words_hash } => {
                let rec = rec?;
                let words = session?.words().ok_or(CardRefusal::Busy)?;
                let found = recheck_match(rec, words, words_hash).ok_or(CardRefusal::NoRecord)?;
                let phone_half = CardWords32(words[PHONE_HALF].try_into().expect("32 words"));
                let d_words = match (found, rec.d) {
                    (RecheckMatch::H37, Some(d)) => Some(CardWords5(d.map(|j| words[j as usize]))),
                    _ => None,
                };
                vec![to_coordinator(DeviceCard::RecheckWords {
                    phone_half,
                    d_words,
                })]
            }
        })
    }

    /// [`card_emit`](Self::card_emit), with a refusal as the reply.
    fn card_emit_or_refuse(
        &self,
        share_image: ShareImage,
        out: CardOutput,
        session: Option<&CardSession<'_>>,
    ) -> Vec<DeviceSend> {
        self.card_emit(share_image, out, session)
            .unwrap_or_else(refused)
    }

    /// A verdict screen that went through the gate, or the refusal's screen. A refused screen
    /// is the user's only: it never becomes a second frame to the phone.
    fn card_verdict_screen(&self, share_image: ShareImage, shown: CardScreen) -> Vec<DeviceSend> {
        self.card_emit(
            share_image,
            CardOutput::VerdictScreen { screen: shown },
            None,
        )
        .unwrap_or_else(|reason| vec![screen(CardScreen::Refused { reason })])
    }

    /// Handles a card message. It's infallible and takes no RNG: draws come from the session's
    /// child CSPRNG, and jobs draw theirs in [`CardSession::step`]. `session` is the firmware's
    /// open session, if any.
    #[inline(never)]
    pub fn recv_card_message(
        &mut self,
        message: CoordinatorCard,
        session: Option<&mut CardSession<'_>>,
    ) -> Vec<DeviceSend> {
        match message {
            CoordinatorCard::Begin(begin) => self.card_begin(*begin, session),
            CoordinatorCard::PullCommit { from, count } => match session {
                Some(session) => self.card_pull_commit(from, count, session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::WriteStart { h_c } => match session {
                Some(session) => self.card_write_start(h_c, session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::PullProofs { from, count } => match session {
                Some(session) => self.card_pull_proofs(from, count, session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::OpenStart { n } => match session {
                Some(session) => self.card_open_start(n, session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::OpenBox { n, j } => match session {
                Some(session) => self.card_open_box(n, j, session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::BoxAnswered { .. } => vec![],
            CoordinatorCard::PhoneRound { n, pass } => self.card_phone_round(n, pass, session),
            CoordinatorCard::Stop => self.card_stop(session),
            CoordinatorCard::FinalBegin => match session {
                Some(session) => self.card_final_begin(session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::Recheck { words_hash } => self.card_recheck(words_hash, session),
            // Retired with format C′ (append-only wire): there are no phone-half openings and no
            // prepare stage.
            CoordinatorCard::OpenPhoneWord { .. } | CoordinatorCard::Prepare { .. } => {
                refused(CardRefusal::Unsupported)
            }
            CoordinatorCard::PullPhoneHalf { h_c } => match session {
                Some(session) => self.card_pull_phone_half(h_c, session),
                None => refused(CardRefusal::NoSession),
            },
            CoordinatorCard::DeviceHalfWritten { h_c } => match session {
                Some(session) => self.card_device_half_written(h_c, session),
                None => refused(CardRefusal::NoSession),
            },
        }
    }

    /// The share a `Begin` names, as the device's own record gives it, and what a session for
    /// it needs.
    fn card_session_start(&self, begin: &CardBegin) -> Result<CardSessionStart, CardRefusal> {
        // The same lookup as DisplayBackup: the request only selects the share.
        let access_structure_ref = AccessStructureRef::from_root_shared_key(&begin.root_shared_key);
        let encrypted = self
            .get_encrypted_share(access_structure_ref, begin.share_index)
            .filter(|encrypted| {
                encrypted.share_image == begin.root_shared_key.share_image(begin.share_index)
            })
            .ok_or(CardRefusal::BadRequest)?;
        let share_image = encrypted.share_image;
        let rec = self
            .card_record(&share_image)
            .ok_or(CardRefusal::NotKeygenOrigin)?;
        Ok(CardSessionStart {
            access_structure_ref,
            share_index: begin.share_index,
            share_image,
            encrypted_secret_share: encrypted.ciphertext,
            coord_contrib: CoordShareDecryptionContrib::for_master_share(
                self.device_id(),
                begin.share_index,
                &begin.root_shared_key,
            ),
            root_shared_key: begin.root_shared_key.clone(),
            sealed: rec.sealed,
            state: rec.state.clone(),
        })
    }

    fn card_begin(
        &mut self,
        begin: CardBegin,
        session: Option<&mut CardSession<'_>>,
    ) -> Vec<DeviceSend> {
        let start = match self.card_session_start(&begin) {
            Ok(start) => start,
            Err(reason) => return self.card_refusal(reason),
        };
        let share_image = start.share_image;
        let open = CardOutput::SessionOpen {
            format_id: begin.format_id,
            proof_version: begin.proof_version,
            index: begin.share_index,
            threshold: begin.root_shared_key.threshold(),
        };
        if let Err(reason) = self.card_emit(share_image, open, None) {
            return self.card_refusal(reason);
        }
        let rec = self
            .card_record(&share_image)
            .expect("the gate passed, so there's a record");
        let sealed = rec.sealed;
        let state = rec.state.clone();
        // A later check opens its session only on `Recheck`.
        self.card.tmp_recheck_begin = None;
        if state.is_terminal() {
            if state == CardState::Passed {
                self.card.tmp_recheck_begin = Some(RecheckBegin { share_image, begin });
            }
            return self.card_emit_or_refuse(share_image, CardOutput::Status, None);
        }
        let mut out = vec![];

        match session {
            // The phone began again while this session is live: keep it, so nothing is
            // recomputed and no run is spent. Its screen is up, so nothing is redrawn (design
            // S10). The phone's session restarted, so an opening it asked for leaves only on its
            // new request; a ready one is kept.
            Some(session) if session.share_image() == share_image => {
                session.asked = None;
                if sealed.is_some() {
                    out.extend(self.card_emit_or_refuse(
                        share_image,
                        CardOutput::Status,
                        Some(session),
                    ));
                }
                if state == CardState::Checking {
                    out.extend(self.card_checking_screen(session));
                } else if !session.write_started || session.ui_at == CardUiStep::Preparing {
                    out.extend(self.card_resume_screen(session));
                }
                return out;
            }
            // Another share's session: the firmware drops it once it opens this one, and until
            // then its job must not finish and send for the old share.
            Some(session) => session.fail(),
            None => {}
        }
        // A verdict screen up from an earlier session isn't this one's.
        self.card.tmp_verdict_shown = None;
        out.push(to_user(ToUserCard::OpenSession(Box::new(start))));
        if state.before_checking() {
            // The first time the seal follows the encode; later the encode alone.
            let task = if state == CardState::Unsealed {
                BusyTask::Seal
            } else {
                BusyTask::Card
            };
            out.push(screen(CardScreen::Preparing { task }));
        }
        if sealed.is_some() {
            out.extend(self.card_emit_or_refuse(share_image, CardOutput::Status, None));
        }
        out
    }

    /// Arms the session's first job, the encode. It stages no mutation, as the firmware calls
    /// it while draining the outbox.
    #[inline(never)]
    pub fn card_session_opened(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        session.arm_encode();
        if self
            .card_record(&session.share_image())
            .is_some_and(|rec| rec.state.before_checking())
        {
            // `Begin` showed "Preparing": the encode's event shows what's next.
            session.ui_at = CardUiStep::Preparing;
        }
        if let Some((share_image, words_hash)) = self.card.tmp_recheck_hash.take() {
            if share_image == session.share_image() {
                session.recheck = Some(words_hash);
            }
        }
        vec![]
    }

    /// `PullCommit`. In SEALED and PROVING, with no seal in the session, it's computed first;
    /// while words 1–32 are written and quizzed the gate refuses rather than start it.
    fn card_pull_commit(
        &mut self,
        from: u16,
        count: u8,
        session: &mut CardSession<'_>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        match self.card_emit(
            share_image,
            CardOutput::CommitPoints { from, count },
            Some(session),
        ) {
            Ok(out) => out,
            Err(CardRefusal::Busy) => {
                session.want_seal();
                refused(CardRefusal::Busy)
            }
            Err(reason) => refused(reason),
        }
    }

    /// `PullProofs`: in PROVING the first one starts the proofs (after the seal, if the session
    /// has none). Before PROVING it's `Busy` and arms nothing.
    fn card_pull_proofs(
        &mut self,
        from: u16,
        count: u8,
        session: &mut CardSession<'_>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        match self.card_emit(
            share_image,
            CardOutput::BitProofs { from, count },
            Some(session),
        ) {
            Ok(out) => out,
            Err(CardRefusal::Busy) => {
                session.prefer_proofs_from(from);
                self.card_want_proofs(session);
                refused(CardRefusal::Busy)
            }
            Err(reason) => refused(reason),
        }
    }

    /// Arms the bit proofs, only in PROVING: the one place they're armed from (spec PK.7,
    /// owner B1). Before PROVING the user may be writing or quizzing words 1–32 on the device,
    /// and the proofs' timing would show when they touched it.
    fn card_want_proofs(&self, session: &mut CardSession<'_>) {
        let proving = self
            .card_record(&session.share_image())
            .is_some_and(|rec| rec.state == CardState::Proving);
        if proving {
            session.want_proofs();
        }
    }

    /// `PullPhoneHalf{h_c}`: words 33–64, from SEALED through CHECKING (build plan A10), once
    /// this session's encode matched the record's words hash, and only for the seal the phone
    /// names: `h_c` is this card's persisted h_C, which the phone pins with these words or has
    /// pinned. It binds P to that seal, not to an authenticated phone: h_C is in every `Status`
    /// (review T5; the audit-secrecy info item waits for the encrypted link, design §8).
    fn card_pull_phone_half(
        &mut self,
        h_c: [u8; 32],
        session: &mut CardSession<'_>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let sealed = self.card_record(&share_image).and_then(|rec| rec.sealed);
        if sealed.is_some_and(|sealed| sealed.h_c != h_c) {
            return refused(CardRefusal::BadRequest);
        }
        self.card_emit_or_refuse(share_image, CardOutput::PhoneHalf, Some(session))
    }

    /// `WriteStart{h_c}`: SEALED → WRITING, persisted before the first word is drawn; later
    /// states up to PROVING show their screen. A repeat in the session redraws nothing (design
    /// S10).
    fn card_write_start(
        &mut self,
        h_c: [u8; 32],
        session: &mut CardSession<'_>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(rec) = self.card_record(&share_image) else {
            return refused(CardRefusal::NotKeygenOrigin);
        };
        if rec.state == CardState::OtherFormat {
            return self.card_refusal(CardRefusal::Unsupported);
        }
        if rec.sealed.map(|sealed| sealed.h_c) != Some(h_c) {
            return refused(CardRefusal::BadRequest);
        }
        match rec.state {
            CardState::Sealed => {
                if let Err(reason) =
                    self.card_mutate(CardMutation::SplitWriteStarted { share_image })
                {
                    return refused(reason);
                }
                // A seal a pull in this session armed doesn't run on into WRITING, whatever the
                // phone sends (an honest one pins only after the seal).
                session.stop_seal_job();
            }
            _ if rec.state.resumes_writing() => {}
            _ => {
                let mut out = refused(CardRefusal::WrongState);
                out.push(screen(CardScreen::RedisplayRefused));
                return out;
            }
        }
        if core::mem::replace(&mut session.write_started, true) {
            return vec![];
        }
        // Before the encode is done its event draws the screen.
        if session.words().is_none() {
            return vec![];
        }
        self.card_resume_screen(session)
    }

    /// `DeviceHalfWritten{h_c}` (owner B1): the user confirmed words 1–32 on the phone. Only in
    /// CARD_CHECKED, where the device's own quiz on them passed, and only with the persisted
    /// h_C: CARD_CHECKED → PROVING is persisted, then the bit proofs are armed (after the seal,
    /// if the session has none) and the proving screen shows. Again in PROVING it changes
    /// nothing and restarts nothing. Anywhere else it's refused and nothing is remembered, so
    /// neither side alone can start the proofs.
    fn card_device_half_written(
        &mut self,
        h_c: [u8; 32],
        session: &mut CardSession<'_>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(rec) = self.card_record(&share_image) else {
            return refused(CardRefusal::NotKeygenOrigin);
        };
        if rec.state == CardState::OtherFormat {
            return refused(CardRefusal::Unsupported);
        }
        if rec.state.is_terminal() {
            return refused(CardRefusal::Closed);
        }
        if let Err(reason) = rec.origin_refusal() {
            return refused(reason);
        }
        if rec.sealed.map(|sealed| sealed.h_c) != Some(h_c) {
            return refused(CardRefusal::BadRequest);
        }
        match rec.state {
            CardState::CardChecked => {
                if let Err(reason) = self.card_mutate(CardMutation::ProvingStarted { share_image })
                {
                    return refused(reason);
                }
                self.card_want_proofs(session);
                // Before the encode is done its event draws the screen.
                if session.words().is_none() {
                    session.ui_at = CardUiStep::Preparing;
                    return vec![];
                }
                self.card_resume_screen(session)
            }
            CardState::Proving => vec![],
            _ => refused(CardRefusal::WrongState),
        }
    }

    /// `Recheck{words_hash}` (design §3.9): a later check of a PASSED card. The device opens a
    /// session for the share the last `Begin` named; once its encode is done it sends the phone
    /// half only to a phone whose hash matches, and runs its own quiz either way.
    fn card_recheck(
        &mut self,
        words_hash: Option<[u8; 32]>,
        session: Option<&mut CardSession<'_>>,
    ) -> Vec<DeviceSend> {
        let Some(RecheckBegin { share_image, begin }) = self.card.tmp_recheck_begin.clone() else {
            return refused(CardRefusal::NoSession);
        };
        match self.card_state(&share_image) {
            Some(CardState::Passed) => {}
            Some(CardState::OtherFormat) => return refused(CardRefusal::Unsupported),
            _ => return refused(CardRefusal::WrongState),
        }
        match session {
            Some(session) if session.share_image() == share_image => {
                session.recheck = Some(words_hash);
                if session.words().is_some() {
                    return self.card_recheck_ready(session);
                }
                vec![]
            }
            other => {
                if let Some(session) = other {
                    session.fail();
                }
                let start = match self.card_session_start(&begin) {
                    Ok(start) => start,
                    Err(reason) => return refused(reason),
                };
                self.card.tmp_recheck_hash = Some((share_image, words_hash));
                self.card.tmp_verdict_shown = None;
                vec![
                    to_user(ToUserCard::OpenSession(Box::new(start))),
                    screen(CardScreen::Preparing {
                        task: BusyTask::Card,
                    }),
                ]
            }
        }
    }

    /// A later check, once the session's card is built: the words for the phone (or
    /// `NoRecord`), then the device's own quiz on words 1–32 and E.
    fn card_recheck_ready(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(words_hash) = session.recheck.take() else {
            return vec![];
        };
        let mut out = self.card_emit_or_refuse(
            share_image,
            CardOutput::RecheckWords {
                words_hash: words_hash.as_ref(),
            },
            Some(session),
        );
        if let Err(reason) = self.card_emit(share_image, CardOutput::RecheckScreen, Some(session)) {
            out.push(show(session, CardScreen::Refused { reason }));
            return out;
        }
        let e = self
            .card_record(&share_image)
            .and_then(|rec| rec.device_round)
            .map(|dr| dr.e)
            .unwrap_or_default();
        let items = session.quiz_items(DEVICE_HALF.map(|j| j as u8).chain(e));
        let seed = session.self_check_seed();
        out.push(show(session, CardScreen::Recheck { items, seed }));
        out
    }

    /// `FinalBegin` (B2, design §5.3): the device round starts in PROVING, after the phone's
    /// [Continue] (owner B1), resumes in a new session, and is answered with its verdict once
    /// it's over.
    fn card_final_begin(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(rec) = self.card_record(&share_image) else {
            return refused(CardRefusal::NotKeygenOrigin);
        };
        if rec.state == CardState::OtherFormat {
            return refused(CardRefusal::Unsupported);
        }
        if let Err(reason) = rec.origin_refusal() {
            return refused(reason);
        }
        let pending = rec
            .device_round
            .is_some_and(|dr| dr.result == DrResult::Pending);
        // Owner B8: the device round needs N taken first, so a device round that fails after
        // the chip learned D is the device's failure too.
        if rec.state == CardState::Proving && rec.n_tag.is_none() {
            return refused(CardRefusal::WrongState);
        }
        // It's a notify, so it isn't re-sent: one that comes during the encode waits for it.
        if session.words().is_none()
            && matches!(rec.state, CardState::Proving | CardState::Checking)
        {
            session.final_begin_deferred = true;
            return vec![];
        }
        match rec.state {
            CardState::Proving => {
                // The seal and the proofs aren't needed again: they go before E is drawn. A
                // running opening carries on.
                session.drop_seal_and_proofs();
                session.clear_displays();
                let mut e = session.draw_round_boxes();
                let started = self.card_mutate(CardMutation::DeviceRoundStarted { share_image, e });
                wipe(&mut e, [0; CHECK_BOXES]);
                if let Err(reason) = started {
                    return refused(reason);
                }
                // Owner B8: an opening the proofs held back (a resume) may start now.
                self.card_arm_next_opening(session);
                vec![show(session, CardScreen::DeviceRoundIntro)]
            }
            // The round is on screen in this session: a repeat redraws nothing (design S10).
            CardState::Checking
                if pending && matches!(session.ui_at, CardUiStep::DeviceRound { .. }) =>
            {
                vec![]
            }
            // A new session: the persisted k and attempt, with a fresh option order. A second
            // look already used isn't given again (spec L2.7.10).
            CardState::Checking if pending => self.card_show_answer(session),
            // The round is over: its verdict again, and the screen stays.
            CardState::Checking | CardState::Failed(VerdictReason::DeviceRoundFailed) => {
                self.card_emit_or_refuse(share_image, CardOutput::DeviceRoundVerdict, Some(session))
            }
            CardState::Passed | CardState::Failed(_) => refused(CardRefusal::Closed),
            _ => refused(CardRefusal::WrongState),
        }
    }

    /// The device round's answer screen for E[k] (spec L2.7.7): word j's and two of its six
    /// closest lookalikes, in a fresh order for this display.
    fn card_show_answer(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(dr) = self
            .card_record(&share_image)
            .and_then(|rec| rec.device_round)
        else {
            return vec![];
        };
        let Some(&j) = dr.e.get(dr.k as usize) else {
            return vec![];
        };
        session.clear_displays();
        if let Err(reason) = self.card_emit(
            share_image,
            CardOutput::DeviceRoundScreen { j },
            Some(session),
        ) {
            return vec![show(session, CardScreen::Refused { reason })];
        }
        let display_id = session.next_display();
        let Some(options) = session.round_options(j, dr.k, dr.attempt, display_id) else {
            return vec![];
        };
        let shown = CardScreen::DeviceRoundAnswer {
            j,
            k: dr.k,
            options: options.clone(),
            display_id,
        };
        session.dr = Some(DrScreen {
            display_id,
            j,
            k: dr.k,
            options,
        });
        vec![show(session, shown)]
    }

    /// A pick on the answer screen, after its flash (design §5.5.4). It's scored against the
    /// options it was made on, and the step is persisted before the next screen. Nothing goes
    /// to the phone until the round's verdict (spec L2.7.11).
    fn card_round_pick(
        &mut self,
        session: &mut CardSession<'_>,
        display_id: u32,
        pick: Option<u8>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(dr) = self
            .card_record(&share_image)
            .and_then(|rec| rec.device_round)
            .filter(|dr| dr.result == DrResult::Pending)
        else {
            return vec![];
        };
        let Some(shown) = session
            .dr
            .take_if(|shown| shown.display_id == display_id && shown.k == dr.k)
        else {
            return vec![];
        };
        let ok = session.round_pick_matches(shown.j, &shown.options, pick);
        let j = shown.j;
        drop(shown);
        let last = dr.k as usize + 1 == CHECK_BOXES;
        let staged = match (ok, last) {
            (true, false) => CardMutation::DeviceRoundStep {
                share_image,
                k: dr.k + 1,
                attempt: 0,
            },
            (true, true) => CardMutation::DeviceRoundResult {
                share_image,
                pass: true,
            },
            // One second look at the same word, persisted before it's shown.
            (false, _) if dr.attempt == 0 => CardMutation::DeviceRoundStep {
                share_image,
                k: dr.k,
                attempt: 1,
            },
            (false, _) => CardMutation::DeviceRoundResult {
                share_image,
                pass: false,
            },
        };
        if let Err(reason) = self.card_mutate(staged) {
            return vec![show(session, CardScreen::Refused { reason })];
        }
        match (ok, last) {
            (true, false) => self.card_show_answer(session),
            // Owner B8: N is taken already, so the gate is up straight away and the phone told.
            (true, true) => {
                let mut out = self.card_emit_or_refuse(
                    share_image,
                    CardOutput::DeviceRoundVerdict,
                    Some(session),
                );
                out.push(self.card_show_gate(session));
                out.extend(self.card_gate_shown(session));
                self.card_arm_next_opening(session);
                out
            }
            (false, _) if dr.attempt == 0 => {
                vec![show(session, CardScreen::DeviceRoundMismatch { j })]
            }
            // FAILED(DeviceRoundFailed) is persisted: N is never taken, no opening computed.
            (false, _) => {
                session.finish_check();
                self.card.tmp_verdict_shown = Some(share_image);
                let mut out = self.card_emit_or_refuse(
                    share_image,
                    CardOutput::DeviceRoundVerdict,
                    Some(session),
                );
                out.extend(self.card_verdict_screen(share_image, CardScreen::DeviceRoundFailed));
                out
            }
        }
    }

    /// B4, owner B8: the phone's N, right after "I've written words 1–32". It's taken only in
    /// PROVING (the device's quiz passed and the phone's `DeviceHalfWritten` came): n_tag and D
    /// are persisted before the coin leaves, and no other N is ever taken. The openings of D are
    /// computed after the bit proofs, while words 33–64 are written. Again with the same N
    /// (PROVING, or CHECKING up to the verdict) it resumes: the coin again, and at the gate the
    /// gate screen.
    fn card_open_start(&mut self, n: [u8; 32], session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let mut ops = Ops::default();
        let Some(rec) = self.card_record(&share_image) else {
            return refused(CardRefusal::NotKeygenOrigin);
        };
        let first = rec.state == CardState::Proving && rec.n_tag.is_none();
        let resume =
            rec.n_tag.is_some() && n_phase(rec.state.clone(), rec.device_round.map(|dr| dr.result));
        let (Some(sealed), true, Some(coin)) = (
            rec.sealed,
            first || resume,
            session.secrets().map(|secrets| secrets.coin()),
        ) else {
            // CARD_CHECKED: the phone's confirm of words 1–32 was lost on its way, and comes
            // again on the `Busy` (as for a pull of proofs).
            let confirm_lost = rec.state == CardState::CardChecked;
            if ((first || resume) && session.secrets().is_none()) || confirm_lost {
                return refused(CardRefusal::Busy);
            }
            // Before PROVING, another N, and terminal states (Closed), from the gate.
            if rec.state.is_terminal() {
                return self.card_emit_or_refuse(
                    share_image,
                    CardOutput::Coin { n: &n },
                    Some(session),
                );
            }
            return refused(match rec.n_tag {
                Some(_) => CardRefusal::AlreadyUsed,
                None => CardRefusal::WrongState,
            });
        };
        // The coin must be the one the seal committed to, or the phone would blame the card.
        if !proof::coin_matches(&sealed.coin_commit, &coin, &mut ops) {
            return self.card_job_failed(session);
        }
        let d = proof::derive_boxes(&sealed.h_c, &n, &coin, &mut ops);

        let mut out = if first {
            if let Err(reason) = self.card_mutate(CardMutation::PhoneRound {
                share_image,
                n_tag: proof::n_tag(&n, &mut ops),
                d,
            }) {
                return refused(reason);
            }
            session.n = Some(n);
            self.card_emit_or_refuse(share_image, CardOutput::Coin { n: &n }, Some(session))
        } else {
            let out = match self.card_emit(share_image, CardOutput::Coin { n: &n }, Some(session)) {
                Ok(out) => out,
                Err(reason) => return refused(reason),
            };
            if rec.d != Some(d) {
                return self.card_job_failed(session);
            }
            session.n = Some(n);
            out
        };
        let rec = self
            .card_record(&share_image)
            .expect("the record was there above");
        let round_passed = rec.device_round_passed();
        if rec.state != CardState::Checking {
            // PROVING: the openings follow the bit proofs (or start now if they're done).
            self.card_arm_next_opening(session);
            return out;
        }
        // CHECKING: the seal and the proofs aren't needed again.
        session.enter_checking(n);
        self.card_arm_next_opening(session);
        if !round_passed {
            return out;
        }
        // The gate is up after the round, or comes up now in a new session; either way the
        // phone is told, and the gate is updated in place.
        if session.ui_at == CardUiStep::Gate {
            out.extend(self.card_gate_shown(session));
            out.push(self.card_gate_screen(session));
        } else {
            out.push(self.card_show_gate(session));
            out.extend(self.card_gate_shown(session));
        }
        out
    }

    /// Owner B8: the next opening of D that isn't released or held, armed once nothing else
    /// needs the job: in PROVING only after the bit proofs are done. Never more than one
    /// computed and unsent (spec L1.5.3.2).
    fn card_arm_next_opening(&mut self, session: &mut CardSession<'_>) {
        let share_image = session.share_image();
        let Some(rec) = self.card_record(&share_image) else {
            return;
        };
        if session.n.is_none() || rec.n_tag.is_none() || !session.ready.is_empty() {
            return;
        }
        if rec.state == CardState::Proving && !session.proofs_complete() {
            return;
        }
        let next = session
            .asked
            .filter(|&j| rec.d_position(j).is_some())
            .or_else(|| {
                rec.d.and_then(|d| {
                    (0..CHECK_BOXES)
                        .find(|&k| {
                            rec.released & (1 << k) == 0 && rec.runs[d[k] as usize] < MAX_RUNS
                        })
                        .map(|k| d[k])
                })
            });
        if let Some(next) = next {
            self.card_arm_opening(session, next);
        }
    }

    fn card_open_box(
        &mut self,
        n: [u8; 32],
        j: u8,
        session: &mut CardSession<'_>,
    ) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let mut ops = Ops::default();
        let rec = self.card_record(&share_image);
        let facts = session.facts();
        match card_gate(
            rec,
            Some(&facts),
            &CardOutput::Opening { n: &n, j },
            &mut ops,
        ) {
            // Only the gate screen isn't up yet: the request is kept, and reaching the gate
            // releases its opening.
            Ok(()) | Err(CardRefusal::Busy) => {}
            Err(reason) => return refused(reason),
        }
        session.n = Some(n);
        session.asked = Some(j);
        if session.ready_opening(j).is_some() {
            return self.card_release(session, j).unwrap_or_else(refused);
        }
        // Computing j, or another box whose result is kept and j starts next.
        if session.opening_box().is_some() {
            return refused(CardRefusal::Busy);
        }
        // Owner B8: in PROVING the bit proofs come first; their end arms the asked box.
        let proving = self
            .card_record(&share_image)
            .is_some_and(|rec| rec.state == CardState::Proving);
        if proving && !session.proofs_complete() {
            return refused(CardRefusal::Busy);
        }
        match self.card_start_opening(session, j) {
            Ok(()) => refused(CardRefusal::Busy),
            Err(reason) => {
                session.asked = None;
                refused(reason)
            }
        }
    }

    /// Persists a run of word j, then arms its opening; the run read after the apply is σ's
    /// hedge index (spec L1.5.3.2).
    fn card_start_opening(
        &mut self,
        session: &mut CardSession<'_>,
        j: u8,
    ) -> Result<(), CardRefusal> {
        let share_image = session.share_image();
        self.card_mutate(CardMutation::OpenRun { share_image, j })?;
        let run = self
            .card_record(&share_image)
            .map(|rec| rec.runs[j as usize])
            .ok_or(CardRefusal::NotKeygenOrigin)?;
        session.arm_open(j, run);
        Ok(())
    }

    /// [`card_start_opening`](Self::card_start_opening), when nothing else is computing.
    fn card_arm_opening(&mut self, session: &mut CardSession<'_>, j: u8) {
        if session.opening_box().is_none() {
            let _ = self.card_start_opening(session, j);
        }
    }

    /// Releases the finished opening of box j of D, only while the gate screen is up: its three
    /// parts pass the gate first, so `Released` (the dots) is persisted only for an opening
    /// that leaves. Then it arms the next box of D that isn't released, so its opening is ready
    /// when the phone asks.
    fn card_release(
        &mut self,
        session: &mut CardSession<'_>,
        j: u8,
    ) -> Result<Vec<DeviceSend>, CardRefusal> {
        let share_image = session.share_image();
        let n = session.n.ok_or(CardRefusal::NoSession)?;
        let mut out =
            self.card_emit(share_image, CardOutput::Opening { n: &n, j }, Some(session))?;
        self.card_mutate(CardMutation::Released { share_image, j })?;
        session.take_ready(j);
        session.asked = None;
        // One box ahead at most, and only while no other opening is held (spec L1.5.3.2). A
        // kept opening of a box already released is dropped: it would be a second.
        if let Some(rec) = self.card_record(&share_image) {
            let released = |j: u8| {
                rec.d_position(j)
                    .is_some_and(|k| rec.released & (1 << k) != 0)
            };
            session.ready.retain(|opening| !released(opening.j));
            let ahead = !session.ready.is_empty();
            let next = rec.d.filter(|_| !ahead).and_then(|d| {
                (0..CHECK_BOXES)
                    .find(|&k| rec.released & (1 << k) == 0 && rec.runs[d[k] as usize] < MAX_RUNS)
                    .map(|k| d[k])
            });
            let proving = rec.state == CardState::Proving && !session.proofs_complete();
            if let (Some(next), false) = (next, proving) {
                self.card_arm_opening(session, next);
            }
        }
        if session.ui_at == CardUiStep::Gate {
            out.push(self.card_gate_screen(session));
        }
        Ok(out)
    }

    /// The final check on a re-attach to a live session: at the gate the phone is told again
    /// that it's up. A device round this session hasn't shown yet waits for the phone, whose
    /// `FinalBegin` resumes it (addendum §2.1). Any other screen is left as it is.
    fn card_checking_screen(&self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let round_pending = self
            .card_record(&session.share_image())
            .and_then(|rec| rec.device_round)
            .is_some_and(|dr| dr.result == DrResult::Pending);
        match session.ui_at {
            CardUiStep::Gate => {
                let mut out = self.card_gate_shown(session);
                out.push(self.card_gate_screen(session));
                out
            }
            CardUiStep::Other | CardUiStep::Preparing if round_pending => {
                vec![show(session, CardScreen::WaitingForPhone)]
            }
            _ => vec![],
        }
    }

    /// `GateShown`, while the gate screen is up.
    fn card_gate_shown(&self, session: &CardSession<'_>) -> Vec<DeviceSend> {
        self.card_emit(session.share_image(), CardOutput::GateShown, Some(session))
            .unwrap_or_default()
    }

    fn card_gate_screen(&self, session: &CardSession<'_>) -> DeviceSend {
        let dots = self
            .card_record(&session.share_image())
            .map(CardRecord::dots)
            .unwrap_or(0);
        screen(CardScreen::Gate {
            dots,
            preparing: session.opening_box().is_some(),
        })
    }

    /// The gate screen, as the screen now up.
    fn card_show_gate(&self, session: &mut CardSession<'_>) -> DeviceSend {
        session.ui_at = CardUiStep::Gate;
        self.card_gate_screen(session)
    }

    fn card_phone_round(
        &mut self,
        n: [u8; 32],
        pass: bool,
        session: Option<&mut CardSession<'_>>,
    ) -> Vec<DeviceSend> {
        let mut ops = Ops::default();
        let n_tag = proof::n_tag(&n, &mut ops);
        // N names the check, so the record needs no session; without one it's found by n_tag.
        let share_image = self
            .card
            .records
            .iter()
            .find(|(_, rec)| rec.n_tag == Some(n_tag))
            .map(|(share_image, _)| *share_image)
            .or(session.as_ref().map(|session| session.share_image()));
        let Some(share_image) = share_image else {
            return refused(CardRefusal::NoSession);
        };
        let Some(rec) = self.card_record(&share_image) else {
            return refused(CardRefusal::NotKeygenOrigin);
        };
        match rec.state {
            // A lost verdict: send it again, and leave the screen. A stop before N has no N.
            CardState::Passed | CardState::Failed(_) => {
                let n = rec.n_tag.map(|_| &n);
                return self.card_emit_or_refuse(share_image, CardOutput::Verdict { n }, None);
            }
            CardState::Checking | CardState::Proving | CardState::CardChecked => {}
            CardState::OtherFormat => return refused(CardRefusal::Unsupported),
            _ => return refused(CardRefusal::WrongState),
        }
        // The phone holds N but its `OpenStart` was lost, and the phone ended the check (an
        // abort): the device never learned D, so this is a stop before N (design §5.3, owner
        // B8).
        if rec.n_tag.is_none() {
            if pass {
                return refused(CardRefusal::WrongState);
            }
            return self.card_stop_before_n(share_image, session);
        }
        if let Err(reason) = n_matches(rec, &n, &mut ops) {
            return refused(reason);
        }
        let reason = if pass {
            VerdictReason::PhoneRoundPassed
        } else {
            VerdictReason::PhoneRoundFailed
        };
        if self
            .card_mutate(CardMutation::Verdict {
                share_image,
                passed: pass,
                reason,
            })
            .is_err()
        {
            return refused(CardRefusal::WrongState);
        }
        let session = session.filter(|session| session.share_image() == share_image);
        if let Some(session) = session {
            session.finish_check();
        }
        self.card.tmp_verdict_shown = Some(share_image);
        // Both keys are in once the phone round passed, so a pass is the verdict itself.
        let shown = if pass {
            CardScreen::Verdict { passed: true }
        } else {
            CardScreen::PhoneRoundFailed
        };
        let mut out =
            self.card_emit_or_refuse(share_image, CardOutput::Verdict { n: Some(&n) }, None);
        out.extend(self.card_verdict_screen(share_image, shown));
        out
    }

    /// The phone's Stop, before N only. Owner B12: once N is taken there's no Stop on either
    /// screen, so a `Stop` then is refused and changes nothing; the only way out is a disconnect.
    fn card_stop(&mut self, session: Option<&mut CardSession<'_>>) -> Vec<DeviceSend> {
        // Stop names no share. Without a session it's the one held share that can take it.
        let share_image = match &session {
            Some(session) => Some(session.share_image()),
            None => {
                let mut stoppable = self.card.records.iter().filter(|(share_image, rec)| {
                    let before_n = match rec.state {
                        CardState::Failed(VerdictReason::StoppedBeforeN) => true,
                        CardState::CardChecked | CardState::Proving => rec.n_tag.is_none(),
                        _ => false,
                    };
                    before_n && self.holds_key_share(**share_image)
                });
                match (stoppable.next(), stoppable.next()) {
                    (Some((share_image, _)), None) => Some(*share_image),
                    _ => None,
                }
            }
        };
        let Some(share_image) = share_image else {
            return refused(CardRefusal::NoSession);
        };
        let n_taken = self
            .card_record(&share_image)
            .is_some_and(|rec| rec.n_tag.is_some());
        match self.card_state(&share_image) {
            Some(CardState::Failed(VerdictReason::StoppedBeforeN)) => {
                self.card_emit_or_refuse(share_image, CardOutput::Verdict { n: None }, None)
            }
            Some(CardState::OtherFormat) => refused(CardRefusal::Unsupported),
            // A verdict is final: the phone gets it again with its `PhoneRound`.
            Some(state) if state.is_terminal() => refused(CardRefusal::Closed),
            // Owner B12: no Stop once N is taken.
            _ if n_taken => refused(CardRefusal::WrongState),
            // Before CARD_CHECKED a check is paused by unplugging, never stopped.
            Some(state)
                if state.before_checking()
                    && !matches!(state, CardState::CardChecked | CardState::Proving) =>
            {
                refused(CardRefusal::WrongState)
            }
            _ => self.card_stop_before_n(share_image, session),
        }
    }

    /// CARD_CHECKED or PROVING before N → FAILED(StoppedBeforeN), then the reply and the screen.
    /// The transition rule refuses it once N is taken (CHECKING always has N).
    fn card_stop_before_n(
        &mut self,
        share_image: ShareImage,
        session: Option<&mut CardSession<'_>>,
    ) -> Vec<DeviceSend> {
        if let Err(reason) = self.card_mutate(CardMutation::Verdict {
            share_image,
            passed: false,
            reason: VerdictReason::StoppedBeforeN,
        }) {
            return refused(reason);
        }
        if let Some(session) = session.filter(|session| session.share_image() == share_image) {
            session.finish_check();
        }
        self.card.tmp_verdict_shown = Some(share_image);
        let mut out = self.card_emit_or_refuse(share_image, CardOutput::Verdict { n: None }, None);
        out.extend(self.card_verdict_screen(share_image, CardScreen::Stopped));
        out
    }

    /// Whether this device holds a share with this image, as a key share or a card record.
    pub(crate) fn holds_share_image(&self, share_image: ShareImage) -> bool {
        self.card.records.contains_key(&share_image) || self.holds_key_share(share_image)
    }

    /// Whether this device holds a key share with this image. A card record whose share it no
    /// longer holds is inert.
    fn holds_key_share(&self, share_image: ShareImage) -> bool {
        self.keys
            .values()
            .flat_map(|key| key.access_structures.values())
            .flat_map(|access_structure| access_structure.shares.values())
            .any(|share| share.share_image == share_image)
    }

    /// A finished job's result. `may_draw` is false while the firmware shows no card screen (a
    /// non-card screen, or home): then nothing is drawn and no opening of D leaves; the next
    /// `WriteStart`, `OpenBox` or re-attach does it (addendum U5).
    #[inline(never)]
    pub fn card_job_event(
        &mut self,
        session: &mut CardSession<'_>,
        event: super::card_session::CardJobEvent,
        may_draw: bool,
    ) -> Vec<DeviceSend> {
        use super::card_session::CardJobEvent;
        let share_image = session.share_image();
        match event {
            CardJobEvent::Encoded => {
                let Some(rec) = self.card_record(&share_image) else {
                    return self.card_job_failed(session);
                };
                let state = rec.state.clone();
                // Binding (2), spec PK.6: the card is the one sealed, under this layout, before
                // any word or P of it leaves the chip. A card not sealed yet is sealed from these
                // words.
                if !session.check_words(rec.words_hash) {
                    return self.card_job_failed(session);
                }
                if state == CardState::Passed {
                    return self.card_recheck_ready(session);
                }
                if core::mem::take(&mut session.final_begin_deferred) {
                    return self.card_final_begin(session);
                }
                match state {
                    // The seal before word 1, persisted as `SealedC` (spec PK.7 (a)).
                    CardState::Unsealed => {
                        session.arm_seal();
                        return vec![];
                    }
                    // A resume in PROVING restarts the proofs, after the seal (spec PK.7).
                    CardState::Proving => self.card_want_proofs(session),
                    // A pull in SEALED that came before the encode was done.
                    CardState::Sealed if session.seal_wanted => session.arm_seal(),
                    _ => {}
                }
                if may_draw && session.ui_at == CardUiStep::Preparing {
                    return self.card_resume_screen(session);
                }
                vec![]
            }
            CardJobEvent::Sealed(seal) => {
                let sealed = CardSealed {
                    format_id: FORMAT_ID,
                    h_c: seal.h_c,
                    coin_commit: seal.coin_commit,
                };
                let Some(rec) = self.card_record(&share_image) else {
                    return self.card_job_failed(session);
                };
                let stored = rec.sealed;
                let mut out = vec![];
                match stored {
                    None => {
                        let Some(words_hash) = session.words_hash() else {
                            return self.card_job_failed(session);
                        };
                        if self
                            .card_mutate(CardMutation::SealedC {
                                share_image,
                                sealed,
                                words_hash,
                            })
                            .is_err()
                        {
                            return self.card_job_failed(session);
                        }
                        session.seal_ready(sealed, seal);
                        out.extend(self.card_emit_or_refuse(
                            share_image,
                            CardOutput::Status,
                            Some(session),
                        ));
                    }
                    Some(stored) if stored == sealed => session.seal_ready(sealed, seal),
                    // This session's seal isn't the one persisted, which the phone pinned.
                    Some(_) => return self.card_job_failed(session),
                }
                // A `DeviceHalfWritten` or `PullProofs` that came before the seal starts the
                // proofs now, in PROVING only.
                if session.proofs_wanted {
                    self.card_want_proofs(session);
                }
                // Only "Preparing" waits for the seal; a screen it ran behind carries on.
                if may_draw && session.ui_at == CardUiStep::Preparing {
                    out.extend(self.card_resume_screen(session));
                }
                out
            }
            // The proving screen gives way to "Look at your phone" (owner B2).
            CardJobEvent::ProofsDone => {
                let proving = self.card_state(&share_image) == Some(CardState::Proving);
                // Owner B8: the openings of D follow the proofs, while words 33–64 are written.
                if proving {
                    self.card_arm_next_opening(session);
                }
                if proving && may_draw && session.ui_at == CardUiStep::Preparing {
                    return self.card_resume_screen(session);
                }
                vec![]
            }
            CardJobEvent::OpeningReady { j } => {
                let mut out = vec![];
                match session.asked {
                    // Owner B8: released as soon as it's asked for and ready; it carries no
                    // screen, and the phone asks its question only at the gate.
                    Some(asked) if asked == j => match self.card_release(session, j) {
                        Ok(released) => {
                            session.trim_ready();
                            return released;
                        }
                        Err(CardRefusal::Busy) => {}
                        Err(reason) => out.extend(refused(reason)),
                    },
                    // The phone asked for another box while this one computed: it's next, unless
                    // its opening is ready and waits for the gate.
                    Some(asked) if session.ready_opening(asked).is_some() => {}
                    Some(asked) => {
                        if let Err(reason) = self.card_start_opening(session, asked) {
                            session.asked = None;
                            out.extend(refused(reason));
                        }
                    }
                    None => {}
                }
                session.trim_ready();
                if may_draw && session.ui_at == CardUiStep::Gate {
                    out.push(self.card_gate_screen(session));
                }
                out
            }
            CardJobEvent::Failed(_) => self.card_job_failed(session),
        }
    }

    /// A self-check failed: the job stops and the session closes. Nothing is persisted.
    fn card_job_failed(&self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        session.fail();
        vec![
            to_coordinator(DeviceCard::Refused {
                reason: CardRefusal::DeviceError,
            }),
            screen(CardScreen::DeviceError),
            to_user(ToUserCard::CloseSession),
        ]
    }

    /// The screen to show for the record's state in this session: words 1–32 or their quiz
    /// once the phone's `WriteStart` matched, "Now use your phone", the proving screen (no word
    /// while the proofs run, owner B2), the final check, or the verdict.
    fn card_resume_screen(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        let Some(state) = self.card_state(&share_image) else {
            return vec![screen(CardScreen::Refused {
                reason: CardRefusal::NotKeygenOrigin,
            })];
        };
        let built = session.words().is_some();
        let shown = match state {
            CardState::OtherFormat => CardScreen::Refused {
                reason: CardRefusal::Unsupported,
            },
            CardState::Unsealed => CardScreen::Preparing {
                task: BusyTask::Seal,
            },
            _ if state.before_checking() && !built => CardScreen::Preparing {
                task: BusyTask::Card,
            },
            CardState::Sealed => CardScreen::WaitingForPhone,
            CardState::Writing | CardState::SelfCheck if !session.write_started => {
                CardScreen::WaitingForPhone
            }
            CardState::Writing => return self.card_show_device_half(session),
            CardState::SelfCheck => return self.card_show_self_check(session),
            CardState::CardChecked => CardScreen::PhoneHalfNext,
            CardState::Proving if session.proofs_complete() => CardScreen::CardFinishedWaiting,
            CardState::Proving => CardScreen::Preparing {
                task: BusyTask::Proofs,
            },
            CardState::Checking => return self.card_checking_screen(session),
            CardState::Passed => {
                return self.card_verdict_screen(share_image, CardScreen::Verdict { passed: true })
            }
            CardState::Failed(reason) => {
                return self.card_verdict_screen(share_image, failed_screen(reason))
            }
        };
        vec![show(session, shown)]
    }

    /// Words 1–32 to write, from word 1.
    fn card_show_device_half(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        if let Err(reason) =
            self.card_emit(share_image, CardOutput::DeviceHalfScreen, Some(session))
        {
            return vec![show(session, CardScreen::Refused { reason })];
        }
        let Some(words) = session.words() else {
            return vec![];
        };
        let words = CardWords32(words[DEVICE_HALF].try_into().expect("32 words"));
        vec![show(session, CardScreen::DeviceHalf { words })]
    }

    /// The quiz on words 1–32, from item 1.
    fn card_show_self_check(&mut self, session: &mut CardSession<'_>) -> Vec<DeviceSend> {
        let share_image = session.share_image();
        if let Err(reason) =
            self.card_emit(share_image, CardOutput::DeviceHalfScreen, Some(session))
        {
            return vec![show(session, CardScreen::Refused { reason })];
        }
        let items = session.quiz_items(DEVICE_HALF.map(|j| j as u8));
        let seed = session.self_check_seed();
        vec![show(session, CardScreen::SelfCheck { items, seed })]
    }

    /// The user acted on a card screen. Each step persists first, then emits one next screen.
    /// An action that doesn't fit the record's state or the screen up (a stale event) is
    /// ignored. `session` is the firmware's open session: a verdict screen can be up without
    /// one.
    #[inline(never)]
    pub fn card_user_action(
        &mut self,
        session: Option<&mut CardSession<'_>>,
        action: CardUserAction,
    ) -> Vec<DeviceSend> {
        let session = match (session, &action) {
            (session, CardUserAction::VerdictAck | CardUserAction::FailAck) => {
                let shown = self
                    .card
                    .tmp_verdict_shown
                    .or(session.map(|session| session.share_image()));
                return shown
                    .map(|share_image| self.card_verdict_ack(share_image, action))
                    .unwrap_or_default();
            }
            (Some(session), _) => session,
            // Every other card screen belongs to a session, and it's gone.
            (None, _) => return vec![],
        };
        let share_image = session.share_image();
        let Some(rec) = self.card_record(&share_image) else {
            return vec![];
        };
        let round_pending = rec
            .device_round
            .is_some_and(|dr| dr.result == DrResult::Pending);
        let round_at = |screen| CardUiStep::DeviceRound { screen };
        match (action, rec.state.clone()) {
            (CardUserAction::DeviceHalfWritten, CardState::Writing)
                if session.ui_at == CardUiStep::DeviceHalf =>
            {
                if self
                    .card_mutate(CardMutation::DeviceHalfWritten { share_image })
                    .is_err()
                {
                    return vec![];
                }
                self.card_show_self_check(session)
            }
            (CardUserAction::SelfCheckPassed, CardState::SelfCheck)
                if session.ui_at == CardUiStep::SelfCheck =>
            {
                if self
                    .card_mutate(CardMutation::CardChecked { share_image })
                    .is_err()
                {
                    return vec![];
                }
                let mut out =
                    self.card_emit_or_refuse(share_image, CardOutput::CheckDone, Some(session));
                out.push(show(session, CardScreen::PhoneHalfNext));
                out
            }
            (CardUserAction::RecheckPassed, CardState::Passed)
                if session.ui_at == CardUiStep::Recheck =>
            {
                vec![
                    to_coordinator(DeviceCard::RecheckDone),
                    show(session, CardScreen::RecheckDone),
                ]
            }
            (CardUserAction::DeviceRoundIntroDone, CardState::Checking)
                if round_pending && session.ui_at == round_at(RoundScreen::Intro) =>
            {
                self.card_show_answer(session)
            }
            (CardUserAction::DeviceRoundPick { display_id, pick }, CardState::Checking)
                if round_pending && session.ui_at == round_at(RoundScreen::Answer) =>
            {
                self.card_round_pick(session, display_id, pick)
            }
            (CardUserAction::DeviceRoundMismatchAck, CardState::Checking)
                if round_pending && session.ui_at == round_at(RoundScreen::Mismatch) =>
            {
                self.card_show_answer(session)
            }
            // Owner B12: the device round runs after N, so the device offers no Stop in it; the
            // only way out is a disconnect, which the phone asks about (owner B9).
            _ => vec![],
        }
    }

    /// The page after a verdict screen: "Card done" after "Backup checked"; after a failed
    /// round or a stop, the next step for it. A verdict screen needs no session (spec L1.7.6).
    fn card_verdict_ack(&self, share_image: ShareImage, action: CardUserAction) -> Vec<DeviceSend> {
        let Some(rec) = self.card_record(&share_image) else {
            return vec![];
        };
        let next = match (action, &rec.state) {
            (CardUserAction::VerdictAck, CardState::Passed) => CardScreen::CardDone,
            // Before N: "Set this wallet up again", with no device left out (addendum U10).
            (CardUserAction::FailAck, CardState::Failed(VerdictReason::StoppedBeforeN)) => {
                CardScreen::StoppedNextStep
            }
            // Owner B8: after N a failed device round leaves this device out too.
            (CardUserAction::FailAck, CardState::Failed(VerdictReason::DeviceRoundFailed))
                if rec.n_tag.is_some() =>
            {
                CardScreen::NextStep
            }
            (CardUserAction::FailAck, CardState::Failed(VerdictReason::DeviceRoundFailed)) => {
                CardScreen::DeviceRoundNextStep
            }
            (CardUserAction::FailAck, CardState::Failed(VerdictReason::PhoneRoundFailed)) => {
                CardScreen::NextStep
            }
            _ => return vec![],
        };
        self.card_verdict_screen(share_image, next)
    }
}
