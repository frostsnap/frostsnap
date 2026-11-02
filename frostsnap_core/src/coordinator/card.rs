//! The phone's side of the verifiable backup card, the 64-word split card in format C′ (spec
//! Part 4): the phone's record of each share's card, persisted as coordinator mutations, and the
//! protocol logic. The flow (spec PK.7, owner B1):
//! 1. The phone pulls the seal's 311 points and words 33–64 (P, 44 bytes), and pins the seal: its
//!    h_C, over the phone's own context and H(P), is persisted and never replaced (owner B3).
//!    Only then does `WriteStart` go out.
//! 2. The device shows words 1–32 and quizzes on them. Once its quiz passed (`Finished`), the
//!    phone asks the user to confirm they're written; the confirm is persisted
//!    (`DeviceHalfWritten`) and sent, and the device starts the bit proofs. Owner B8: N is drawn
//!    and persisted with the confirm (`CheckStarted`) and sent right after it (`OpenStart`); the
//!    device computes the 5 openings of D after the proofs, and the phone verifies them against
//!    the proven seal while words 33–64 are written.
//! 3. Only after the confirm does the phone show words 33–64 and quiz on them, while it verifies
//!    the proofs as they come.
//! 4. [Continue] needs both halves recorded and all 311 proofs verified: the device round runs
//!    on that N, and once its pass is persisted the phone round quizzes 5 words of D, already
//!    verified (owner B8).
//!
//! Every check is the phone's own, against its own wallet record: the seal's context comes from
//! the key, never from the device. Every P the phone takes after the pin must hash to the H(P)
//! inside the pinned h_C (binding (1), spec PK.6), and a seal that isn't the pinned one fails the
//! card (owner B3). Failures are `CoordinatorToUserMessage::Card` events, never `Err`; an `Err`
//! from [`FrostCoordinator::recv_card_message`] means only that a message was unexpected or
//! malformed, and changes nothing.
//!
//! The words of 33–64 stay in RAM ([`PhoneWords`]); the record holds only hashes over them
//! (design O5). An app restart pulls P again.
//!
//! Owner B8 (replacing design S8): N leaves the phone right after the confirm of words 1–32,
//! once persisted. From then on the chip knows D, so a device that misbehaves or refuses is an
//! abort (spec L1.7.6): the round fails, the device is marked suspect and `PhoneRound{n, pass:
//! false}` is sent; a failed device round is an abort too.
//!
//! Owner B12: once N is out there is no Stop, on either screen; its follow-up 5 removes the
//! phone's Stop altogether (a phone's `Stopped` record comes only from a pre-release build;
//! its lost verdict is still collected). The only way out is a disconnect, and owner B9 decides what
//! follows it: while the D check runs (N out, no round result), a disconnect, a device that lost
//! its session, an app closed mid-check, or the user leaving the flow (review F1) is persisted
//! as an interruption (`CheckInterrupted`), and nothing goes to the device until the
//! user says whether they caused it. "Yes" resumes on the same N, where a device that refuses or
//! reports any state but this check's is an abort; "no" is an abort, the device suspect. While
//! the check runs, the device's owed work (its `Status`, the coin, the proofs, an opening, the
//! gate) has a deadline ([`FrostCoordinator::card_device_work`]), so a stall while plugged in is
//! an abort too.
//!
//! Owner B9, one phone per check: only a phone that pinned the seal before the writing started
//! (the device still SEALED) runs the check. A phone with no pin that meets a device past
//! SEALED didn't start this backup: it records nothing and never draws N, so it can never reach
//! Checked (audit-lying-chip F2).
//!
//! Owner B15: only the install that ran the key's keygen (its `KeyFormat`) pins a seal, so only
//! it draws N, takes proofs and openings and reaches Checked. Any other install with the wallet
//! may write the card unverified, once the user chooses to: words 1–32 on the device and 33–64
//! on the phone, pulled for the device's own seal, with both quizzes. It sends nothing else, and
//! its record is "written, not verified" (`Unverified`), never Checked. So a chip that tells a
//! second phone its card is fresh (SEALED) gets no second D from it (second review, lying-chip
//! F2).

use super::*;
use crate::device::card::VerdictReason;
use crate::message::card::*;
use frost_backup::{
    bip39_words::{closest_lookalikes, lookalike_pool, BIP39_WORDS},
    verifiable_card::{
        layout::{Bits, MetaError, COMMITTED, HALF, PHONE_HALF, WORDS},
        phone::{self, OpeningBuilder, PinnedSeal, SealBuilder},
        proof::{
            self, CardError, Context, Ops, Seal, Setup, CHECK_BOXES, FORMAT_ID, POINTS_PER_FRAME,
            PROOFS_PER_FRAME, PROOF_VERSION,
        },
    },
};
use frostsnap_macros::Kind as KindDerive;
use rand_core::{CryptoRng, RngCore};

/// The seal's commitment points, and so the commit proofs a phone verifies (`layout::COMMITTED`).
pub const COMMIT_COUNT: u16 = COMMITTED as u16;

/// The card format this build writes and reads (`proof::FORMAT_ID`). A card restore's
/// `EnterCard` names it as `recorded_format` when the app's record says the key backs up as a
/// card.
pub const CARD_FORMAT: u8 = FORMAT_ID;

/// How a key's shares are backed up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum BackupFormat {
    Words25,
    /// The 64-word split card.
    Card64,
}

/// The seal the phone pinned (spec L1.7.1 A2).
#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct CardPin {
    pub format_id: u8,
    pub proof_version: u8,
    pub h_c: [u8; 32],
    pub coin_commit: [u8; 32],
    /// ser(C_v) for v = 0..311.
    pub commitments: Vec<[u8; 33]>,
}

impl CardPin {
    fn seal(&self) -> Seal {
        Seal {
            commitments: self.commitments.clone(),
            coin_commit: self.coin_commit,
            h_c: self.h_c,
        }
    }
}

/// Why the phone failed a card. It's persisted (inside `Failed` and `RoundResult`), so it's
/// append-only: a variant is never removed or reordered, and
/// `tests/coordinator_backward_compat.rs` pins the encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum CardFailReason {
    /// The device's seal, share image or words 33–64 aren't the ones the phone's record gives,
    /// at the first pin: before any word is shown.
    DifferentCard,
    /// A commit proof failed twice: a device fault.
    BadProof,
    /// The revealed coin isn't the one the seal committed to.
    CoinMismatch,
    /// An opening failed its check twice: a box of D, or a word of 33–64.
    BadOpening,
    /// An opening for a word other than the one asked for.
    WrongBox,
    /// A word of 2048 or more.
    WordRange,
    /// A second miss on the same word.
    TwoMisses,
    Stopped,
    /// A device refusal after N.
    Refused(CardRefusal),
    /// The device's verdict isn't the phone's round result.
    DeviceVerdictMismatch,
    /// A verdict before the phone sent its round result.
    UnexpectedVerdict,
    /// The device's D isn't the one recorded.
    BoxesMismatch,
    /// After N, the device didn't do the work it owed within the phone's deadline (its
    /// `Status`, the coin, the next proofs or opening, or its gate screen): a stall, which is an
    /// abort (spec L1.7.6, review S3, owner B9).
    PhoneTimeout,
    /// The device round failed: N never left the phone.
    DeviceRoundFailed,
    /// Words 33–64 the device sent after the pin don't hash to the H(P) inside the pinned h_C
    /// (binding (1), spec PK.6): a device fault.
    PhoneHalfChanged,
    /// The device reports a seal other than the pinned one (owner B3). An honest device can't:
    /// its seal is deterministic in s and ctx. So it's a device fault and the device is marked
    /// suspect (owner B10): "This device showed a different backup partway through. Don't trust
    /// it. Set this wallet up again without this device." The paper card is never continued
    /// under a new seal.
    SealChanged,
    /// Owner B9: the check was interrupted after N (the device disconnected or lost its session,
    /// or the app closed) and the user said they didn't cause it: the device is marked suspect.
    Interrupted,
    /// Owner R2-2: the user said "Yes, it was me" to a fourth interruption of one check, past
    /// [`MAX_INTERRUPTION_RESUMES`]: the check ends with the device suspect, as on "no". Closes
    /// a chip (or a phantom device on its port) that keeps interrupting the check, and the
    /// replug-forever stall (review F1(4)).
    TooManyInterruptions,
}

/// Owner R2-2: the resumes on "Yes, it was me" one check allows. The next interruption's "yes"
/// ends the check with the device suspect ([`CardFailReason::TooManyInterruptions`]).
pub const MAX_INTERRUPTION_RESUMES: u8 = 3;

/// The phone's record of the device round (spec L2.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum DeviceRoundRecord {
    Pending,
    Passed,
    Failed,
}

/// N, drawn when the final check starts and held unsent until the device round passed (design
/// S8). It encodes as the bare 32 bytes, and its `Debug` never shows them. Never synced or
/// exported.
#[derive(Clone, Copy, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct SecretN(pub [u8; 32]);

impl fmt::Debug for SecretN {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretN(..)")
    }
}

/// Coordinator card mutations. They're persisted, so they're append-only, like the
/// coordinator's other mutations: a variant is never removed or reordered, and each is pinned
/// in `tests/coordinator_backward_compat.rs`.
#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode, KindDerive)]
pub enum CardMutation {
    /// The key's backup format, persisted only by `finalize_keygen`: so it's also the marker
    /// that this install ran the key's keygen (owner B15). A restoration never writes it. Only
    /// a key with it runs the verified check (pin, N, proofs, openings): any other install with
    /// the wallet may write a card only as "written, not verified" (`UnverifiedWriteStarted`,
    /// `UnverifiedWritten`).
    KeyFormat { key_id: KeyId, format: BackupFormat },
    Pinned {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        pin: Box<CardPin>,
    },
    /// The commit proofs of C_from..C_{from + count} verified.
    ProofsVerified {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        from: u16,
        count: u8,
    },
    /// This phone's self-check of words 33–64 passed. `words_hash` is h32 over them (design
    /// §2.7). Staged only with or after `Finished`, before the event that reports it.
    Checked {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        words_hash: [u8; 32],
    },
    /// N, persisted before it's sent. Owner B8: staged with `DeviceHalfWritten`, at the
    /// confirm of words 1–32 once the device's quiz on them passed, and sent before the device
    /// round. In a record from before B8 it comes with or after `DeviceRoundResult{pass: true}`,
    /// on the round's N. `SecretN` encodes as the bare 32 bytes.
    CheckStarted {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        n: SecretN,
    },
    BoxesDrawn {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        coin: [u8; 32],
        d: [u8; CHECK_BOXES],
    },
    BoxAnswered {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        j: u8,
        correct: bool,
    },
    OpeningRejected {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        j: u8,
    },
    /// The phone round's result, persisted before `PhoneRound` is sent. On a pass `words_hash`
    /// is h37 over words 33–64 and D's words, when the phone half is still in RAM.
    RoundResult {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        pass: bool,
        reason: Option<CardFailReason>,
        words_hash: Option<[u8; 32]>,
    },
    DeviceVerdict {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        passed: bool,
    },
    /// A failure before N, or an abort after the phone's round passed.
    Failed {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        reason: CardFailReason,
    },
    /// Not tied to a key, so it outlives the key's deletion.
    DeviceSuspect { device_id: DeviceId },
    /// The device reached CARD_CHECKED: its self-check of words 1–32 passed.
    Finished {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    /// The final check started ([Continue]): the device round runs on the N `CheckStarted`
    /// recorded and sent after the confirm of words 1–32 (owner B8). Only a record from before
    /// B8, with no N yet, draws N here, held unsent until the device round passes (design S8).
    DeviceRoundPending {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        n: SecretN,
    },
    /// The device round's result, persisted before N (on a pass) or the alarm (on a fail).
    DeviceRoundResult {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        pass: bool,
    },
    /// The user confirmed on the phone that words 1–32 are written, after the device's quiz on
    /// them passed (owner B1). Persisted before `DeviceHalfWritten{h_c}` goes out; words 33–64
    /// show only after it.
    DeviceHalfWritten {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    /// Owner B9: the D check was interrupted after N (the device disconnected or lost its
    /// session, or the app was closed while it ran). Until the user says whether they caused it,
    /// nothing goes to the device. Persisted, so an app restart can't skip the question.
    CheckInterrupted {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    /// Owner B9: the user said they caused the interruption: the check resumes on the same N.
    InterruptionAnswered {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    /// Owner B15: an install that didn't run this key's keygen is writing its card unverified:
    /// the user chose to, and `WriteStart{h_c}` for the device's own seal goes out once this is
    /// persisted. `h_c` is the device's, as it reported it: never pinned, never proven. It lets
    /// this phone resume its own writing.
    UnverifiedWriteStarted {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        h_c: [u8; 32],
    },
    /// Owner B15: both quizzes passed on the unverified card (the device's on words 1–32, this
    /// phone's on words 33–64): "written, not verified", never Checked.
    UnverifiedWritten {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
}

impl CardMutation {
    pub fn tied_to_key(&self) -> Option<KeyId> {
        use CardMutation::*;
        match self {
            KeyFormat { key_id, .. } => Some(*key_id),
            Pinned {
                access_structure_ref,
                ..
            }
            | ProofsVerified {
                access_structure_ref,
                ..
            }
            | Checked {
                access_structure_ref,
                ..
            }
            | CheckStarted {
                access_structure_ref,
                ..
            }
            | BoxesDrawn {
                access_structure_ref,
                ..
            }
            | BoxAnswered {
                access_structure_ref,
                ..
            }
            | OpeningRejected {
                access_structure_ref,
                ..
            }
            | RoundResult {
                access_structure_ref,
                ..
            }
            | DeviceVerdict {
                access_structure_ref,
                ..
            }
            | Failed {
                access_structure_ref,
                ..
            }
            | Finished {
                access_structure_ref,
                ..
            }
            | DeviceRoundPending {
                access_structure_ref,
                ..
            }
            | DeviceRoundResult {
                access_structure_ref,
                ..
            }
            | DeviceHalfWritten {
                access_structure_ref,
                ..
            }
            | CheckInterrupted {
                access_structure_ref,
                ..
            }
            | InterruptionAnswered {
                access_structure_ref,
                ..
            }
            | UnverifiedWriteStarted {
                access_structure_ref,
                ..
            }
            | UnverifiedWritten {
                access_structure_ref,
                ..
            } => Some(access_structure_ref.key_id),
            DeviceSuspect { .. } => None,
        }
    }
}

/// The phone's record of one share's card, rebuilt from the mutations. Its `Debug` never shows
/// N.
#[derive(Clone, PartialEq)]
pub struct CoordCardRecord {
    pub pin: Option<CardPin>,
    /// The C's whose commit proofs verified.
    pub verified: Bits,
    /// This phone's self-check of words 33–64 passed.
    pub checked: bool,
    /// Kept in memory until the device's verdict is recorded (it stays in the log).
    pub n: Option<[u8; 32]>,
    pub drawn: Option<([u8; 32], [u8; CHECK_BOXES])>,
    /// Bit k: D[k] was answered right.
    pub answered: u8,
    /// Bit k: one miss on D[k].
    pub misses: u8,
    pub opening_rejects: [u8; WORDS],
    pub round: Option<Result<(), CardFailReason>>,
    pub device_passed: Option<bool>,
    pub failed: Option<CardFailReason>,
    /// The device reached CARD_CHECKED.
    pub finished: bool,
    /// The user confirmed words 1–32 on this phone (owner B1).
    pub device_half_written: bool,
    pub device_round: Option<DeviceRoundRecord>,
    /// N while the device round runs. Dropped from memory when the round fails or the device's
    /// verdict is recorded (it stays in the log).
    pub pending_n: Option<SecretN>,
    /// The hash over the words this phone was given: h32 once its self-check passed, h37 once
    /// its round passed. A later check sends it, and gets the words again only on a match.
    pub words_hash: Option<[u8; 32]>,
    /// Owner B9: the D check was interrupted and the user hasn't said whether they caused it.
    pub interrupted: bool,
    /// Owner R2-2: how many times the user said they caused an interruption of this check and
    /// it resumed. It's the count of `InterruptionAnswered` in the log, so it's persisted with
    /// the check.
    pub resumes: u8,
    /// Owner B15: on an install that didn't run the keygen, the device's own h_C this phone
    /// sent `WriteStart` for, unpinned: an unverified card is being written here.
    pub unverified_h_c: Option<[u8; 32]>,
    /// Owner B15: the unverified card is written and both quizzes passed: "written, not
    /// verified".
    pub unverified_written: bool,
}

impl Default for CoordCardRecord {
    fn default() -> Self {
        Self {
            pin: None,
            verified: Bits::default(),
            checked: false,
            n: None,
            drawn: None,
            answered: 0,
            misses: 0,
            opening_rejects: [0; WORDS],
            round: None,
            device_passed: None,
            failed: None,
            finished: false,
            device_half_written: false,
            device_round: None,
            pending_n: None,
            words_hash: None,
            interrupted: false,
            resumes: 0,
            unverified_h_c: None,
            unverified_written: false,
        }
    }
}

impl fmt::Debug for CoordCardRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoordCardRecord")
            .field("pin", &self.pin)
            .field("verified", &self.verified)
            .field("checked", &self.checked)
            .field("n", &self.n.map(SecretN))
            .field("drawn", &self.drawn)
            .field("answered", &self.answered)
            .field("misses", &self.misses)
            .field("opening_rejects", &self.opening_rejects)
            .field("round", &self.round)
            .field("device_passed", &self.device_passed)
            .field("failed", &self.failed)
            .field("finished", &self.finished)
            .field("device_half_written", &self.device_half_written)
            .field("device_round", &self.device_round)
            .field("pending_n", &self.pending_n)
            .field("words_hash", &self.words_hash)
            .field("interrupted", &self.interrupted)
            .field("resumes", &self.resumes)
            .field("unverified_h_c", &self.unverified_h_c)
            .field("unverified_written", &self.unverified_written)
            .finish()
    }
}

impl CoordCardRecord {
    /// The status rule (two keys): Checked needs the phone round, the device's verdict and the
    /// device round. Owner B15: a card written on an install that didn't run the keygen is
    /// `Unverified` once both quizzes passed, and never Checked.
    pub fn status(&self) -> CardBackupStatus {
        if self.failed.is_some()
            || matches!(self.round, Some(Err(_)))
            || self.device_passed == Some(false)
            || self.device_round == Some(DeviceRoundRecord::Failed)
        {
            CardBackupStatus::Failed
        } else if self.round == Some(Ok(()))
            && self.device_passed == Some(true)
            && self.device_round == Some(DeviceRoundRecord::Passed)
        {
            CardBackupStatus::Checked
        } else if self.unverified_written {
            CardBackupStatus::Unverified
        } else if self.finished
            || self.unverified_h_c.is_some()
            || self.device_half_written
            || self.checked
            || self.device_round.is_some()
            || self.n.is_some()
            || self.pin.is_some()
        {
            CardBackupStatus::NotChecked
        } else {
            CardBackupStatus::NoCard
        }
    }

    pub fn fail_reason(&self) -> Option<CardFailReason> {
        match (self.failed, self.round, self.device_passed) {
            (Some(reason), _, _) | (_, Some(Err(reason)), _) => Some(reason),
            (_, _, Some(false)) => Some(CardFailReason::DeviceVerdictMismatch),
            _ if self.device_round == Some(DeviceRoundRecord::Failed) => {
                Some(CardFailReason::DeviceRoundFailed)
            }
            _ => None,
        }
    }

    /// Both halves are recorded: the device's self-check passed (`Finished`) and this phone's
    /// (`Checked`). The app's backup run is complete from here (design §3.10).
    pub fn halves_recorded(&self) -> bool {
        self.finished && self.checked
    }

    /// Every commit proof verified against the pinned seal.
    pub fn proven(&self) -> bool {
        self.pin.is_some() && (0..COMMITTED).all(|v| self.verified.get(v))
    }

    /// The final check started: the device round. Owner B8: N is drawn earlier, with the
    /// confirm of words 1–32, so it doesn't start the final check.
    fn in_final_check(&self) -> bool {
        self.device_round.is_some() || self.round.is_some()
    }

    /// Words 33–64 are next on the phone: the user confirmed words 1–32 (owner B1), and neither
    /// the final check nor a failure has come.
    fn phone_half_next(&self) -> bool {
        self.device_half_written
            && !self.in_final_check()
            && self.status() != CardBackupStatus::Failed
    }

    /// A later check (design §3.9) is only for a card this phone holds as a two-key pass, or
    /// from a phone with no record of the card. A device's PASSED never stands in for this
    /// phone's own final check: a record pinned or written here but not passed here gets no
    /// later check, whatever the device reports (audit F1, 2 Nov).
    ///
    /// Owner B15: an unverified card (this install didn't run the keygen) holds no record of a
    /// check, as a phone with no record holds none: the later check is the device's half only.
    fn may_recheck(&self) -> bool {
        match self.status() {
            CardBackupStatus::Checked | CardBackupStatus::NoCard | CardBackupStatus::Unverified => {
                true
            }
            CardBackupStatus::NotChecked => self.unverified_h_c.is_some() && self.pin.is_none(),
            CardBackupStatus::Failed => false,
        }
    }

    /// N is committed to the device round: from here every end but finishing the phone round is
    /// an abort. Owner B8: N goes out before the device round, and an end before that round is a
    /// device fault (an abort) too; this only says which side of the device round an alarm is
    /// on. Owner B12: there's no Stop on either side ([`n_out`](Self::n_out)).
    fn after_n(&self) -> bool {
        (self.n.is_some() || self.round.is_some()) && self.device_round.is_some()
    }

    /// Owner B8, B12: N is persisted, so it may have reached the device, and the check hasn't
    /// ended. From here there's no Stop, and a disconnect while the D check runs asks the user
    /// whether they caused it (owner B9).
    fn n_out(&self) -> bool {
        self.n.is_some() && self.check_not_ended()
    }

    /// Owner B9: the D check runs: N is out and the phone round has no result yet. An
    /// interruption now could give a chip that saw D another draw, so it asks the user.
    fn check_running(&self) -> bool {
        self.n_out() && self.round.is_none()
    }

    /// Owner R2-2: how many more "Yes, it was me" answers resume this check before a "yes"
    /// ends it.
    pub fn resumes_left(&self) -> u8 {
        MAX_INTERRUPTION_RESUMES.saturating_sub(self.resumes)
    }

    /// Owner B9: an interruption of the running check the user hasn't answered.
    pub fn interruption_pending(&self) -> bool {
        self.interrupted && self.check_running()
    }

    /// Owner B8: N is drawn and sent, and the device round hasn't started.
    fn early_n(&self) -> Option<[u8; 32]> {
        self.n.filter(|_| self.device_round.is_none())
    }

    /// The phone's round passed and nothing failed the card since: the result the device
    /// should report.
    fn passed_round(&self) -> bool {
        self.round == Some(Ok(())) && self.failed.is_none()
    }

    /// Nothing has ended the check: no failure, no failed round and no device verdict. A round
    /// that passed still waits for the device's verdict.
    fn check_not_ended(&self) -> bool {
        self.failed.is_none() && !matches!(self.round, Some(Err(_))) && self.device_passed.is_none()
    }

    /// The first k of D not answered yet.
    fn first_unanswered(&self) -> Option<u8> {
        (0..CHECK_BOXES as u8).find(|k| self.answered & (1 << k) == 0)
    }

    /// k with D[k] = j.
    fn d_position(&self, j: u8) -> Option<u8> {
        let (_, d) = self.drawn?;
        d.iter().position(|&b| b == j).map(|k| k as u8)
    }
}

/// A quiz on the phone: (word number, word) per item, retry until right.
#[derive(Clone, PartialEq)]
struct SelfCheck {
    items: Vec<(u8, u16)>,
    k: usize,
    /// A later check's quiz.
    recheck: bool,
}

/// The phone half in RAM (design O5): words 33–64 once they're checked against the pinned seal,
/// D's words, and where the user is in pages and quiz. It's kept across device sessions and
/// unplugs, and cleared on the verdict, a different card, or an app exit: never persisted. Its
/// `Debug` shows no word and it's wiped when dropped.
#[derive(Clone, PartialEq)]
pub struct PhoneWords {
    h_c: [u8; 32],
    /// Words 33–64 (P), once they hashed to the H(P) inside the pinned h_C.
    half: Option<[u16; HALF]>,
    /// The phone round's words, in D's order.
    d_words: [Option<u16>; CHECK_BOXES],
    /// The page last shown, 0..=11.
    page: u8,
    /// "I've written words 33–64" was held.
    written: bool,
    self_check: Option<SelfCheck>,
    /// Owner B8: the openings of D may be fetched ahead of the phone round, while words 33–64
    /// are written. Only when these words came into RAM before N was drawn: then every opening
    /// verified since is still here. Words that came back after N (an app restart lost D's
    /// verified words) fetch an opening only in the phone round, for the box it asks, since each
    /// opening the device computes in a new session spends one of its capped runs (review P1).
    ahead: bool,
}

impl PhoneWords {
    fn new(h_c: [u8; 32], ahead: bool) -> Self {
        Self {
            h_c,
            half: None,
            d_words: [None; CHECK_BOXES],
            page: 0,
            written: false,
            self_check: None,
            ahead,
        }
    }

    fn complete(&self) -> Option<[u16; HALF]> {
        self.half
    }
}

impl fmt::Debug for PhoneWords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PhoneWords")
            .field("half_in", &self.half.is_some())
            .field("page", &self.page)
            .field("written", &self.written)
            .finish_non_exhaustive()
    }
}

impl Drop for PhoneWords {
    fn drop(&mut self) {
        if let Some(half) = self.half.as_mut() {
            crate::device::card::wipe(half, [0; HALF]);
        }
        crate::device::card::wipe(&mut self.d_words, [None; CHECK_BOXES]);
        if let Some(check) = self.self_check.as_mut() {
            check.items.iter_mut().for_each(|item| *item = (0, 0));
        }
    }
}

/// The phone's pages of words 33–64: 0..=10 hold three words each (the last two), and page 11
/// is all 32.
pub const PHONE_PAGES: u8 = 11;
const WORDS_PER_PAGE: usize = 3;

/// The word positions on `page`.
fn page_words(page: u8) -> core::ops::Range<usize> {
    if page >= PHONE_PAGES {
        return 0..HALF;
    }
    let first = page as usize * WORDS_PER_PAGE;
    first..(first + WORDS_PER_PAGE).min(HALF)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub formats: BTreeMap<KeyId, BackupFormat>,
    pub records: BTreeMap<(AccessStructureRef, ShareIndex), CoordCardRecord>,
    pub suspects: BTreeSet<DeviceId>,
    tmp_sessions: BTreeMap<DeviceId, CoordCardSession>,
    /// The phone half of each card in RAM (design O5).
    tmp_phone_words: BTreeMap<(AccessStructureRef, ShareIndex), PhoneWords>,
    /// A persist failed: every card action and handler refuses until the app restarts.
    poisoned: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardBackupStatus {
    NoCard,
    NotChecked,
    Checked,
    Failed,
    /// Owner B15: written on an install that didn't run the keygen, with both quizzes passed,
    /// and never verified: no pin, no N, no proofs or openings. It must be verified on a trusted
    /// device that learns the entire share (all 64 words).
    Unverified,
}

impl State {
    pub fn apply(&mut self, m: CardMutation) -> Option<CardMutation> {
        use CardMutation::*;
        let changed = match &m {
            KeyFormat { key_id, format } => self.formats.insert(*key_id, *format) != Some(*format),
            DeviceSuspect { device_id } => self.suspects.insert(*device_id),
            Pinned {
                access_structure_ref,
                share_index,
                pin,
            } => {
                // Owner B15: only the install that ran the keygen pins, and every step of the
                // verified check (N, proofs, openings) needs the pin.
                if !self.created_here(access_structure_ref.key_id) {
                    crate::fail!("a card pinned on an install that didn't run the keygen");
                }
                let rec = self.record_mut(*access_structure_ref, *share_index);
                // A pin is never replaced.
                rec.pin.is_none() && {
                    rec.pin = Some(pin.as_ref().clone());
                    true
                }
            }
            ProofsVerified {
                access_structure_ref,
                share_index,
                from,
                count,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                let range = *from as usize..(*from as usize + *count as usize).min(COMMITTED);
                let before = rec.verified;
                for v in range {
                    rec.verified.set(v, true);
                }
                rec.verified != before
            }
            Checked {
                access_structure_ref,
                share_index,
                words_hash,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                if rec.checked {
                    false
                } else if !rec.finished || !rec.device_half_written {
                    crate::fail!("card checked before the device's half was");
                } else {
                    rec.checked = true;
                    rec.words_hash = Some(*words_hash);
                    true
                }
            }
            CheckStarted {
                access_structure_ref,
                share_index,
                n,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                // N is never redrawn.
                if rec.n.is_some() || rec.device_passed.is_some() {
                    false
                } else if rec.device_round.is_none() {
                    // Owner B8: N is drawn and persisted right after the confirm of words 1–32,
                    // once the device's quiz on them passed, and sent before the device round.
                    if rec.pin.is_none()
                        || !rec.finished
                        || !rec.device_half_written
                        || rec.status() == CardBackupStatus::Failed
                    {
                        crate::fail!("card N committed before the device half was confirmed");
                    }
                    rec.n = Some(n.0);
                    true
                } else if rec.device_round != Some(DeviceRoundRecord::Passed)
                    || rec.pending_n != Some(*n)
                {
                    crate::fail!("card N committed before the device round passed");
                } else {
                    rec.n = Some(n.0);
                    true
                }
            }
            BoxesDrawn {
                access_structure_ref,
                share_index,
                coin,
                d,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                rec.drawn.is_none() && {
                    rec.drawn = Some((*coin, *d));
                    true
                }
            }
            BoxAnswered {
                access_structure_ref,
                share_index,
                j,
                correct,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                match rec.d_position(*j) {
                    Some(k) => {
                        let bits = if *correct {
                            &mut rec.answered
                        } else {
                            &mut rec.misses
                        };
                        let before = *bits;
                        *bits |= 1 << k;
                        *bits != before
                    }
                    None => false,
                }
            }
            OpeningRejected {
                access_structure_ref,
                share_index,
                j,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                match rec.opening_rejects.get_mut(*j as usize) {
                    Some(rejects) => {
                        *rejects = rejects.saturating_add(1);
                        true
                    }
                    None => false,
                }
            }
            RoundResult {
                access_structure_ref,
                share_index,
                pass,
                reason,
                words_hash,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                let round = match (pass, reason) {
                    (true, _) => Some(Ok(())),
                    (false, Some(reason)) => Some(Err(*reason)),
                    (false, None) => None,
                };
                rec.round.is_none() && round.is_some() && {
                    rec.round = round;
                    if let Some(words_hash) = words_hash.filter(|_| *pass) {
                        rec.words_hash = Some(words_hash);
                    }
                    true
                }
            }
            DeviceVerdict {
                access_structure_ref,
                share_index,
                passed,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                rec.device_passed.is_none() && {
                    rec.device_passed = Some(*passed);
                    // Hygiene: N is no longer needed once the check is over.
                    rec.n = None;
                    rec.pending_n = None;
                    true
                }
            }
            Failed {
                access_structure_ref,
                share_index,
                reason,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                rec.failed.is_none() && {
                    rec.failed = Some(*reason);
                    true
                }
            }
            Finished {
                access_structure_ref,
                share_index,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                !core::mem::replace(&mut rec.finished, true)
            }
            DeviceRoundPending {
                access_structure_ref,
                share_index,
                n,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                if rec.device_round == Some(DeviceRoundRecord::Pending) && rec.pending_n == Some(*n)
                {
                    false
                } else if !rec.proven()
                    || !rec.checked
                    || !rec.finished
                    || !rec.device_half_written
                    // Owner B8: the round runs on the N sent after the confirm of words 1–32.
                    || rec.n.is_some_and(|recorded| recorded != n.0)
                    || rec.device_round.is_some()
                    || rec.status() == CardBackupStatus::Failed
                {
                    crate::fail!("card device round started out of order");
                } else {
                    rec.device_round = Some(DeviceRoundRecord::Pending);
                    rec.pending_n = Some(*n);
                    true
                }
            }
            DeviceRoundResult {
                access_structure_ref,
                share_index,
                pass,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                let result = if *pass {
                    DeviceRoundRecord::Passed
                } else {
                    DeviceRoundRecord::Failed
                };
                match rec.device_round {
                    Some(DeviceRoundRecord::Pending) => {
                        rec.device_round = Some(result);
                        if !*pass {
                            // N never leaves the phone.
                            rec.pending_n = None;
                        }
                        true
                    }
                    Some(recorded) if recorded == result => false,
                    _ => crate::fail!("card device round result without a pending round"),
                }
            }
            DeviceHalfWritten {
                access_structure_ref,
                share_index,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                if rec.device_half_written {
                    false
                } else if rec.pin.is_none()
                    || !rec.finished
                    || rec.n.is_some()
                    || rec.device_round.is_some()
                    || rec.status() == CardBackupStatus::Failed
                {
                    crate::fail!("card device half confirmed out of order");
                } else {
                    rec.device_half_written = true;
                    true
                }
            }
            CheckInterrupted {
                access_structure_ref,
                share_index,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                // Only while the D check runs; once it has ended there's nothing to resume.
                !rec.interrupted && rec.check_running() && {
                    rec.interrupted = true;
                    true
                }
            }
            InterruptionAnswered {
                access_structure_ref,
                share_index,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                // Owner R2-2 counts them; the cap is enforced where the answer is taken
                // (`card_interruption_answer`), so a log from before the cap replays as it was.
                core::mem::replace(&mut rec.interrupted, false) && {
                    rec.resumes = rec.resumes.saturating_add(1);
                    true
                }
            }
            UnverifiedWriteStarted {
                access_structure_ref,
                share_index,
                h_c,
            } => {
                if self.created_here(access_structure_ref.key_id) {
                    crate::fail!("an unverified card on the install that ran the keygen");
                }
                let rec = self.record_mut(*access_structure_ref, *share_index);
                if rec.unverified_h_c.is_some() {
                    // The first stands: a resume sends `WriteStart` for it again.
                    false
                } else if rec.pin.is_some() || rec.n.is_some() {
                    crate::fail!("an unverified card over a verified record");
                } else {
                    rec.unverified_h_c = Some(*h_c);
                    true
                }
            }
            UnverifiedWritten {
                access_structure_ref,
                share_index,
            } => {
                let rec = self.record_mut(*access_structure_ref, *share_index);
                if rec.unverified_written {
                    false
                } else if rec.unverified_h_c.is_none() {
                    crate::fail!("an unverified card written before it was started");
                } else {
                    rec.unverified_written = true;
                    true
                }
            }
        };
        changed.then_some(m)
    }

    /// Owner B15: this install ran the key's keygen, as a card key (its `KeyFormat`).
    fn created_here(&self, key_id: KeyId) -> bool {
        self.formats.get(&key_id) == Some(&BackupFormat::Card64)
    }

    fn record_mut(
        &mut self,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> &mut CoordCardRecord {
        self.records
            .entry((access_structure_ref, share_index))
            .or_default()
    }

    pub fn clear_up_key_deletion(&mut self, key_id: KeyId) {
        self.formats.remove(&key_id);
        self.records
            .retain(|(access_structure_ref, _), _| access_structure_ref.key_id != key_id);
        self.tmp_sessions
            .retain(|_, session| session.asref.key_id != key_id);
        self.tmp_phone_words
            .retain(|(access_structure_ref, _), _| access_structure_ref.key_id != key_id);
    }

    pub fn clear_tmp_data(&mut self) {
        self.tmp_sessions.clear();
        self.tmp_phone_words.clear();
        self.poisoned = false;
    }

    /// Another request goes to the device, and a refusal names no request: so one that comes
    /// back must never reach this device's card flow, where after N it would be an abort. The
    /// next `card_begin` opens the flow again from the record.
    pub(super) fn end_session(&mut self, device_id: DeviceId) {
        self.tmp_sessions.remove(&device_id);
    }

    /// The device's session is gone (an unplug, or the user left the page): its gate screen
    /// went with it, so a question waits for the next session's `GateShown`. The phone half stays
    /// in RAM.
    fn device_disconnected(&mut self, device_id: DeviceId) {
        if let Some(session) = self.tmp_sessions.get_mut(&device_id) {
            session.gate_shown = false;
            session.device_state = None;
            session.awaiting_phone_half = None;
            // The protocol sends its `Begin` again once the device is back.
            session.status_due = true;
            session.link_refused = false;
        }
    }
}

/// Where one device's card flow is, as far as the phone can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionPhase {
    /// Waiting for the device's `Status`, or pulling the seal's points.
    Pinning,
    /// The card is being written and self-checked on both screens.
    Writing,
    /// Both halves are recorded: the final check is next.
    Finished,
    DeviceRound,
    PhoneRound,
    /// A later check of a PASSED card.
    Recheck,
    /// The flow ended here, or the device's state isn't one this phone serves.
    Closed,
}

/// The phone's RAM state for one device's card flow.
#[derive(Clone, Debug, PartialEq)]
struct CoordCardSession {
    asref: AccessStructureRef,
    share_index: ShareIndex,
    /// Built from the phone's own record of the key.
    context: Context,
    phase: SessionPhase,
    device_state: Option<CardStateReport>,
    /// The seal and proof version from the last `Status`, kept to pin the points against.
    sealed: Option<(CardSealed, u8)>,
    seal_builder: Option<SealBuilder>,
    /// The seal from the points, with no pin yet: it's pinned once words 33–64 come.
    unpinned_seal: Option<Seal>,
    pinned: Option<PinnedSeal>,
    /// A `Status` parked while words 33–64 are pulled again: it's handled once they hashed to
    /// the pinned H(P). The phone needs P before anything that uses it (binding (1)).
    awaiting_phone_half: Option<CardStatus>,
    /// This device has sent a commit-proof frame that failed, in this session: the next bad
    /// frame fails the card. Keyed on the session, which is this device's own, never on the
    /// frame's device-chosen `from`, so a device can't dodge the second strike by changing it
    /// (core-protocol review F2). A good frame doesn't clear it, or a device could interleave
    /// proofs already verified to keep striking once. An honest device verifies every proof
    /// before it releases it.
    proof_strike: bool,
    /// The opening of a box of D asked for.
    opening: Option<OpeningBuilder>,
    /// A verified, unanswered opening of D: (k, j, y). It's kept across a reconnect, so asking
    /// its question again spends no run.
    current: Option<(u8, u8, u16)>,
    /// The device's `GateShown` came in its current session: only then is a question shown or
    /// an answer taken (spec §0.6.1.2 item 2).
    gate_shown: bool,
    /// The last request sent, which the protocol re-sends after a `Busy`.
    outstanding: Option<CoordinatorCard>,
    /// A `Begin` went out, or the protocol sends it again (after an unplug or a `NoSession`),
    /// and its `Status` hasn't come yet. A device `Status` is taken only then: never
    /// unsolicited (audit F1b).
    status_due: bool,
    /// The bit proofs verified from this device in its current session. Only once all 311 are
    /// does the device hold its proofs, so an opening it computes doesn't wait for them all to be
    /// computed again: a resume that pulls only the phone's missing proofs leaves the device
    /// computing the rest before the opening (review P3).
    proofs_this_session: Bits,
    /// The device refused `Begin` with `NotDirectUsb`: it's on a link that can't run the flow (a
    /// daisy chain), not stalling, so it owes nothing until it's unplugged or `Begin` goes again
    /// (review P2).
    link_refused: bool,
    /// The app sees another device on this device's USB port, as of its last card message
    /// ([`FrostCoordinator::card_port_shared`]). Only then can it be downstream of another
    /// device, so only then is a `NotDirectUsb` taken as a link refusal (re-attack RA-1).
    /// `false` until the app says so: a caller that never says aborts a `NotDirectUsb` after N.
    port_shared: bool,
    /// Owner B15: this install didn't run the key's keygen, so this flow never runs the
    /// verified check: it may only write the card unverified, with both quizzes. It never pins,
    /// draws N, pulls proofs or openings.
    unverified: bool,
    /// Owner B15: the user confirmed words 1–32 on this phone for the unverified card, after
    /// the device's quiz on them passed in this session. Only then do words 33–64 show. RAM
    /// only: nothing goes to the device, which stays in CARD_CHECKED.
    unverified_confirmed: bool,
}

impl CoordCardSession {
    fn key(&self) -> (AccessStructureRef, ShareIndex) {
        (self.asref, self.share_index)
    }
}

/// H and the proof transcript, computed once per process. `Setup` isn't `PartialEq`, so the
/// coordinator never stores one.
pub fn card_setup() -> &'static Setup {
    static SETUP: std::sync::OnceLock<Setup> = std::sync::OnceLock::new();
    SETUP.get_or_init(Setup::new)
}

/// The user's answer to a phone-round question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxAnswer {
    /// The word the user picked, as a word index.
    Picked(u16),
    /// "None of these": the word they wrote isn't among the options.
    NoneOfThese,
}

#[derive(Clone, Debug)]
pub struct CardToUser {
    pub device_id: DeviceId,
    pub access_structure_ref: AccessStructureRef,
    pub share_index: ShareIndex,
    pub event: CardEvent,
}

/// What the phone's card flow shows. Its `Debug` shows the variant name only: several carry
/// words, and the coordinator logs at DEBUG.
#[derive(Clone, KindDerive)]
pub enum CardEvent {
    PreparingCard,
    Pinning {
        points: u16,
    },
    DifferentCard,
    /// "Write words 1–32 from your device, then answer its quiz."
    DeviceHalf,
    /// The device's quiz on words 1–32 passed: the phone asks the user to confirm they're
    /// written ("I've written words 1–32"), which [`FrostCoordinator::card_device_half_written`]
    /// takes (owner B1). Words 33–64 show only after it.
    ConfirmDeviceHalf,
    /// A page of words 33–64 (page 11 is all 32), checked against the pinned seal. `first_word`
    /// is the first word's number.
    PhonePage {
        page: u8,
        first_word: u8,
        words: Vec<&'static str>,
    },
    /// The quiz's next item: word `word_number`, among its two closest lookalikes (the
    /// protocol shuffles them). `wrong`: the same item after a wrong pick.
    SelfCheckQuestion {
        k: u8,
        total: u8,
        word_number: u8,
        word: &'static str,
        lookalikes: [&'static str; 2],
        wrong: bool,
        recheck: bool,
    },
    /// This phone's quiz on words 33–64 passed, and `Checked` is persisted.
    PhoneSelfChecked,
    /// The device reached CARD_CHECKED (its self-check passed), and `Finished` is persisted.
    CardFinished,
    /// The phone's own refusal, never the device's: the device reports a state below
    /// CARD_CHECKED for a card this phone recorded as finished. Nothing is served ("This card
    /// is finished").
    AlreadyFinished,
    /// The device round runs on the device.
    DeviceRound,
    /// "Checking your card": `verified` of the `total` bit proofs have verified. The device
    /// computes them once the user confirmed words 1–32, while words 33–64 are written (owner
    /// B1); a page of words on the phone stays up meanwhile.
    Proving {
        verified: u16,
        total: u16,
    },
    /// All 311 bit proofs verified.
    Proven,
    /// Both halves are recorded and the seal is proven: [Continue] to the final check
    /// ([`FrostCoordinator::card_final_begin`]).
    ReadyForFinalCheck,
    PreparingBox {
        k: u8,
    },
    /// A phone-round question: word `word_number`, which the device opened with proofs, and its
    /// six closest lookalikes, of which the protocol shows two (design O11).
    Question {
        k: u8,
        word_number: u8,
        word: &'static str,
        pool: [&'static str; 6],
        second_look: bool,
    },
    /// The first miss on D[k]: its question comes again, as a second look.
    TryAgain {
        k: u8,
    },
    /// The device's gate screen is up in its current session, after its round passed: the
    /// phone round's questions may show.
    GateShown,
    /// The phone's round result went to the device; `answered` boxes of D were answered
    /// right.
    RoundSent {
        answered: u8,
    },
    Passed,
    Failed {
        reason: CardFailReason,
        device_suspect: bool,
        /// N had gone to the device: the check ended after N.
        after_n: bool,
    },
    Refused {
        reason: CardRefusal,
    },
    StartedOnAnotherPhone,
    /// A later check of a PASSED card started: the device runs its quiz.
    RecheckWaiting,
    /// This phone's quiz in a later check passed.
    RecheckPassed,
    /// The device's quiz in a later check ended.
    RecheckDeviceDone,
    /// This phone has no record of words 33–64 the device recognises: only the device's half is
    /// checked.
    RecheckUnavailable,
    /// Re-send `resend` after a second.
    Busy {
        resend: Box<CoordinatorToDeviceMessage>,
    },
    /// The device has no session: send `Begin` again.
    NeedsBegin,
    StorageError,
    /// Owner B8, B12: N is persisted and about to go out: from now on there's no Stop.
    CheckStarted,
    /// Owner B9: the D check was interrupted (the device disconnected or lost its session) and
    /// the interruption is persisted: ask the user whether they caused it
    /// ([`FrostCoordinator::card_interruption_answer`]), and send the device nothing until then.
    /// `resumes_left`: how many more "yes" answers resume this check (owner R2-2): at 1 this
    /// "yes" is the last resume, at 0 "yes" ends the check too (review B15 protocol F2).
    Interrupted {
        resumes_left: u8,
    },
    /// Owner B9: the user caused the interruption: the flow sends `Begin` again once the device
    /// is plugged in, and the check resumes on the same N.
    Resuming,
    /// Owner B15: this install didn't run the keygen, and the device's card is unwritten
    /// (SEALED). The phone says it can't verify the backup, and writes it only once the user
    /// chooses to ([`FrostCoordinator::card_write_unverified`]). Nothing went to the device but
    /// `Begin`.
    UnverifiedIntro,
    /// Owner B15: the unverified card is written and both quizzes passed, and
    /// `UnverifiedWritten` is persisted: "written, not verified". Also the page for a card this
    /// phone already wrote unverified.
    UnverifiedWritten,
}

impl fmt::Debug for CardEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())
    }
}

/// A frame from a device in a card restore, for the restore page.
#[derive(Clone, Debug)]
pub struct CardRestoreToUser {
    pub device_id: DeviceId,
    pub event: CardRestoreEvent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardRestoreEvent {
    /// The device's view of the halves: the acknowledgement of `EnterCard`, and every change.
    State {
        device_half_in: bool,
        phone_half_in: bool,
        decoding: bool,
    },
    /// T_dec: what the decode came to, and nothing else (owner B7).
    Outcome { result: RestoreResult },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardStartError {
    UnknownKey,
    NoShareOnDevice,
    CouldntDecryptRootKey,
    /// The index or threshold can't be written on a card.
    Metadata(CardRefusal),
    Poisoned,
}

impl fmt::Display for CardStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CardStartError::UnknownKey => write!(f, "no such key or access structure"),
            CardStartError::NoShareOnDevice => write!(f, "the device has no share in this key"),
            CardStartError::CouldntDecryptRootKey => write!(f, "couldn't decrypt the root key"),
            CardStartError::Metadata(reason) => {
                write!(f, "this share can't be written on a card: {reason:?}")
            }
            CardStartError::Poisoned => {
                write!(f, "card storage failed; restart the app to continue")
            }
        }
    }
}

impl std::error::Error for CardStartError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardActionError {
    NoSession,
    WrongState(&'static str),
    /// The action was confirmed on a page that's no longer up: an answer to an interruption
    /// already answered. Ignore it.
    Stale,
    Poisoned,
}

impl fmt::Display for CardActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CardActionError::NoSession => write!(f, "no card flow with this device"),
            CardActionError::WrongState(why) => write!(f, "can't do that now: {why}"),
            CardActionError::Stale => write!(f, "that's no longer on screen"),
            CardActionError::Poisoned => {
                write!(f, "card storage failed; restart the app to continue")
            }
        }
    }
}

impl std::error::Error for CardActionError {}

/// What the device owes the phone while the D check runs (owner B9): the protocol gives each
/// item a deadline, and a device that misses it is stalling, which is an abort
/// ([`FrostCoordinator::card_device_timeout`]). The user-paced steps (writing words 33–64, the
/// device round's answers, the phone round's questions) owe nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardDeviceWork {
    /// The `Status` that answers the flow's `Begin`.
    Status,
    /// Words 33–64 again (`PullPhoneHalf`), after an app restart.
    PhoneHalf,
    /// The coin, which answers `OpenStart`.
    Coin,
    /// The next frame of bit proofs.
    Proofs,
    /// The next opening of D. `proofs_first`: the device may not hold all its proofs in this
    /// session (it is in PROVING, and the phone didn't pull all 311 from it), so it computes the
    /// rest again before the opening (EARLY-N: a resume in PROVING recomputes the proofs).
    Opening { proofs_first: bool },
    /// The gate screen, once the device round passed.
    Gate,
}

fn to_device(device_id: DeviceId, message: CoordinatorCard) -> CoordinatorSend {
    CoordinatorSend::ToDevice {
        message: CoordinatorToDeviceMessage::Card(message),
        destinations: [device_id].into(),
    }
}

fn word(y: u16) -> &'static str {
    BIP39_WORDS[y as usize]
}

/// The word index of a BIP39 word as the card events give it, for a pick or an entry.
pub fn word_index(word: &str) -> Option<u16> {
    BIP39_WORDS
        .iter()
        .position(|w| *w == word)
        .map(|i| i as u16)
}

/// The backup words that start with `prefix` (capitals): a restore keyboard's suggestions.
pub fn words_with_prefix(prefix: &str) -> &'static [&'static str] {
    frost_backup::bip39_words::words_with_prefix(prefix)
}

/// The letters (capitals) that follow `prefix` (capitals) in some backup word: the keys a
/// restore keyboard enables.
pub fn valid_next_letters(prefix: &str) -> String {
    frost_backup::bip39_words::get_valid_next_letters(prefix)
        .iter_valid()
        .collect()
}

/// The question for an opened word of D, with its six closest lookalikes (design O11).
fn question(k: u8, j: u8, y: u16, second_look: bool) -> CardEvent {
    let pool = lookalike_pool(y).expect("check_opening refuses y ≥ 2048");
    CardEvent::Question {
        k,
        word_number: j + 1,
        word: word(y),
        pool: pool.map(word),
        second_look,
    }
}

/// What one card handler or action returns: sends to the device and events to the user, in
/// order.
struct CardOut<'a> {
    device_id: DeviceId,
    session: &'a mut CoordCardSession,
    sends: Vec<CoordinatorSend>,
}

impl CardOut<'_> {
    fn event(&mut self, event: CardEvent) {
        self.sends
            .push(CoordinatorSend::ToUser(CoordinatorToUserMessage::Card(
                CardToUser {
                    device_id: self.device_id,
                    access_structure_ref: self.session.asref,
                    share_index: self.session.share_index,
                    event,
                },
            )));
    }

    /// A request the device replies to; it's re-sent after a `Busy`.
    fn request(&mut self, message: CoordinatorCard) {
        self.session.outstanding = Some(message.clone());
        self.notify(message);
    }

    /// A message the device doesn't reply to.
    fn notify(&mut self, message: CoordinatorCard) {
        self.sends.push(to_device(self.device_id, message));
    }
}

impl FrostCoordinator {
    pub fn key_backup_format(&self, key_id: KeyId) -> BackupFormat {
        self.card
            .formats
            .get(&key_id)
            .copied()
            .unwrap_or(BackupFormat::Words25)
    }

    pub fn device_is_suspect(&self, device_id: DeviceId) -> bool {
        self.card.suspects.contains(&device_id)
    }

    /// Owner B15: this install ran the key's keygen, as a card key. Only then does it run the
    /// verified check (pin a seal, draw N, take proofs and openings, reach Checked). The marker
    /// is the key's `KeyFormat`, which only `finalize_keygen` persists; a restoration never does.
    /// A copy of this install's data directory carries it too, and nothing persisted can tell
    /// the two apart.
    pub fn card_created_here(&self, key_id: KeyId) -> bool {
        self.card.created_here(key_id)
    }

    /// `None` for a key this install doesn't hold as a card key and has no card record of.
    /// Owner B15: on an install that didn't run the keygen, a share's status is its unverified
    /// card's (`NotChecked` while it's written, then `Unverified`), or `None` with no record.
    pub fn card_backup_status(
        &self,
        asref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> Option<CardBackupStatus> {
        let record = self.card.records.get(&(asref, share_index));
        if self.card_created_here(asref.key_id) {
            Some(record.map_or(CardBackupStatus::NoCard, CoordCardRecord::status))
        } else {
            record
                .map(CoordCardRecord::status)
                .filter(|status| *status != CardBackupStatus::NoCard)
        }
    }

    /// Whether this phone holds words 33–64 of a share's card in RAM.
    pub fn card_phone_half_in_ram(
        &self,
        asref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> bool {
        self.card
            .tmp_phone_words
            .get(&(asref, share_index))
            .is_some_and(|words| words.half.is_some())
    }

    /// The phone's record of a share's card.
    pub fn card_record(
        &self,
        asref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> Option<&CoordCardRecord> {
        self.card.records.get(&(asref, share_index))
    }

    /// A persist of card mutations failed: refuse everything card related until restart, so
    /// nothing only in memory (such as N) is ever sent.
    pub fn card_poison(&mut self) {
        self.card.poisoned = true;
    }

    /// `device_id` was unplugged: a question its card flow holds waits for the device's next
    /// `GateShown`, and the opening it asked for is gone. Owner B9: if its D check runs, the
    /// interruption is persisted and `Interrupted` asks the user whether they caused it. The
    /// app persists what this stages before the events go out.
    pub fn card_device_disconnected(&mut self, device_id: DeviceId) -> Vec<CoordinatorSend> {
        self.card.device_disconnected(device_id);
        if self.card.poisoned {
            return vec![];
        }
        let Some(mut session) = self.card.tmp_sessions.remove(&device_id) else {
            return vec![];
        };
        let mut out = CardOut {
            device_id,
            session: &mut session,
            sends: vec![],
        };
        self.card_interrupted(&mut out);
        let sends = out.sends;
        self.card.tmp_sessions.insert(device_id, session);
        sends
    }

    /// The user left `device_id`'s card flow, and the app sent the device a `Cancel`: its
    /// session is gone, as on an unplug. Owner B12: the only way out of a running check is
    /// through B9's question, so leaving it is an interruption too (review F1): it's persisted
    /// (the app persists what this stages), and the flow opened next asks whether the user
    /// caused it before anything goes to the device. A device that froze its round and made the
    /// user leave is then marked suspect on "no", rather than never.
    pub fn card_flow_left(&mut self, device_id: DeviceId) {
        self.card.device_disconnected(device_id);
        if self.card.poisoned {
            return;
        }
        let Some(session) = self.card.tmp_sessions.get(&device_id) else {
            return;
        };
        let rec = self.card_rec(session);
        let (access_structure_ref, share_index) = session.key();
        if rec.check_running() && !rec.interrupted {
            self.mutate(Mutation::Card(CardMutation::CheckInterrupted {
                access_structure_ref,
                share_index,
            }));
        }
    }

    /// Owner B9: the D check was interrupted. Persisted (once), and the user is asked whether
    /// they caused it. Outside a running check it does nothing: before N there's no D to
    /// re-roll, and after the phone round's result only the device's verdict is left.
    fn card_interrupted(&mut self, out: &mut CardOut) {
        let rec = self.card_rec(out.session);
        if !rec.check_running() {
            return;
        }
        if !rec.interrupted {
            self.card_mutate(out.session, |asref, share_index| {
                CardMutation::CheckInterrupted {
                    access_structure_ref: asref,
                    share_index,
                }
            });
        }
        out.session.gate_shown = false;
        out.session.opening = None;
        out.session.outstanding = None;
        out.event(CardEvent::Interrupted {
            resumes_left: rec.resumes_left(),
        });
    }

    /// Owner B9: whether `device_id`'s card flow waits for the user to say whether they caused
    /// the check's interruption. The flow sends the device nothing until then.
    pub fn card_interruption_pending(&self, device_id: DeviceId) -> bool {
        self.card
            .tmp_sessions
            .get(&device_id)
            .is_some_and(|session| self.card_rec(session).interruption_pending())
    }

    /// Owner R2-2: how many more "Yes, it was me" answers resume `device_id`'s check before a
    /// "yes" ends it (review B15 protocol F2: the question warns before the last one).
    pub fn card_resumes_left(&self, device_id: DeviceId) -> u8 {
        self.card
            .tmp_sessions
            .get(&device_id)
            .map_or(MAX_INTERRUPTION_RESUMES, |session| {
                self.card_rec(session).resumes_left()
            })
    }

    /// Owner B9: the user's answer to "did you cause it?". Yes: the interruption is cleared
    /// and `Resuming` tells the flow to send `Begin` again; the device must then report this
    /// check, on the same N, or the resume is an abort. No: an abort, with the device marked
    /// suspect. Owner R2-2: a "yes" after [`MAX_INTERRUPTION_RESUMES`] resumes of this check is
    /// an abort too, with the device suspect
    /// ([`TooManyInterruptions`](CardFailReason::TooManyInterruptions)). It needs no device.
    pub fn card_interruption_answer(
        &mut self,
        device_id: DeviceId,
        caused_by_user: bool,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action_with(device_id, true, |coord, out| {
            if !coord.card_rec(out.session).interruption_pending() {
                return Err(CardActionError::Stale);
            }
            if caused_by_user && coord.card_rec(out.session).resumes >= MAX_INTERRUPTION_RESUMES {
                coord.card_abort(out, CardFailReason::TooManyInterruptions);
            } else if caused_by_user {
                coord.card_mutate(out.session, |asref, share_index| {
                    CardMutation::InterruptionAnswered {
                        access_structure_ref: asref,
                        share_index,
                    }
                });
                out.session.status_due = true;
                out.event(CardEvent::Resuming);
            } else {
                coord.card_abort(out, CardFailReason::Interrupted);
            }
            Ok(())
        })
    }

    /// Starts (or re-attaches to) the card flow for `device_id`'s share of `asref`. Only
    /// `Begin` goes out from here.
    pub fn card_begin(
        &mut self,
        device_id: DeviceId,
        asref: AccessStructureRef,
        encryption_key: SymmetricKey,
        _rng: &mut (impl RngCore + CryptoRng),
    ) -> Result<CoordinatorSend, CardStartError> {
        if self.card.poisoned {
            return Err(CardStartError::Poisoned);
        }
        // Owner B15: an install that didn't run the keygen may still write the card, unverified;
        // the device says whether the share is a card share at all (`NotCardKey` otherwise).
        let unverified = !self.card_created_here(asref.key_id);
        let access_structure = self
            .get_access_structure(asref)
            .ok_or(CardStartError::UnknownKey)?;
        let share_index = *access_structure
            .device_to_share_index
            .get(&device_id)
            .ok_or(CardStartError::NoShareOnDevice)?;
        let root_shared_key = self
            .root_shared_key(asref, encryption_key)
            .ok_or(CardStartError::CouldntDecryptRootKey)?;
        let context = Context::new(
            asref.key_id.0,
            asref.access_structure_id.0,
            root_shared_key.share_image(share_index),
            access_structure.threshold() as usize,
        )
        .map_err(|error| {
            CardStartError::Metadata(match error {
                CardError::Meta(MetaError::ThresholdOne) => CardRefusal::ThresholdOne,
                _ => CardRefusal::MetadataRange,
            })
        })?;

        match self.card.tmp_sessions.get_mut(&device_id) {
            // The pinned seal, its verified proofs and a verified question carry on; the
            // question waits for the device's `GateShown`, as after any `Status`.
            Some(session) if session.key() == (asref, share_index) => {
                session.phase = SessionPhase::Pinning;
                session.device_state = None;
                session.sealed = None;
                session.seal_builder = None;
                session.unpinned_seal = None;
                session.awaiting_phone_half = None;
                session.opening = None;
                session.gate_shown = false;
                session.status_due = true;
                session.link_refused = false;
                session.unverified = unverified;
            }
            _ => {
                let record = self.card.records.get(&(asref, share_index));
                let interrupted =
                    record.is_some_and(|record| record.check_running() && !record.interrupted);
                let pinned = record.and_then(|record| {
                    let pin = record.pin.as_ref()?;
                    Some(PinnedSeal::from_record(
                        pin.seal(),
                        &context,
                        record.verified,
                    ))
                });
                self.card.tmp_sessions.insert(
                    device_id,
                    CoordCardSession {
                        asref,
                        share_index,
                        context,
                        phase: SessionPhase::Pinning,
                        device_state: None,
                        sealed: None,
                        seal_builder: None,
                        unpinned_seal: None,
                        pinned,
                        awaiting_phone_half: None,
                        proof_strike: false,
                        opening: None,
                        current: None,
                        gate_shown: false,
                        outstanding: None,
                        status_due: true,
                        proofs_this_session: Bits::default(),
                        link_refused: false,
                        port_shared: false,
                        unverified,
                        unverified_confirmed: false,
                    },
                );
                // Owner B9: a new session on a check that runs means the app was closed (or its
                // flow lost) mid-check, and nothing told it the device stayed: the user is asked.
                if interrupted {
                    self.mutate(Mutation::Card(CardMutation::CheckInterrupted {
                        access_structure_ref: asref,
                        share_index,
                    }));
                }
            }
        }

        Ok(to_device(
            device_id,
            CoordinatorCard::Begin(Box::new(CardBegin {
                share_index,
                root_shared_key,
                format_id: FORMAT_ID,
                proof_version: PROOF_VERSION,
            })),
        ))
    }

    /// Whether `device_id`'s card flow has N recorded and the check hasn't ended: from now on
    /// there's no Stop (owner B12), and a flow can start without the device, for the question
    /// an interruption asks (owner B9).
    pub fn card_check_started(&self, device_id: DeviceId) -> bool {
        self.card
            .tmp_sessions
            .get(&device_id)
            .is_some_and(|session| self.card_rec(session).n_out())
    }

    /// Both halves of `device_id`'s card are recorded on this phone (design §3.10): the app's
    /// backup run is complete.
    pub fn card_halves_recorded(&self, device_id: DeviceId) -> bool {
        self.card
            .tmp_sessions
            .get(&device_id)
            .is_some_and(|session| self.card_rec(session).halves_recorded())
    }

    /// [`card_check_started`](Self::card_check_started) for a share, from its persisted record
    /// alone: no card flow is needed. An app starts a flow without the device (or with firmware
    /// that isn't the app's) only then.
    pub fn card_check_started_for(
        &self,
        asref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> bool {
        self.card
            .records
            .get(&(asref, share_index))
            .is_some_and(CoordCardRecord::n_out)
    }

    /// Why a share's failed card failed, and whether N had gone out: what its alarm was built
    /// from, so the backup checklist can repeat the alarm's next step. `None` unless it failed.
    pub fn card_failure(
        &self,
        asref: AccessStructureRef,
        share_index: ShareIndex,
    ) -> Option<(CardFailReason, bool)> {
        let rec = self.card.records.get(&(asref, share_index))?;
        Some((rec.fail_reason()?, rec.after_n()))
    }

    /// The hold "I've written words 1–32" on the phone (owner B1), offered once the device's
    /// quiz on them passed (`ConfirmDeviceHalf`): `DeviceHalfWritten` is persisted, then sent
    /// with the pinned h_C, and the device starts the bit proofs; the phone pulls them as they're
    /// computed and shows words 33–64. Again (an app restart, say) it sends the confirm and pulls
    /// the missing proofs. The device refuses it until its own quiz passed, so neither side alone
    /// starts the proofs.
    ///
    /// Owner B8: with the confirm, N is drawn (once) and persisted, and goes to the device right
    /// after it, so the device computes the openings of D while words 33–64 are written.
    pub fn card_device_half_written(
        &mut self,
        device_id: DeviceId,
        rng: &mut (impl RngCore + CryptoRng),
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            if out.session.unverified {
                return coord.card_unverified_confirm(out);
            }
            let rec = coord.card_rec(out.session);
            if rec.pin.is_none() {
                return Err(CardActionError::WrongState("the seal isn't pinned"));
            }
            if !rec.finished {
                return Err(CardActionError::WrongState(
                    "the device's quiz on words 1–32 isn't done",
                ));
            }
            if rec.in_final_check() || rec.status() == CardBackupStatus::Failed {
                return Err(CardActionError::WrongState("the check is past this step"));
            }
            if !rec.device_half_written {
                coord.card_mutate(out.session, |asref, share_index| {
                    CardMutation::DeviceHalfWritten {
                        access_structure_ref: asref,
                        share_index,
                    }
                });
            }
            if rec.n.is_none() {
                let mut n = [0u8; 32];
                rng.fill_bytes(&mut n);
                coord.card_mutate(out.session, |asref, share_index| {
                    CardMutation::CheckStarted {
                        access_structure_ref: asref,
                        share_index,
                        n: SecretN(n),
                    }
                });
                // Owner B12: no Stop from here on.
                out.event(CardEvent::CheckStarted);
            }
            // It sends the confirm with the pinned h_C, unless the device is proving already.
            coord.card_phone_stage(out, false);
            Ok(())
        })
    }

    /// The phone's [Continue] to the final check (owner B1): only with both halves recorded and
    /// every bit proof verified. It persists the pending device round on the N sent after the
    /// confirm (owner B8; it needs the device's coin recorded), then sends `FinalBegin`. A record
    /// from before B8 with no N draws one here. A second call sends `FinalBegin` again.
    pub fn card_final_begin(
        &mut self,
        device_id: DeviceId,
        rng: &mut (impl RngCore + CryptoRng),
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            let rec = coord.card_rec(out.session);
            if !rec.proven() {
                return Err(CardActionError::WrongState("the seal isn't proven yet"));
            }
            if !rec.device_half_written || !rec.halves_recorded() {
                return Err(CardActionError::WrongState(
                    "both halves of the card aren't checked yet",
                ));
            }
            if rec.round.is_some()
                || rec.device_passed.is_some()
                || rec.status() == CardBackupStatus::Failed
            {
                return Err(CardActionError::WrongState("the check is past this step"));
            }
            // Owner B8: the device took N (its coin is recorded) before the device round runs.
            if rec.n.is_some() && rec.drawn.is_none() {
                return Err(CardActionError::WrongState(
                    "the device hasn't taken the check's number yet",
                ));
            }
            match rec.device_round {
                None => {
                    let mut n = rec.n.unwrap_or_default();
                    if rec.n.is_none() {
                        rng.fill_bytes(&mut n);
                    }
                    coord.card_mutate(out.session, |asref, share_index| {
                        CardMutation::DeviceRoundPending {
                            access_structure_ref: asref,
                            share_index,
                            n: SecretN(n),
                        }
                    });
                }
                Some(DeviceRoundRecord::Pending) => {}
                Some(_) => {
                    return Err(CardActionError::WrongState("the device round is over"));
                }
            }
            coord.card_send_final_begin(out);
            Ok(())
        })
    }

    /// `FinalBegin` for the recorded pending round. It's a notify: the device's `DeviceRound`
    /// comes when the user finishes the round, and answers no request.
    fn card_send_final_begin(&mut self, out: &mut CardOut) {
        out.session.phase = SessionPhase::DeviceRound;
        out.notify(CoordinatorCard::FinalBegin);
        out.event(CardEvent::DeviceRound);
    }

    /// The page of words 33–64 to show: 0..=10 three words each (the last two), 11 all 32. Only
    /// after the user confirmed words 1–32 (owner B1). It needs no device: it reads only the
    /// words in RAM.
    pub fn card_phone_page(
        &mut self,
        device_id: DeviceId,
        page: u8,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        if page > PHONE_PAGES {
            return Err(CardActionError::WrongState("no such page"));
        }
        self.card_action(device_id, |coord, out| {
            if !coord.card_phone_half_next(out.session) {
                return Err(CardActionError::WrongState("the phone's words aren't next"));
            }
            let words = coord
                .card
                .tmp_phone_words
                .get_mut(&out.session.key())
                .filter(|words| words.half.is_some())
                .ok_or(CardActionError::WrongState("no words yet"))?;
            if words.written {
                return Err(CardActionError::WrongState("the words are written"));
            }
            words.page = page;
            Self::card_page_event(out, words);
            Ok(())
        })
    }

    /// The page in `words.page`.
    fn card_page_event(out: &mut CardOut, words: &PhoneWords) {
        let Some(half) = words.half.as_ref() else {
            return;
        };
        let range = page_words(words.page);
        out.event(CardEvent::PhonePage {
            page: words.page,
            first_word: (range.start + HALF + 1) as u8,
            words: half[range].iter().map(|&y| word(y)).collect(),
        });
    }

    /// The hold "I've written words 33–64": the phone's quiz on them starts. It needs no
    /// device.
    pub fn card_phone_half_written(
        &mut self,
        device_id: DeviceId,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            if !coord.card_phone_half_next(out.session) {
                return Err(CardActionError::WrongState("the phone's words aren't next"));
            }
            let words = coord
                .card
                .tmp_phone_words
                .get_mut(&out.session.key())
                .ok_or(CardActionError::WrongState("no words yet"))?;
            let half = words
                .complete()
                .ok_or(CardActionError::WrongState("the words aren't in"))?;
            words.written = true;
            words.self_check = Some(SelfCheck {
                items: PHONE_HALF
                    .zip(half)
                    .map(|(j, y)| (j as u8 + 1, y))
                    .collect(),
                k: 0,
                recheck: false,
            });
            Self::card_self_check_question(out, words, false);
            Ok(())
        })
    }

    fn card_self_check_question(out: &mut CardOut, words: &PhoneWords, wrong: bool) {
        let Some(check) = words.self_check.as_ref() else {
            return;
        };
        let Some(&(word_number, y)) = check.items.get(check.k) else {
            return;
        };
        let [a, b] = closest_lookalikes(y).expect("a verified word is below 2048");
        out.event(CardEvent::SelfCheckQuestion {
            k: check.k as u8,
            total: check.items.len() as u8,
            word_number,
            word: word(y),
            lookalikes: [word(a), word(b)],
            wrong,
            recheck: check.recheck,
        });
    }

    /// One pick in the phone's quiz (a word index). A wrong pick asks the same item again; after
    /// the last right pick the phone's half is checked: `Checked{h32}` is persisted, or in a
    /// later check, `RecheckPassed`. It needs no device.
    pub fn card_self_check_answer(
        &mut self,
        device_id: DeviceId,
        pick: u16,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            let key = out.session.key();
            let words = coord
                .card
                .tmp_phone_words
                .get_mut(&key)
                .ok_or(CardActionError::WrongState("no quiz is running"))?;
            let check = words
                .self_check
                .as_mut()
                .ok_or(CardActionError::WrongState("no quiz is running"))?;
            let &(_, y) = check
                .items
                .get(check.k)
                .ok_or(CardActionError::WrongState("the quiz is over"))?;
            if pick != y {
                Self::card_self_check_question(out, words, true);
                return Ok(());
            }
            check.k += 1;
            if check.k < check.items.len() {
                Self::card_self_check_question(out, words, false);
                return Ok(());
            }
            let recheck = check.recheck;
            let h_c = words.h_c;
            let half = words.complete();
            words.self_check = None;
            if recheck {
                out.event(CardEvent::RecheckPassed);
                return Ok(());
            }
            if out.session.unverified {
                // Owner B15: written, not verified. The words leave RAM: nothing checks them.
                if let Some(mut half) = half {
                    crate::device::card::wipe(&mut half, [0; HALF]);
                }
                coord.card_mutate(out.session, |asref, share_index| {
                    CardMutation::UnverifiedWritten {
                        access_structure_ref: asref,
                        share_index,
                    }
                });
                coord.card.tmp_phone_words.remove(&key);
                out.session.phase = SessionPhase::Closed;
                out.event(CardEvent::UnverifiedWritten);
                return Ok(());
            }
            let Some(mut half) = half else {
                return Err(CardActionError::WrongState("not every word is in"));
            };
            let words_hash = phone::h32(&h_c, &half);
            crate::device::card::wipe(&mut half, [0; HALF]);
            coord.card_mutate(out.session, |asref, share_index| CardMutation::Checked {
                access_structure_ref: asref,
                share_index,
                words_hash,
            });
            out.session.phase = SessionPhase::Finished;
            out.event(CardEvent::PhoneSelfChecked);
            // [Continue] waits for the last proofs, if the device is still computing them.
            let rec = coord.card_rec(out.session);
            if rec.proven() {
                out.event(CardEvent::ReadyForFinalCheck);
            } else {
                out.event(coord.card_proving_event(out.session));
            }
            Ok(())
        })
    }

    /// A later check of a PASSED card (design §3.9): `Recheck` with this phone's h37 when it
    /// holds the card as a two-key pass, or none from a phone with no record of it, in which
    /// case only the device's half is checked.
    pub fn card_recheck(
        &mut self,
        device_id: DeviceId,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            if out.session.device_state != Some(CardStateReport::Passed) {
                return Err(CardActionError::WrongState("the card hasn't passed"));
            }
            if !coord.card_rec(out.session).may_recheck() {
                return Err(CardActionError::WrongState(
                    "this phone hasn't finished checking this card",
                ));
            }
            coord.card_start_recheck(out);
            Ok(())
        })
    }

    fn card_start_recheck(&mut self, out: &mut CardOut) {
        let words_hash = self.card_rec(out.session).words_hash;
        out.session.phase = SessionPhase::Recheck;
        out.request(CoordinatorCard::Recheck { words_hash });
        out.event(CardEvent::RecheckWaiting);
        if words_hash.is_none() {
            out.event(CardEvent::RecheckUnavailable);
        }
    }

    /// The user's answer to the current phone-round question.
    pub fn card_answer(
        &mut self,
        device_id: DeviceId,
        answer: BoxAnswer,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            let (k, j, y) = out
                .session
                .current
                .filter(|_| out.session.gate_shown)
                .ok_or(CardActionError::WrongState("no question is open"))?;
            let rec = coord.card_rec(out.session);
            let (Some(n), Some((_, d)), None) = (rec.n, rec.drawn, rec.round) else {
                return Err(CardActionError::WrongState("no phone round is running"));
            };
            // "None of these", or any other word, is a miss.
            let correct = answer == BoxAnswer::Picked(y);
            if correct {
                coord.card_mutate(out.session, |asref, share_index| {
                    CardMutation::BoxAnswered {
                        access_structure_ref: asref,
                        share_index,
                        j,
                        correct: true,
                    }
                });
                out.notify(CoordinatorCard::BoxAnswered { j });
                out.session.current = None;
                match d.get(k as usize + 1) {
                    // Owner B8: its opening was verified ahead, or is on its way.
                    Some(_) => {
                        let rec = coord.card_rec(out.session);
                        coord.card_next_box(out, &rec, n, d);
                    }
                    None => {
                        let words_hash = coord.card_round_hash(out.session, &d);
                        coord.card_mutate(out.session, |asref, share_index| {
                            CardMutation::RoundResult {
                                access_structure_ref: asref,
                                share_index,
                                pass: true,
                                reason: None,
                                words_hash,
                            }
                        });
                        out.request(CoordinatorCard::PhoneRound { n, pass: true });
                        out.event(CardEvent::RoundSent {
                            answered: CHECK_BOXES as u8,
                        });
                    }
                }
            } else if rec.misses & (1 << k) != 0 {
                coord.card_abort(out, CardFailReason::TwoMisses);
            } else {
                coord.card_mutate(out.session, |asref, share_index| {
                    CardMutation::BoxAnswered {
                        access_structure_ref: asref,
                        share_index,
                        j,
                        correct: false,
                    }
                });
                out.event(CardEvent::TryAgain { k });
                out.event(question(k, j, y, true));
            }
            Ok(())
        })
    }

    /// h37 over the words this phone was given, when they're all in RAM.
    fn card_round_hash(
        &self,
        session: &CoordCardSession,
        d: &[u8; CHECK_BOXES],
    ) -> Option<[u8; 32]> {
        let words = self.card.tmp_phone_words.get(&session.key())?;
        let mut half = words.complete()?;
        let mut d_words = [0u16; CHECK_BOXES];
        for (out, word) in d_words.iter_mut().zip(words.d_words) {
            *out = word?;
        }
        let hash = phone::h37(&words.h_c, &half, d, &d_words);
        crate::device::card::wipe(&mut half, [0; HALF]);
        crate::device::card::wipe(&mut d_words, [0; CHECK_BOXES]);
        Some(hash)
    }

    /// What the app sees of `device_id`'s USB port: whether another device is on it too. The
    /// app says it before each of the device's card messages. A device alone on its port is
    /// plugged straight into the phone, so its `NotDirectUsb` after N is a lie (re-attack
    /// RA-1). The app can't tell which device on a shared port is first (the order it heard
    /// their announcements in isn't the chain's), so a shared port is all it says.
    pub fn card_port_shared(&mut self, device_id: DeviceId, shared: bool) {
        if let Some(session) = self.card.tmp_sessions.get_mut(&device_id) {
            session.port_shared = shared;
        }
    }

    /// What `device_id` owes the phone while its D check runs (owner B9), and how far it has
    /// got with it (proofs verified, or openings verified and answered): the protocol gives the
    /// work a deadline from when this last changed. `None` while nothing is owed: before N,
    /// once the round has a result, while an interruption waits for the user, or while the user
    /// is the one to act, or while the device is on a link that refused the flow (`NotDirectUsb`).
    pub fn card_device_work(&self, device_id: DeviceId) -> Option<(CardDeviceWork, u16)> {
        let session = self.card.tmp_sessions.get(&device_id)?;
        let rec = self.card_rec(session);
        if !rec.check_running() || rec.interrupted || session.link_refused {
            return None;
        }
        if session.status_due {
            return Some((CardDeviceWork::Status, 0));
        }
        if session.awaiting_phone_half.is_some()
            || matches!(
                session.outstanding,
                Some(CoordinatorCard::PullPhoneHalf { .. })
            )
        {
            return Some((CardDeviceWork::PhoneHalf, 0));
        }
        if rec.drawn.is_none() {
            return Some((CardDeviceWork::Coin, 0));
        }
        if !rec.proven() {
            let verified = session
                .pinned
                .as_ref()
                .map_or(0, |pinned| pinned.verified().len() as u16);
            return Some((CardDeviceWork::Proofs, verified));
        }
        if session.opening.is_some() {
            let verified = self
                .card
                .tmp_phone_words
                .get(&session.key())
                .map_or(0, |words| words.d_words.iter().flatten().count() as u16);
            let proofs_first = session.device_state == Some(CardStateReport::Proving)
                && session.proofs_this_session.len() < COMMITTED;
            return Some((
                CardDeviceWork::Opening { proofs_first },
                verified + rec.answered.count_ones() as u16,
            ));
        }
        if rec.device_round == Some(DeviceRoundRecord::Passed) && !session.gate_shown {
            return Some((CardDeviceWork::Gate, 0));
        }
        None
    }

    /// The phone's deadline after N (review S3, owner B9): the device didn't do the work it owed
    /// ([`card_device_work`](Self::card_device_work)) in time, so the phone aborts as for any
    /// other end after N, and the device is suspect. The protocol keeps the clock; this refuses
    /// unless the device still owes work.
    pub fn card_device_timeout(
        &mut self,
        device_id: DeviceId,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        if self.card_device_work(device_id).is_none() {
            return Err(CardActionError::WrongState("the device owes nothing now"));
        }
        self.card_action(device_id, |coord, out| {
            coord.card_abort(out, CardFailReason::PhoneTimeout);
            Ok(())
        })
    }

    /// Handles a card message from a device.
    pub(crate) fn recv_card_message(
        &mut self,
        from: DeviceId,
        message: DeviceCard,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        let kind = message.kind();
        // A card restore's frames go to its page, before any card flow: they stage nothing.
        let restore = match message {
            DeviceCard::RestoreState {
                device_half_in,
                phone_half_in,
                decoding,
            } => Some(CardRestoreEvent::State {
                device_half_in,
                phone_half_in,
                decoding,
            }),
            DeviceCard::RestoreOutcome { result } => Some(CardRestoreEvent::Outcome { result }),
            _ => None,
        };
        if let Some(event) = restore {
            return Ok(vec![CoordinatorSend::ToUser(
                CoordinatorToUserMessage::CardRestore(CardRestoreToUser {
                    device_id: from,
                    event,
                }),
            )]);
        }
        let Some(mut session) = self.card.tmp_sessions.remove(&from) else {
            // A refusal outside a card flow, such as the device refusing a card restore or the
            // 25-word display of a card-restored share: the protocol that asked is told.
            if let DeviceCard::Refused { reason } = message {
                return Ok(vec![CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::CardRefused {
                        device_id: from,
                        reason,
                    },
                )]);
            }
            return Err(Error::coordinator_invalid_message(
                kind,
                "no card flow with this device",
            ));
        };
        let mut out = CardOut {
            device_id: from,
            session: &mut session,
            sends: vec![],
        };
        let result = if self.card.poisoned {
            out.event(CardEvent::StorageError);
            Ok(())
        } else {
            // Every reply but `Busy` answers the outstanding request, and a handler that needs
            // more asks again: so a late `Busy` never re-sends what was answered. `GateShown`,
            // `CheckDone` and `DeviceRound` answer nothing. An `Err` changes nothing, so the
            // request is put back.
            let reply = !matches!(
                message,
                DeviceCard::Refused {
                    reason: CardRefusal::Busy
                } | DeviceCard::GateShown
                    | DeviceCard::CheckDone
                    | DeviceCard::DeviceRound { .. }
                    | DeviceCard::RecheckDone
            );
            let asked = if reply {
                out.session.outstanding.take()
            } else {
                out.session.outstanding.clone()
            };
            let result = self.card_handle(&mut out, message, asked.as_ref());
            if result.is_err() {
                out.session.outstanding = asked;
            }
            result.map_err(|reason| Error::coordinator_invalid_message(kind, reason))
        };
        let sends = out.sends;
        self.card.tmp_sessions.insert(from, session);
        result.map(|()| sends)
    }

    /// The device-message table, given the request the message answers. `Err` is only for a
    /// message that's unexpected or malformed; it changes nothing.
    fn card_handle(
        &mut self,
        out: &mut CardOut,
        message: DeviceCard,
        asked: Option<&CoordinatorCard>,
    ) -> Result<(), &'static str> {
        // Owner B9: until the user says whether they caused the interruption, no device frame is
        // taken, so a device can't resume the check by itself.
        if self.card_rec(out.session).interruption_pending() {
            return Err("the check waits for the user's answer about the interruption");
        }
        // The device's verdict is recorded, so the check is over (spec §0.7.4.2): no device
        // frame records anything again. A `Status` or a repeated `Verdict` shows the phone's own
        // record, a refusal records nothing, and a later check's frames only quiz. The
        // unsolicited frames are dropped, and any other reply is unexpected.
        if self.card_rec(out.session).device_passed.is_some() {
            match message {
                DeviceCard::Status(_)
                | DeviceCard::Verdict { .. }
                | DeviceCard::Refused { .. }
                | DeviceCard::RecheckWords { .. }
                | DeviceCard::RecheckDone => {}
                DeviceCard::GateShown
                | DeviceCard::CheckDone
                | DeviceCard::DeviceRound { .. }
                | DeviceCard::PhoneHalf(_) => {
                    event!(
                        Level::DEBUG,
                        kind = message.kind(),
                        "ignoring a frame after the verdict"
                    );
                    return Ok(());
                }
                _ => return Err("the check is over"),
            }
        }
        match message {
            // Only as the reply to a `Begin` (audit F1b): a device's report of its state is never
            // taken unasked.
            DeviceCard::Status(status) => {
                if !out.session.status_due {
                    return Err("a status no Begin asked for");
                }
                out.session.status_due = false;
                let result = self.card_status(out, *status);
                if result.is_err() {
                    out.session.status_due = true;
                }
                result
            }
            DeviceCard::CommitPoints { from, points } => {
                let builder = out
                    .session
                    .seal_builder
                    .as_mut()
                    .ok_or("points weren't asked for")?;
                builder.add(from, &points).map_err(|_| "bad points frame")?;
                if let Some((from, count)) = builder.next_missing() {
                    out.request(CoordinatorCard::PullCommit { from, count });
                    out.event(CardEvent::Pinning { points: from });
                    return Ok(());
                }
                // Checked before the builder is taken, so an `Err` changes nothing.
                let (sealed, _) = out.session.sealed.ok_or("no status")?;
                out.session.device_state.ok_or("no status")?;
                let builder = out.session.seal_builder.take().expect("just used");
                let seal = builder
                    .finish(sealed.coin_commit, sealed.h_c)
                    .map_err(|_| "seal incomplete")?;
                // The pin needs words 33–64: h_C binds their hash (binding (1), spec PK.6).
                out.session.unpinned_seal = Some(seal);
                out.request(CoordinatorCard::PullPhoneHalf { h_c: sealed.h_c });
                out.event(CardEvent::Pinning {
                    points: COMMITTED as u16,
                });
                Ok(())
            }
            DeviceCard::PhoneHalf(words) => {
                if !matches!(asked, Some(CoordinatorCard::PullPhoneHalf { .. })) {
                    return Err("words 33–64 weren't asked for");
                }
                self.card_phone_half(out, &words.0)
            }
            DeviceCard::CheckDone if out.session.unverified => {
                // Owner B15: the device's quiz on words 1–32 of the unverified card passed.
                if self.card_rec(out.session).unverified_h_c.is_none() {
                    return Err("no unverified card is being written");
                }
                out.session.device_state = Some(CardStateReport::CardChecked);
                self.card_unverified_stage(out, false);
                Ok(())
            }
            DeviceCard::CheckDone => {
                if out.session.pinned.is_none() {
                    return Err("the seal isn't pinned");
                }
                // A device sends it only on reaching CARD_CHECKED, so it can't follow the final
                // check's start. Once the phone holds N it's a device below CHECKING: an abort,
                // as for its `Status`.
                let rec = self.card_rec(out.session);
                if rec.n.is_some() {
                    self.card_abort(out, CardFailReason::Refused(CardRefusal::WrongState));
                    return Ok(());
                }
                if rec.device_round.is_some() {
                    return Err("the card check is over");
                }
                out.session.device_state = Some(CardStateReport::CardChecked);
                self.card_finished(out, false);
                Ok(())
            }
            DeviceCard::BitProofs { from, proofs } => {
                // Only a reply to a pull: unasked frames, such as one after the verdict, can't
                // fail the card.
                if !matches!(asked, Some(CoordinatorCard::PullProofs { .. })) {
                    return Err("proofs weren't asked for");
                }
                let pinned = out.session.pinned.as_mut().ok_or("the seal isn't pinned")?;
                let count = proofs.len() as u8;
                match card_setup().verify_commit_proofs(pinned, from, &proofs, &mut Ops::default())
                {
                    Ok(()) => {
                        // Proofs are served in PROVING only.
                        if out.session.device_state == Some(CardStateReport::CardChecked) {
                            out.session.device_state = Some(CardStateReport::Proving);
                        }
                        for v in from..from.saturating_add(count as u16) {
                            if (v as usize) < COMMITTED {
                                out.session.proofs_this_session.set(v as usize, true);
                            }
                        }
                        self.card_mutate(out.session, |asref, share_index| {
                            CardMutation::ProofsVerified {
                                access_structure_ref: asref,
                                share_index,
                                from,
                                count,
                            }
                        });
                        self.card_pull_proofs(out);
                    }
                    // A bad frame is pulled once more; a second bad frame from this device in the
                    // session, whatever range it names, fails the card.
                    Err(_) if !out.session.proof_strike => {
                        out.session.proof_strike = true;
                        let pull = match asked {
                            Some(&CoordinatorCard::PullProofs { from, count }) => (from, count),
                            _ => (from, PROOFS_PER_FRAME as u8),
                        };
                        out.request(CoordinatorCard::PullProofs {
                            from: pull.0,
                            count: pull.1,
                        });
                    }
                    // Words 1–32 are written by now: a device fault, not another card (C16).
                    Err(_) => self.card_device_fault(out, CardFailReason::BadProof),
                }
                Ok(())
            }
            DeviceCard::OpenAccepted { coin } => {
                let rec = self.card_rec(out.session);
                let (Some(n), Some(pin), None) = (rec.n, rec.pin.as_ref(), rec.round) else {
                    return Err("no N was sent");
                };
                let mut ops = Ops::default();
                if !proof::coin_matches(&pin.coin_commit, &coin, &mut ops) {
                    self.card_abort(out, CardFailReason::CoinMismatch);
                    return Ok(());
                }
                let d = proof::derive_boxes(&pin.h_c, &n, &coin, &mut ops);
                match rec.drawn {
                    Some(drawn) if drawn != (coin, d) => {
                        self.card_abort(out, CardFailReason::BoxesMismatch);
                        return Ok(());
                    }
                    Some(_) => {}
                    None => self.card_mutate(out.session, |asref, share_index| {
                        CardMutation::BoxesDrawn {
                            access_structure_ref: asref,
                            share_index,
                            coin,
                            d,
                        }
                    }),
                }
                let rec = self.card_rec(out.session);
                // Owner B8: N taken in PROVING. The proofs are pulled now, and once they're all
                // verified the openings of D.
                if rec.device_round.is_none() {
                    self.card_pull_proofs(out);
                    return Ok(());
                }
                out.session.device_state = Some(CardStateReport::Checking);
                self.card_next_box(out, &rec, n, d);
                Ok(())
            }
            // Every word of D is a computed device word, opened the one way (owner B6).
            DeviceCard::OpeningPart(part) => self.card_round_opening_part(out, part),
            DeviceCard::Verdict { passed } => {
                let rec = self.card_rec(out.session);
                match (rec.round, rec.n) {
                    // A verdict recorded already is the one shown.
                    _ if rec.device_passed.is_some() => self.card_device_verdict(out, passed),
                    (Some(_), _) => self.card_device_verdict(out, passed),
                    (None, Some(_)) => self.card_abort(out, CardFailReason::UnexpectedVerdict),
                    (None, None) if passed => return Err("a verdict without a check"),
                    // The verdict of a device-round fail, already shown: recorded only.
                    (None, None) if rec.failed == Some(CardFailReason::DeviceRoundFailed) => {
                        self.card_record_device_fail(out);
                    }
                    // The verdict of a phone Stop before N, in a record of a pre-release build
                    // (owner follow-up 5 to B12: the phone has no Stop now).
                    (None, None) if rec.failed == Some(CardFailReason::Stopped) => {
                        self.card_ended_before_n(out, CardFailReason::Stopped);
                    }
                    // The device's own Stop in a device round of a pre-release build, before N
                    // (owner B8, B12: the device round runs after N, with no Stop).
                    (None, None)
                        if rec.failed.is_none()
                            && rec.device_round == Some(DeviceRoundRecord::Pending) =>
                    {
                        self.card_ended_before_n(out, CardFailReason::Stopped);
                    }
                    (None, None) => return Err("a verdict without a check"),
                }
                Ok(())
            }
            DeviceCard::GateShown => {
                let rec = self.card_rec(out.session);
                // Another phone's check, say: nothing of this phone's waits for it. Owner B8: N
                // goes out before the device round, so a gate before its pass is ignored.
                if rec.n.is_none() || rec.device_round != Some(DeviceRoundRecord::Passed) {
                    return Ok(());
                }
                let shown = core::mem::replace(&mut out.session.gate_shown, true);
                if shown || rec.round.is_some() {
                    return Ok(());
                }
                out.event(CardEvent::GateShown);
                // A verified question waiting for the gate is asked now.
                if let Some((k, j, y)) = out.session.current {
                    if rec.first_unanswered() == Some(k) {
                        out.event(question(k, j, y, rec.misses & (1 << k) != 0));
                    }
                }
                Ok(())
            }
            DeviceCard::Refused { reason } => {
                let rec = self.card_rec(out.session);
                if out.session.phase == SessionPhase::Recheck {
                    match reason {
                        CardRefusal::Busy => {}
                        CardRefusal::NoRecord => out.event(CardEvent::RecheckUnavailable),
                        CardRefusal::NoSession => {
                            out.session.status_due = true;
                            out.event(CardEvent::NeedsBegin)
                        }
                        _ => out.event(CardEvent::Refused { reason }),
                    }
                    if reason != CardRefusal::Busy {
                        return Ok(());
                    }
                }
                match reason {
                    // Only while the request is outstanding: a late `Busy` for one already
                    // answered is dropped. An `OpenBox` is never re-sent: the device sends the
                    // opening by itself once it's ready and its gate screen is up, so a re-send
                    // could only arrive after that and spend another run.
                    CardRefusal::Busy => {
                        // A pull of proofs refused by a device still in CARD_CHECKED: the confirm
                        // was lost on its way, so it goes again before the pull is re-sent (a
                        // device in PROVING ignores a repeat).
                        // Owner B8: so is an `OpenStart` that follows the confirm.
                        if matches!(
                            asked,
                            Some(
                                CoordinatorCard::PullProofs { .. }
                                    | CoordinatorCard::OpenStart { .. }
                            )
                        ) && out.session.device_state == Some(CardStateReport::CardChecked)
                            && rec.device_half_written
                        {
                            if let Some(h_c) = rec.pin.as_ref().map(|pin| pin.h_c) {
                                out.notify(CoordinatorCard::DeviceHalfWritten { h_c });
                            }
                        }
                        if let Some(resend) = asked
                            .filter(|asked| !matches!(asked, CoordinatorCard::OpenBox { .. }))
                            .cloned()
                        {
                            out.event(CardEvent::Busy {
                                resend: Box::new(CoordinatorToDeviceMessage::Card(resend)),
                            });
                        }
                    }
                    // The protocol sends `Begin` again, and its `Status` is due. Owner B9: while
                    // the D check runs, a device that lost its session without an unplug the app
                    // saw is an interruption, and the user is asked first.
                    CardRefusal::NoSession => {
                        out.session.status_due = true;
                        if rec.check_running() {
                            self.card_interrupted(out);
                        } else {
                            out.event(CardEvent::NeedsBegin)
                        }
                    }
                    // Review P2: a link that can't run the flow, not a stall. The `Status` stays
                    // due for the next `Begin`, but no deadline runs on it meanwhile.
                    // Re-attack RA-1: an honest device sends it only as its reply to `Begin`, and
                    // only downstream of another device. So it pauses the deadline only while
                    // a `Begin` waits for its reply and the app sees another device on this
                    // device's port. Otherwise it's the refusal it says it is: after N an abort,
                    // with the device suspect, so a chip can't use it to stall unblamed.
                    CardRefusal::NotDirectUsb
                        if out.session.status_due && out.session.port_shared =>
                    {
                        out.session.link_refused = true;
                        out.event(CardEvent::Refused { reason })
                    }
                    _ if rec.n.is_some() && rec.device_passed.is_none() => {
                        self.card_abort(out, CardFailReason::Refused(reason))
                    }
                    CardRefusal::AlreadyUsed => out.event(CardEvent::StartedOnAnotherPhone),
                    _ => out.event(CardEvent::Refused { reason }),
                }
                Ok(())
            }
            DeviceCard::DeviceRound { pass } => {
                let rec = self.card_rec(out.session);
                // Only for this phone's pending round of a check that runs; anything else is stale
                // (review P4: never on a check that ended).
                if rec.device_round != Some(DeviceRoundRecord::Pending)
                    || rec.failed.is_some()
                    || rec.round.is_some()
                {
                    return Ok(());
                }
                if pass {
                    self.card_device_round_passed(out);
                } else {
                    self.card_mutate(out.session, |asref, share_index| {
                        CardMutation::DeviceRoundResult {
                            access_structure_ref: asref,
                            share_index,
                            pass: false,
                        }
                    });
                    // Owner B8: the chip knew D, so a failed round is an abort, and the device
                    // is suspect. A chip that saw D can't fail the round to have the phone
                    // blamed and the card drawn again.
                    if rec.n.is_some() {
                        self.card_abort(out, CardFailReason::DeviceRoundFailed);
                        return Ok(());
                    }
                    // Before N (a pre-release build's record): no suspect, as an honest device
                    // fails the round when the user misreads.
                    self.card_fail(out, CardFailReason::DeviceRoundFailed);
                }
                Ok(())
            }
            DeviceCard::RecheckWords {
                phone_half,
                d_words,
            } => {
                if out.session.phase != SessionPhase::Recheck {
                    return Err("no later check is running");
                }
                self.card_recheck_words(out, &phone_half, d_words.as_ref());
                Ok(())
            }
            DeviceCard::RecheckDone => {
                if out.session.phase == SessionPhase::Recheck {
                    out.event(CardEvent::RecheckDeviceDone);
                }
                Ok(())
            }
            // Restore frames are routed before any card flow.
            DeviceCard::RestoreOutcome { .. } | DeviceCard::RestoreState { .. } => Ok(()),
        }
    }

    /// A later check's words from the device: the phone checks words 33–64 against the pinned
    /// H(P) (binding (1)) and all of them against its own hash again, then quizzes on them,
    /// words 33–64 then D's in `derive_boxes` order.
    fn card_recheck_words(
        &mut self,
        out: &mut CardOut,
        phone_half: &CardWords32,
        d_words: Option<&CardWords5>,
    ) {
        let rec = self.card_rec(out.session);
        // Only a phone that holds the card as a two-key pass quizzes its words again, so only
        // it can reach `RecheckPassed`, the app's "Backup checked" (audit F1).
        let (Some(pin), Some(words_hash), CardBackupStatus::Checked) =
            (rec.pin.as_ref(), rec.words_hash, rec.status())
        else {
            out.event(CardEvent::RecheckUnavailable);
            return;
        };
        let pinned = out.session.pinned.as_ref().is_some_and(|pinned| {
            pinned
                .check_phone_half(&phone_half.0, &mut Ops::default())
                .is_ok()
        });
        if !pinned {
            out.event(CardEvent::RecheckUnavailable);
            return;
        }
        let mut items: Vec<(u8, u16)> = PHONE_HALF
            .zip(phone_half.0)
            .map(|(j, y)| (j as u8 + 1, y))
            .collect();
        let hash = match (d_words, rec.drawn) {
            (Some(d_words), Some((_, d))) => {
                items.extend(d.iter().zip(d_words.0).map(|(&j, y)| (j + 1, y)));
                phone::h37(&pin.h_c, &phone_half.0, &d, &d_words.0)
            }
            (None, _) => phone::h32(&pin.h_c, &phone_half.0),
            (Some(_), None) => [0; 32],
        };
        let valid = items.iter().all(|&(_, y)| (y as usize) < BIP39_WORDS.len());
        if hash != words_hash || !valid {
            items.iter_mut().for_each(|item| *item = (0, 0));
            out.event(CardEvent::RecheckUnavailable);
            return;
        }
        let words = self
            .card
            .tmp_phone_words
            .entry(out.session.key())
            .or_insert_with(|| PhoneWords::new(pin.h_c, false));
        words.self_check = Some(SelfCheck {
            items,
            k: 0,
            recheck: true,
        });
        Self::card_self_check_question(out, words, false);
    }

    /// Words 33–64 from the device (`PhoneHalf`, binding (1) of spec PK.6). With no pin yet they
    /// pin the seal: h_C over the phone's own context and H(P) must be the device's, or it's
    /// another card, before any word is shown. After the pin they must hash to the pinned H(P):
    /// then they're kept in RAM and a `Status` parked for them, or an opening that needed them,
    /// goes on; otherwise it's a device fault (an abort after N).
    fn card_phone_half(
        &mut self,
        out: &mut CardOut,
        phone_half: &[u16; HALF],
    ) -> Result<(), &'static str> {
        if out.session.unverified {
            return self.card_unverified_phone_half(out, phone_half);
        }
        let rec = self.card_rec(out.session);
        let mut ops = Ops::default();
        if rec.pin.is_none() {
            // Checked before the seal is taken, so an `Err` changes nothing.
            let (sealed, proof_version) = out.session.sealed.ok_or("no status")?;
            let state = out.session.device_state.ok_or("no status")?;
            let seal = out.session.unpinned_seal.take().ok_or("no seal to pin")?;
            let commitments = seal.commitments.clone();
            match card_setup().pin(&out.session.context, seal, phone_half, &mut ops) {
                Ok(pinned) => {
                    let pin = CardPin {
                        format_id: sealed.format_id,
                        proof_version,
                        h_c: sealed.h_c,
                        coin_commit: sealed.coin_commit,
                        commitments,
                    };
                    self.card_mutate(out.session, |asref, share_index| CardMutation::Pinned {
                        access_structure_ref: asref,
                        share_index,
                        pin: Box::new(pin),
                    });
                    out.session.pinned = Some(pinned);
                    self.card_keep_phone_half(out.session, sealed.h_c, phone_half);
                    self.card_continue(out, state, false);
                }
                // A2: no WriteStart, so no word is shown.
                Err(_) => self.card_fail(out, CardFailReason::DifferentCard),
            }
            return Ok(());
        }
        let pinned = out.session.pinned.as_ref().ok_or("the seal isn't pinned")?;
        let h_c = pinned.seal().h_c;
        if pinned.check_phone_half(phone_half, &mut ops).is_err() {
            out.session.awaiting_phone_half = None;
            self.card_device_fault(out, CardFailReason::PhoneHalfChanged);
            return Ok(());
        }
        self.card_keep_phone_half(out.session, h_c, phone_half);
        if let Some(status) = out.session.awaiting_phone_half.take() {
            return self.card_status(out, status);
        }
        if out
            .session
            .opening
            .as_ref()
            .is_some_and(OpeningBuilder::is_complete)
        {
            return self.card_check_round_opening(out);
        }
        Ok(())
    }

    /// Words 33–64, checked against the pinned seal, into RAM.
    fn card_keep_phone_half(
        &mut self,
        session: &CoordCardSession,
        h_c: [u8; 32],
        phone_half: &[u16; HALF],
    ) {
        let ahead = self.card_rec(session).n.is_none();
        let words = self
            .card
            .tmp_phone_words
            .entry(session.key())
            .or_insert_with(|| PhoneWords::new(h_c, ahead));
        words.half = Some(*phone_half);
    }

    /// An `OpeningPart` of a box of D in the phone round: gathered until the opening is whole,
    /// then checked.
    fn card_round_opening_part(
        &mut self,
        out: &mut CardOut,
        part: CardOpeningPart,
    ) -> Result<(), &'static str> {
        let rec = self.card_rec(out.session);
        let (Some(n), Some((_, d)), None) = (rec.n, rec.drawn, rec.round) else {
            return Err("no phone round is running");
        };
        // Owner B8: the box asked for is the first whose word isn't verified yet.
        let k = self
            .card_next_unopened(out.session, &rec)
            .ok_or("every box is opened")?;
        let first = rec.first_unanswered().ok_or("every box is answered")?;
        let expected = d[k as usize];
        let (CardOpeningPart::Carries { j, .. } | CardOpeningPart::Proofs { j, .. }) = &part;
        // A stale request's opening of a box already answered changes nothing.
        if rec.d_position(*j).is_some_and(|position| position < first) {
            return Err("an opening of a box already answered");
        }
        // An opening that wasn't asked for, such as a second one for a box already verified,
        // is dropped.
        let builder = out
            .session
            .opening
            .as_mut()
            .ok_or("no opening was asked for")?;
        // Proofs of the box asked for with no word before them: what's left of an opening
        // rejected at its first part. They change nothing, so one bad frame costs one
        // re-request.
        if matches!(part, CardOpeningPart::Proofs { j, .. } if j == expected)
            && *builder == OpeningBuilder::new(expected)
        {
            return Err("an opening's proofs before its word");
        }
        let added = match part {
            // A carries part starts the opening again: a re-sent request makes a fresh run. A
            // malformed one is rejected below.
            CardOpeningPart::Carries { j, y, carries } => {
                let mut fresh = OpeningBuilder::new(expected);
                fresh.add_carries(j, y, &carries).map(|()| {
                    *builder = fresh;
                })
            }
            CardOpeningPart::Proofs { j, first, proofs } => builder.add_proofs(j, first, &proofs),
        };
        let complete = builder.is_complete();
        match added {
            Ok(()) => {}
            // L1.7.6: aborts at once.
            Err(proof::VerifyError::WrongBox) => {
                self.card_abort(out, CardFailReason::WrongBox);
                return Ok(());
            }
            Err(proof::VerifyError::WordRange) => {
                self.card_abort(out, CardFailReason::WordRange);
                return Ok(());
            }
            // A frame of the wrong shape, order or size for the box asked for is an invalid
            // opening: it counts toward the one re-request, as a failed check does (spec
            // L1.7.6), so a device can't stall the round by sending them.
            Err(_) => {
                self.card_opening_rejected(out, n, k, expected);
                return Ok(());
            }
        }
        if !complete {
            return Ok(());
        }
        self.card_check_round_opening(out)
    }

    /// The whole opening of the first unanswered box of D, checked against the proven seal and
    /// the words 33–64 in RAM (`check_opening`: H(P) inside h_C and open_dom, the parity
    /// equations and the 77 carry proofs). If the words aren't in RAM they're pulled again first,
    /// and the opening waits for them.
    fn card_check_round_opening(&mut self, out: &mut CardOut) -> Result<(), &'static str> {
        let rec = self.card_rec(out.session);
        let (Some(n), Some((_, d)), None) = (rec.n, rec.drawn, rec.round) else {
            return Err("no phone round is running");
        };
        let k = self
            .card_next_unopened(out.session, &rec)
            .ok_or("every box is opened")?;
        let expected = d[k as usize];
        // Checked before the opening is taken, so an `Err` changes nothing.
        if out
            .session
            .pinned
            .as_ref()
            .and_then(PinnedSeal::proven)
            .is_none()
        {
            return Err("the seal isn't proven");
        }
        let Some(mut phone_half) = self
            .card
            .tmp_phone_words
            .get(&out.session.key())
            .and_then(PhoneWords::complete)
        else {
            let h_c = rec.pin.as_ref().ok_or("the seal isn't pinned")?.h_c;
            out.request(CoordinatorCard::PullPhoneHalf { h_c });
            return Ok(());
        };
        let opening = out
            .session
            .opening
            .take()
            .and_then(OpeningBuilder::finish)
            .ok_or("opening incomplete")?;
        let proven = out
            .session
            .pinned
            .as_ref()
            .and_then(PinnedSeal::proven)
            .expect("checked above");
        let checked = card_setup().check_opening(
            &proven,
            &phone_half,
            &n,
            &d,
            expected,
            &opening,
            &mut Ops::default(),
        );
        crate::device::card::wipe(&mut phone_half, [0; HALF]);
        match checked {
            // Asked once the gate screen is up. Owner B8: a box after the first unanswered is
            // kept in RAM for its turn, and the next box is asked for.
            Ok(y) => {
                if let Some(words) = self.card.tmp_phone_words.get_mut(&out.session.key()) {
                    words.d_words[k as usize] = Some(y);
                }
                if rec.first_unanswered() == Some(k) {
                    out.session.current = Some((k, expected, y));
                    if out.session.gate_shown {
                        out.event(question(k, expected, y, rec.misses & (1 << k) != 0));
                    }
                }
                self.card_request_next_opening(out);
            }
            Err(_) => self.card_opening_rejected(out, n, k, expected),
        }
        Ok(())
    }

    /// A device that misbehaves before N (bad proofs twice, words 33–64 that aren't the pinned
    /// ones, or a seal that isn't the pinned one): Failed, and the device is marked suspect
    /// (design R12, owner B10). After N it's an abort.
    fn card_device_fault(&mut self, out: &mut CardOut, reason: CardFailReason) {
        self.card_suspect(out.device_id);
        self.card_fail(out, reason);
    }

    /// The device round passed: its result and N are persisted together, and only then does N
    /// go out (design S8). The phone round follows, from l1.
    fn card_device_round_passed(&mut self, out: &mut CardOut) {
        let rec = self.card_rec(out.session);
        let Some(SecretN(n)) = rec.pending_n else {
            return;
        };
        // Owner B8: N went out before the round and the openings were fetched meanwhile, so
        // the phone round starts at once; the device's gate screen asks the first question.
        if let (Some(_), Some((_, d))) = (rec.n, rec.drawn) {
            self.card_mutate(out.session, |asref, share_index| {
                CardMutation::DeviceRoundResult {
                    access_structure_ref: asref,
                    share_index,
                    pass: true,
                }
            });
            out.session.phase = SessionPhase::PhoneRound;
            out.session.device_state = Some(CardStateReport::Checking);
            let rec = self.card_rec(out.session);
            self.card_next_box(out, &rec, n, d);
            return;
        }
        self.card_mutate(out.session, |asref, share_index| {
            CardMutation::DeviceRoundResult {
                access_structure_ref: asref,
                share_index,
                pass: true,
            }
        });
        self.card_mutate(out.session, |asref, share_index| {
            CardMutation::CheckStarted {
                access_structure_ref: asref,
                share_index,
                n: SecretN(n),
            }
        });
        out.session.phase = SessionPhase::PhoneRound;
        out.request(CoordinatorCard::OpenStart { n });
        out.event(CardEvent::PreparingBox { k: 0 });
    }

    /// The device reached CARD_CHECKED (its `CheckDone`, or a `Status`): its half is checked,
    /// and the phone asks the user to confirm it, or moves on to what follows the confirm on this
    /// phone. `live`: the phone's page stayed up through an unplug, and keeps its place.
    fn card_finished(&mut self, out: &mut CardOut, live: bool) {
        self.card_mutate(out.session, |asref, share_index| CardMutation::Finished {
            access_structure_ref: asref,
            share_index,
        });
        let rec = self.card_rec(out.session);
        if let Some(reason) = rec.failed {
            out.session.phase = SessionPhase::Closed;
            return self.card_failed_event(out, reason);
        }
        out.event(CardEvent::CardFinished);
        self.card_phone_stage(out, live);
    }

    /// Where this phone is once the device's half is checked (owner B1): the confirm "I've
    /// written words 1–32" until the user held it; after it, the device is told (unless it's
    /// proving already) and the missing proofs are pulled, and the phone shows the final check if
    /// its own half is recorded, its quiz if the words are written, or the pages of words 33–64.
    /// A `live` page stays where it is, quiz item or page.
    fn card_phone_stage(&mut self, out: &mut CardOut, live: bool) {
        let rec = self.card_rec(out.session);
        let Some(h_c) = rec.pin.as_ref().map(|pin| pin.h_c) else {
            return;
        };
        out.session.phase = SessionPhase::Writing;
        if !rec.device_half_written {
            out.event(CardEvent::ConfirmDeviceHalf);
            return;
        }
        // The confirm reached no device yet, or a device that lost it: sent again. A device in
        // PROVING has it.
        if out.session.device_state != Some(CardStateReport::Proving) {
            out.notify(CoordinatorCard::DeviceHalfWritten { h_c });
        }
        if rec.checked {
            out.session.phase = SessionPhase::Finished;
        }
        // Owner B8: N goes right after the confirm, before any pull, until the device's coin is
        // recorded. Then `Proving`, or `Proven` (and `ReadyForFinalCheck` once both halves are
        // recorded).
        match rec.early_n() {
            Some(n) if rec.drawn.is_none() => {
                out.request(CoordinatorCard::OpenStart { n });
                let event = self.card_proving_event(out.session);
                out.event(event);
            }
            _ => self.card_pull_proofs(out),
        }
        if rec.checked {
            return;
        }
        self.card_phone_words_stage(out, live);
    }

    /// The pages of words 33–64, or the phone's quiz on them if the user said they're written.
    /// A `live` page stays where it is, quiz item or page.
    fn card_phone_words_stage(&mut self, out: &mut CardOut, live: bool) {
        let Some(words) = self.card.tmp_phone_words.get_mut(&out.session.key()) else {
            return;
        };
        let in_place = !words.written || words.self_check.is_some();
        if live && in_place {
            return;
        }
        if words.written && words.complete().is_some() {
            // The quiz isn't persisted, like the standard backup check's: it starts again from
            // item 1.
            if let Some(check) = words.self_check.as_mut() {
                check.k = 0;
            } else if let Some(half) = words.complete() {
                words.self_check = Some(SelfCheck {
                    items: PHONE_HALF
                        .zip(half)
                        .map(|(j, y)| (j as u8 + 1, y))
                        .collect(),
                    k: 0,
                    recheck: false,
                });
            }
            Self::card_self_check_question(out, words, false);
        } else {
            words.written = false;
            words.self_check = None;
            Self::card_page_event(out, words);
        }
    }

    /// Words 33–64 are next on the phone: after the user's confirm of words 1–32, and before
    /// the final check (owner B1), or on an unverified card after its confirm and before it's
    /// written (owner B15).
    fn card_phone_half_next(&self, session: &CoordCardSession) -> bool {
        let rec = self.card_rec(session);
        if session.unverified {
            session.unverified_confirmed
                && rec.unverified_h_c.is_some()
                && !rec.unverified_written
                && rec.status() != CardBackupStatus::Failed
        } else {
            rec.phone_half_next()
        }
    }

    /// Owner B15: the `Status` dispatch on an install that didn't run the keygen, for a device
    /// below CHECKING that holds no check of this phone's (the terminal and CHECKING states, and
    /// a device holding an N, were dispatched before, as for any phone with no pin). It never
    /// pins, draws N, pulls proofs or openings. A SEALED card waits for the user to choose to
    /// write it unverified; one this phone is writing resumes; anything else is another phone's
    /// backup, as for a phone with no pin (owner B9).
    fn card_unverified_status(&mut self, out: &mut CardOut, status: &CardStatus, live: bool) {
        let rec = self.card_rec(out.session);
        if rec.unverified_written {
            out.session.phase = SessionPhase::Closed;
            out.event(CardEvent::UnverifiedWritten);
            return;
        }
        let h_c = status.sealed.h_c;
        match (status.state, rec.unverified_h_c) {
            (CardStateReport::Sealed, None) => {
                out.session.phase = SessionPhase::Pinning;
                out.event(CardEvent::UnverifiedIntro);
            }
            (
                CardStateReport::Sealed
                | CardStateReport::Writing
                | CardStateReport::SelfCheck
                | CardStateReport::CardChecked,
                Some(ours),
            ) if ours == h_c => self.card_unverified_continue(out, status.state, live),
            // An honest device's seal is deterministic, so the card this phone started can't
            // come back under another one. Nothing is recorded and no word is shown.
            (CardStateReport::Sealed, Some(_)) => {
                out.session.phase = SessionPhase::Closed;
                out.event(CardEvent::DifferentCard);
            }
            _ => {
                out.session.phase = SessionPhase::Closed;
                out.event(CardEvent::StartedOnAnotherPhone);
            }
        }
    }

    /// Owner B15: the user chose, on the unverified card's intro, to write it unverified. Words
    /// 33–64 are pulled for the device's own seal, unpinned; `WriteStart` follows them.
    pub fn card_write_unverified(
        &mut self,
        device_id: DeviceId,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action(device_id, |coord, out| {
            let rec = coord.card_rec(out.session);
            if !out.session.unverified {
                return Err(CardActionError::WrongState(
                    "this phone made this wallet: its check is verified",
                ));
            }
            if rec.unverified_h_c.is_some() || rec.status() != CardBackupStatus::NoCard {
                return Err(CardActionError::Stale);
            }
            if out.session.device_state != Some(CardStateReport::Sealed)
                || out.session.sealed.is_none()
            {
                return Err(CardActionError::WrongState("the device's card isn't ready"));
            }
            coord.card_unverified_continue(out, CardStateReport::Sealed, false);
            Ok(())
        })
    }

    /// Owner B15: the unverified card, from words 33–64 in RAM (pulled for the device's own seal
    /// if they aren't): `UnverifiedWriteStarted` is persisted, then `WriteStart` goes, and the
    /// phone shows the device's half, or what follows once the device's quiz passed.
    fn card_unverified_continue(&mut self, out: &mut CardOut, state: CardStateReport, live: bool) {
        let Some((sealed, _)) = out.session.sealed else {
            return;
        };
        let h_c = sealed.h_c;
        let in_ram = self
            .card
            .tmp_phone_words
            .get(&out.session.key())
            .is_some_and(|words| words.half.is_some() && words.h_c == h_c);
        out.session.device_state = Some(state);
        if !in_ram {
            out.session.phase = SessionPhase::Pinning;
            out.request(CoordinatorCard::PullPhoneHalf { h_c });
            return;
        }
        if self.card_rec(out.session).unverified_h_c.is_none() {
            self.card_mutate(out.session, |asref, share_index| {
                CardMutation::UnverifiedWriteStarted {
                    access_structure_ref: asref,
                    share_index,
                    h_c,
                }
            });
        }
        out.notify(CoordinatorCard::WriteStart { h_c });
        out.session.phase = SessionPhase::Writing;
        if state == CardStateReport::CardChecked {
            self.card_unverified_stage(out, live);
        } else {
            // The device's quiz isn't done (again): words 33–64 wait for it and the confirm.
            out.session.unverified_confirmed = false;
            out.event(CardEvent::DeviceHalf);
        }
    }

    /// Owner B15: words 33–64 for the unverified card, from the device, for its own seal. They
    /// can't be checked against anything: nothing here is pinned.
    fn card_unverified_phone_half(
        &mut self,
        out: &mut CardOut,
        phone_half: &[u16; HALF],
    ) -> Result<(), &'static str> {
        let (sealed, _) = out.session.sealed.ok_or("no status")?;
        let state = out.session.device_state.ok_or("no status")?;
        let words = self
            .card
            .tmp_phone_words
            .entry(out.session.key())
            .or_insert_with(|| PhoneWords::new(sealed.h_c, false));
        if words.h_c != sealed.h_c {
            *words = PhoneWords::new(sealed.h_c, false);
        }
        words.half = Some(*phone_half);
        self.card_unverified_continue(out, state, false);
        Ok(())
    }

    /// Owner B15: where the unverified card is once the device's quiz on words 1–32 passed: the
    /// confirm until the user held it, then the pages of words 33–64 and the phone's quiz.
    /// Nothing goes to the device.
    fn card_unverified_stage(&mut self, out: &mut CardOut, live: bool) {
        out.session.phase = SessionPhase::Writing;
        if !out.session.unverified_confirmed {
            out.event(CardEvent::ConfirmDeviceHalf);
            return;
        }
        self.card_phone_words_stage(out, live);
    }

    /// Owner B15: the hold "I've written words 1–32" on the unverified card. It's taken only once
    /// the device's quiz on them passed in this session, and nothing goes to the device: no
    /// proof, no N.
    fn card_unverified_confirm(&mut self, out: &mut CardOut) -> Result<(), CardActionError> {
        let rec = self.card_rec(out.session);
        if rec.unverified_h_c.is_none() || rec.unverified_written {
            return Err(CardActionError::WrongState(
                "no unverified card is being written",
            ));
        }
        if out.session.device_state != Some(CardStateReport::CardChecked) {
            return Err(CardActionError::WrongState(
                "the device's quiz on words 1–32 isn't done",
            ));
        }
        out.session.unverified_confirmed = true;
        self.card_unverified_stage(out, false);
        Ok(())
    }

    /// A check that ended before N, with the device's FAILED verdict: both recorded, no
    /// suspect mark.
    fn card_ended_before_n(&mut self, out: &mut CardOut, reason: CardFailReason) {
        out.session.phase = SessionPhase::Closed;
        self.card_mutate(out.session, |asref, share_index| CardMutation::Failed {
            access_structure_ref: asref,
            share_index,
            reason,
        });
        self.card_record_device_fail(out);
        let reason = self.card_rec(out.session).failed.unwrap_or(reason);
        self.card_failed_event(out, reason);
    }

    fn card_record_device_fail(&mut self, out: &mut CardOut) {
        self.card_mutate(out.session, |asref, share_index| {
            CardMutation::DeviceVerdict {
                access_structure_ref: asref,
                share_index,
                passed: false,
            }
        });
        self.card.tmp_phone_words.remove(&out.session.key());
    }

    /// `Failed{reason}` for the user, with the device's suspect mark and whether N had gone out.
    fn card_failed_event(&self, out: &mut CardOut, reason: CardFailReason) {
        let after_n = self.card_rec(out.session).after_n();
        out.event(CardEvent::Failed {
            reason,
            device_suspect: self.device_is_suspect(out.device_id),
            after_n,
        });
    }

    /// The `Status` dispatch. Terminal and Checking states come before any pin decision; those
    /// states only check that the card is the pinned one.
    fn card_status(&mut self, out: &mut CardOut, status: CardStatus) -> Result<(), &'static str> {
        let rec = self.card_rec(out.session);
        // Only a Status names its share, so one for another share is stale: the device's
        // earlier session, for another key. The pin checks h_C over this phone's own ctx, so a
        // device that lies about its image is caught there.
        if status.share_image != out.session.context.image {
            return Err("a status for another share");
        }
        // A device session starts: a verified question is kept, and is asked once this
        // session's gate screen is up (spec §0.6.1.2 item 2), so it costs no run. An opening
        // half received is void: a `Status` always answers a fresh `Begin`.
        out.session.gate_shown = false;
        out.session.opening = None;
        out.session.unpinned_seal = None;
        out.session.awaiting_phone_half = None;
        out.session.proofs_this_session = Bits::default();
        // The phone's pages ran on through an unplug in this flow: their place stands.
        let live = out.session.phase == SessionPhase::Writing;
        out.session.phase = SessionPhase::Closed;
        if status.format_id != FORMAT_ID
            || status.proof_version != PROOF_VERSION
            || status.sealed.format_id != FORMAT_ID
        {
            // Review F3: once N is out, the device took it under this format, so an honest one
            // can't report another: an abort, as for any other state but this check's (owner B9).
            if rec.n_out() {
                self.card_abort(out, CardFailReason::Refused(CardRefusal::Unsupported));
                return Ok(());
            }
            out.event(CardEvent::Refused {
                reason: CardRefusal::Unsupported,
            });
            return Ok(());
        }
        out.session.device_state = Some(status.state);
        out.session.sealed = Some((status.sealed, status.proof_version));

        // A seal that isn't the pinned one is a new card (owner B3): the paper card was written
        // under the pin and is never continued under another seal. An honest device's seal is
        // deterministic, so this is a device fault: the device is marked suspect (owner B10).
        // Before the device's verdict is recorded it fails the card (an abort after N); nothing
        // more is sent, above all not N or `WriteStart`.
        let other_seal = rec.pin.as_ref().is_some_and(|pin| {
            (pin.format_id, pin.h_c, pin.coin_commit)
                != (
                    status.sealed.format_id,
                    status.sealed.h_c,
                    status.sealed.coin_commit,
                )
        });
        if other_seal && rec.device_passed.is_none() {
            if rec.status() != CardBackupStatus::Failed {
                self.card_device_fault(out, CardFailReason::SealChanged);
            } else {
                // The card already failed, some other way or on an earlier changed seal: it
                // stays failed as recorded, and the device is suspect all the same.
                self.card_suspect(out.device_id);
                self.card_failed_event(out, CardFailReason::SealChanged);
            }
            return Ok(());
        }

        // Owner B15: an unverified card this install started (its `WriteStart` lost, so the
        // device stayed SEALED) that the device now reports PASSED under the same seal was
        // written, and checked, on the install that ran the keygen. Here it's written, not
        // verified, and never Checked; without this the record stayed "not written yet" for
        // good (review B15 protocol F4). The later check below then runs as for any unverified
        // card.
        if status.state == CardStateReport::Passed
            && out.session.unverified
            && rec.pin.is_none()
            && rec.unverified_h_c == Some(status.sealed.h_c)
            && !rec.unverified_written
            && rec.status() == CardBackupStatus::NotChecked
        {
            self.card_mutate(out.session, |asref, share_index| {
                CardMutation::UnverifiedWritten {
                    access_structure_ref: asref,
                    share_index,
                }
            });
            // The words leave RAM, as when the user finishes an unverified card here: nothing
            // checks them again.
            self.card.tmp_phone_words.remove(&out.session.key());
        }
        let rec = self.card_rec(out.session);

        // A PASSED card: a later check (design §3.9) only if this phone holds it as a two-key
        // pass, or has no record of it. A PASSED for a card this phone pinned or wrote but
        // didn't pass is never a pass here (audit F1): it falls to the rows below, which show
        // another phone's check before N and abort after it.
        if status.state == CardStateReport::Passed && rec.may_recheck() {
            self.card_start_recheck(out);
            return Ok(());
        }

        // The device's verdict is recorded, so the check is over: whatever the device reports
        // now (another state or seal) changes nothing, and the phone shows its own record.
        if let Some(passed) = rec.device_passed {
            self.card_device_verdict(out, passed);
            return Ok(());
        }

        match status.state {
            CardStateReport::Passed | CardStateReport::Failed => {
                let failed = status.state == CardStateReport::Failed;
                if failed
                    && status.reason == Some(VerdictReason::DeviceRoundFailed)
                    && rec.device_round == Some(DeviceRoundRecord::Pending)
                {
                    // The device round's `DeviceRound{false}` was lost.
                    self.card_mutate(out.session, |asref, share_index| {
                        CardMutation::DeviceRoundResult {
                            access_structure_ref: asref,
                            share_index,
                            pass: false,
                        }
                    });
                }
                if rec.round.is_some() {
                    self.card_device_verdict(out, !failed);
                } else if rec.n.is_some()
                    && failed
                    && status.reason == Some(VerdictReason::DeviceRoundFailed)
                    && rec.device_round.is_some()
                {
                    // Owner B8: the device round failed after the chip knew D.
                    self.card_abort(out, CardFailReason::DeviceRoundFailed);
                } else if rec.n.is_some() {
                    // The device ended a check this phone started and never finished.
                    self.card_abort(out, CardFailReason::Refused(CardRefusal::Closed));
                } else if let Some(reason) = rec.failed {
                    if failed {
                        self.card_record_device_fail(out);
                    }
                    self.card_failed_event(out, reason);
                } else {
                    // A check that ended before N: the device says why, so a reconnecting
                    // phone shows the same alarm.
                    let reason = match (failed, status.reason) {
                        (true, Some(VerdictReason::DeviceRoundFailed)) => {
                            Some(CardFailReason::DeviceRoundFailed)
                        }
                        (true, Some(VerdictReason::StoppedBeforeN)) => {
                            Some(CardFailReason::Stopped)
                        }
                        _ => None,
                    };
                    match reason {
                        Some(reason) => self.card_ended_before_n(out, reason),
                        None => out.event(CardEvent::StartedOnAnotherPhone),
                    }
                }
                return Ok(());
            }
            CardStateReport::Checking => {
                // This phone's check needs P for the D check: it's pulled first (binding (1)).
                // Never for another phone's check, which records nothing here.
                if rec.device_round.is_some()
                    && rec.check_not_ended()
                    && self.card_park_for_phone_half(out, &status)
                {
                    return Ok(());
                }
                self.card_status_checking(out, &rec, status.check);
                return Ok(());
            }
            _ => {}
        }

        if let Some(reason) = rec.failed {
            // A Stop whose verdict was lost is sent again.
            if reason == CardFailReason::Stopped
                && matches!(
                    status.state,
                    CardStateReport::CardChecked | CardStateReport::Proving
                )
            {
                out.request(CoordinatorCard::Stop);
            }
            self.card_failed_event(out, reason);
            return Ok(());
        }
        if let Some(n) = rec.n {
            // Owner B8: N goes out after the confirm of words 1–32, so a device in PROVING holds
            // this N, or (CARD_CHECKED or PROVING) none while its coin isn't recorded here. Any
            // other device below CHECKING went back past what it reported: an abort.
            // A device round recorded pending here whose `FinalBegin` the device never got is
            // started again by the user's [Continue].
            let ours = proof::n_tag(&n, &mut Ops::default());
            let round_not_begun =
                matches!(rec.device_round, None | Some(DeviceRoundRecord::Pending));
            let early = match (status.state, status.check) {
                (CardStateReport::Proving, CardCheckReport::Started { n_tag, .. }) => {
                    n_tag == ours && round_not_begun
                }
                (
                    CardStateReport::CardChecked | CardStateReport::Proving,
                    CardCheckReport::None,
                ) => rec.drawn.is_none() && rec.device_round.is_none(),
                _ => false,
            };
            if !early {
                self.card_abort(out, CardFailReason::Refused(CardRefusal::WrongState));
                return Ok(());
            }
            // An abort while words 33–64 were written, or before the device got `FinalBegin`,
            // which the device didn't hear (review P4).
            if rec.round.is_some() {
                out.request(CoordinatorCard::PhoneRound { n, pass: false });
                let reason = rec.fail_reason().unwrap_or(CardFailReason::Stopped);
                self.card_failed_event(out, reason);
                return Ok(());
            }
        }

        // Owner B8: N goes out with the confirm of words 1–32, so a device holding an N this
        // phone never drew is another phone's check: record nothing, never pull.
        if rec.n.is_none() && matches!(status.check, CardCheckReport::Started { .. }) {
            out.event(CardEvent::StartedOnAnotherPhone);
            return Ok(());
        }

        if rec.pin.is_none() && out.session.unverified {
            // Owner B15: only the install that ran the keygen pins a seal, draws N and takes
            // proofs and openings. Every step of the verified check hangs off the pin, so this is
            // the one place it's withheld.
            self.card_unverified_status(out, &status, live);
            return Ok(());
        }
        if rec.pin.is_none() {
            // Owner B9, one phone per check: the phone that starts a backup pins its seal before
            // any word is shown, while the device is still SEALED. A phone with no pin that meets
            // a device past SEALED didn't start this backup, so it records nothing, never pulls,
            // and never draws N: the check can't move to it, and it can never reach Checked
            // (audit-lying-chip F2).
            if status.state != CardStateReport::Sealed {
                out.event(CardEvent::StartedOnAnotherPhone);
                return Ok(());
            }
            out.session.phase = SessionPhase::Pinning;
            out.session.seal_builder = Some(SealBuilder::default());
            out.request(CoordinatorCard::PullCommit {
                from: 0,
                count: POINTS_PER_FRAME as u8,
            });
            out.event(CardEvent::PreparingCard);
            return Ok(());
        }
        // R24: on resume the status is compared with the pin (above) instead of pulling the C's
        // again; the pinned C's still bind every proof and opening. P, which the phone keeps in
        // RAM only, is pulled again first if it's gone.
        if self.card_park_for_phone_half(out, &status) {
            return Ok(());
        }
        self.card_continue(out, status.state, live);
        Ok(())
    }

    /// With the seal pinned and words 33–64 not in RAM (an app restart lost them), the status
    /// waits while they're pulled again; they must hash to the pinned H(P) (binding (1)). Says
    /// whether it parked.
    fn card_park_for_phone_half(&mut self, out: &mut CardOut, status: &CardStatus) -> bool {
        let rec = self.card_rec(out.session);
        let in_ram = self
            .card
            .tmp_phone_words
            .get(&out.session.key())
            .is_some_and(|words| words.half.is_some());
        let Some(h_c) = rec.pin.as_ref().map(|pin| pin.h_c).filter(|_| !in_ram) else {
            return false;
        };
        out.session.awaiting_phone_half = Some(status.clone());
        out.request(CoordinatorCard::PullPhoneHalf { h_c });
        true
    }

    /// The `Status` dispatch for a device in CHECKING, keyed on its check report.
    fn card_status_checking(
        &mut self,
        out: &mut CardOut,
        rec: &CoordCardRecord,
        check: CardCheckReport,
    ) {
        let device_round = matches!(
            check,
            CardCheckReport::DeviceRoundPending | CardCheckReport::DeviceRoundPassed
        );
        match (check, rec.device_round, rec.n) {
            // This phone stopped the device round, and the device's verdict was lost: Stop
            // again. N never went out.
            (_, Some(DeviceRoundRecord::Pending), None) if device_round && rec.failed.is_some() => {
                let reason = rec.failed.expect("just checked");
                if reason == CardFailReason::Stopped {
                    out.request(CoordinatorCard::Stop);
                }
                self.card_failed_event(out, reason);
            }
            // The device round goes on: the device answers a repeat in its session
            // idempotently, and resumes it at the persisted word in a new one.
            (CardCheckReport::DeviceRoundPending, Some(DeviceRoundRecord::Pending), None) => {
                self.card_send_final_begin(out);
            }
            // The round passed, but its `DeviceRound{true}` was lost.
            (CardCheckReport::DeviceRoundPassed, Some(DeviceRoundRecord::Pending), None) => {
                self.card_device_round_passed(out);
            }
            // Review P4: the check ended on this phone (an abort, such as "No, I didn't" with the
            // device unplugged) and the device never heard it: the round's result goes again, and
            // the device round isn't resumed on a failed record.
            (CardCheckReport::StartedDeviceRoundPending { n_tag, .. }, _, Some(n))
                if !rec.check_not_ended() && proof::n_tag(&n, &mut Ops::default()) == n_tag =>
            {
                out.request(CoordinatorCard::PhoneRound { n, pass: false });
                let reason = rec.fail_reason().unwrap_or(CardFailReason::Interrupted);
                self.card_failed_event(out, reason);
            }
            // Owner B8: the device round on this phone's N goes on (`FinalBegin` resumes it), or
            // passed and its `DeviceRound{true}` was lost.
            (
                CardCheckReport::StartedDeviceRoundPending { n_tag, .. },
                Some(DeviceRoundRecord::Pending),
                Some(n),
            ) if proof::n_tag(&n, &mut Ops::default()) == n_tag => {
                self.card_send_final_begin(out);
            }
            (CardCheckReport::Started { n_tag, .. }, Some(DeviceRoundRecord::Pending), Some(n))
                if proof::n_tag(&n, &mut Ops::default()) == n_tag =>
            {
                self.card_device_round_passed(out);
                // A new device session: `OpenStart` again brings its gate screen up.
                out.request(CoordinatorCard::OpenStart { n });
            }
            (CardCheckReport::StartedDeviceRoundPending { .. }, _, Some(_)) => {
                self.card_abort(out, CardFailReason::Refused(CardRefusal::WrongState));
            }
            // N was persisted but `OpenStart` never arrived: the same N again, or the round's
            // result if the phone already has one.
            (CardCheckReport::DeviceRoundPassed, Some(DeviceRoundRecord::Passed), Some(n)) => {
                out.session.phase = SessionPhase::PhoneRound;
                if rec.round.is_some() {
                    out.request(CoordinatorCard::PhoneRound {
                        n,
                        pass: rec.passed_round(),
                    });
                    out.event(CardEvent::RoundSent {
                        answered: rec.answered.count_ones() as u8,
                    });
                } else {
                    out.request(CoordinatorCard::OpenStart { n });
                    out.event(CardEvent::PreparingBox {
                        k: rec.first_unanswered().unwrap_or(0),
                    });
                }
            }
            // Review F3: this phone holds N but never started the device round, so it never sent
            // `FinalBegin`: no honest device is in CHECKING. An abort, as for any other state but
            // this check's.
            (CardCheckReport::Started { .. }, None, Some(_)) => {
                self.card_abort(out, CardFailReason::Refused(CardRefusal::WrongState));
            }
            (CardCheckReport::Started { n_tag, .. }, _, _) => {
                let ours = rec
                    .n
                    .filter(|n| proof::n_tag(n, &mut Ops::default()) == n_tag);
                match (ours, rec.round) {
                    // This phone holds N for this card, so its device round passed with this
                    // phone, and an honest device takes no other N (`AlreadyUsed`): an abort,
                    // as for the device-round rows below.
                    (None, _) if rec.n.is_some() => {
                        self.card_abort(out, CardFailReason::Refused(CardRefusal::WrongState));
                    }
                    // Record nothing, never pull.
                    (None, _) => out.event(CardEvent::StartedOnAnotherPhone),
                    // Collect a lost verdict; the device answers a repeat idempotently.
                    (Some(n), Some(_)) => {
                        out.session.phase = SessionPhase::PhoneRound;
                        out.request(CoordinatorCard::PhoneRound {
                            n,
                            pass: rec.passed_round(),
                        });
                        out.event(CardEvent::RoundSent {
                            answered: rec.answered.count_ones() as u8,
                        });
                    }
                    (Some(n), None) => {
                        out.session.phase = SessionPhase::PhoneRound;
                        out.request(CoordinatorCard::OpenStart { n });
                        out.event(CardEvent::PreparingBox {
                            k: rec.first_unanswered().unwrap_or(0),
                        });
                    }
                }
            }
            // The phone holds N, so the device round passed, but the device is back before it
            // or at a round this phone can't resume: it went back past a pass it reported. An
            // abort (spec L1.7.6), as for a device below CHECKING.
            (
                CardCheckReport::DeviceRoundPending | CardCheckReport::DeviceRoundPassed,
                _,
                Some(_),
            ) => {
                self.card_abort(out, CardFailReason::Refused(CardRefusal::WrongState));
            }
            // Another phone's final check: record nothing, never pull.
            _ => out.event(CardEvent::StartedOnAnotherPhone),
        }
    }

    /// The flow once the seal is pinned and words 33–64 are in RAM, by the device's state below
    /// CHECKING (the resume table, design §3.6): `WriteStart`, which the device answers with its
    /// screen, then the device's half, or once it's checked the phone's stage. Nothing here
    /// pulls proofs before the user confirmed words 1–32 (spec PK.7).
    fn card_continue(&mut self, out: &mut CardOut, state: CardStateReport, live: bool) {
        let rec = self.card_rec(out.session);
        let (Some(pin), true) = (
            rec.pin.as_ref(),
            matches!(
                state,
                CardStateReport::Sealed
                    | CardStateReport::Writing
                    | CardStateReport::SelfCheck
                    | CardStateReport::CardChecked
                    | CardStateReport::Proving
            ),
        ) else {
            return;
        };
        let checked_device = matches!(
            state,
            CardStateReport::CardChecked | CardStateReport::Proving
        );
        // A finished card never goes back: refused here, and no word is shown.
        if rec.finished && !checked_device {
            out.session.phase = SessionPhase::Closed;
            out.event(CardEvent::AlreadyFinished);
            return;
        }
        let h_c = pin.h_c;
        self.card
            .tmp_phone_words
            .entry(out.session.key())
            .or_insert_with(|| PhoneWords::new(h_c, rec.n.is_none()));
        out.notify(CoordinatorCard::WriteStart { h_c });
        out.session.phase = SessionPhase::Writing;
        if checked_device {
            // A `Finished` the phone missed is persisted from the `Status` first.
            self.card_finished(out, live);
        } else {
            out.event(CardEvent::DeviceHalf);
        }
    }

    /// The proofs' progress, as the phone shows it while the device computes them.
    fn card_proving_event(&self, session: &CoordCardSession) -> CardEvent {
        CardEvent::Proving {
            verified: session
                .pinned
                .as_ref()
                .map_or(0, |pinned| pinned.verified().len() as u16),
            total: COMMIT_COUNT,
        }
    }

    /// The next missing bit proofs, or `Proven` and, once both halves are recorded, the final
    /// check. Pulled only after the user confirmed words 1–32 (owner B1).
    fn card_pull_proofs(&mut self, out: &mut CardOut) {
        let Some(pinned) = out.session.pinned.as_ref() else {
            return;
        };
        if !self.card_rec(out.session).device_half_written {
            return;
        }
        match pinned.next_missing(PROOFS_PER_FRAME as u8) {
            Some((from, count)) => {
                out.request(CoordinatorCard::PullProofs { from, count });
                let event = self.card_proving_event(out.session);
                out.event(event);
            }
            None => {
                out.event(CardEvent::Proven);
                let rec = self.card_rec(out.session);
                if rec.halves_recorded() {
                    if rec.device_round == Some(DeviceRoundRecord::Pending)
                        && rec.round.is_none()
                        && rec.check_not_ended()
                    {
                        // [Continue] was taken (the round is recorded pending) but the device
                        // is still proving: its `FinalBegin` was lost, to an unplug, an app
                        // restart or a dropped frame. It goes again, on the same N, with no
                        // second [Continue] (core-protocol review F1).
                        self.card_send_final_begin(out);
                    } else {
                        out.event(CardEvent::ReadyForFinalCheck);
                    }
                }
                // Owner B8: the openings of D, verified against the proven seal while words
                // 33–64 are written.
                self.card_request_next_opening(out);
            }
        }
    }

    /// Owner B8: the first box of D whose word isn't verified in RAM, from the first unanswered.
    /// Without the phone half in RAM it's the first unanswered box (its opening waits for P).
    fn card_next_unopened(&self, session: &CoordCardSession, rec: &CoordCardRecord) -> Option<u8> {
        let first = rec.first_unanswered()?;
        let Some(words) = self.card.tmp_phone_words.get(&session.key()) else {
            return Some(first);
        };
        (first..CHECK_BOXES as u8).find(|&k| words.d_words[k as usize].is_none())
    }

    /// Owner B8: asks for the next unverified opening of D, once the seal is proven and the
    /// device has taken N, unless one is asked for already. It shows nothing before the phone
    /// round. After an app restart only in the phone round (review P1, [`PhoneWords`]).
    fn card_request_next_opening(&mut self, out: &mut CardOut) {
        let rec = self.card_rec(out.session);
        let (Some(n), Some((_, d)), None) = (rec.n, rec.drawn, rec.round) else {
            return;
        };
        if out.session.opening.is_some() || rec.device_round == Some(DeviceRoundRecord::Failed) {
            return;
        }
        let proven = out
            .session
            .pinned
            .as_ref()
            .and_then(PinnedSeal::proven)
            .is_some();
        if !proven {
            return;
        }
        // Review P1: after an app restart (D's verified words gone from RAM) an opening is fetched
        // only in the phone round, so a restart costs a box at most one run, and only there.
        let ahead = self
            .card
            .tmp_phone_words
            .get(&out.session.key())
            .is_some_and(|words| words.ahead);
        if !ahead && out.session.phase != SessionPhase::PhoneRound {
            return;
        }
        let Some(k) = self.card_next_unopened(out.session, &rec) else {
            return;
        };
        if out
            .session
            .current
            .is_some_and(|(current_k, _, _)| current_k == k)
        {
            return;
        }
        let j = d[k as usize];
        out.session.opening = Some(OpeningBuilder::new(j));
        out.request(CoordinatorCard::OpenBox { n, j });
        Self::card_preparing(out, &rec, k);
    }

    /// "Preparing" for box k, only in the phone round (owner B8: openings are fetched earlier,
    /// behind the phone's pages), and only for the box the user waits for: an opening fetched
    /// ahead while a question is up never covers it.
    fn card_preparing(out: &mut CardOut, rec: &CoordCardRecord, k: u8) {
        if out.session.phase == SessionPhase::PhoneRound && rec.first_unanswered() == Some(k) {
            out.event(CardEvent::PreparingBox { k });
        }
    }

    /// An opening of D[k] = j that failed its check, or came in a bad frame: re-requested once
    /// (it uses a run), and a second failure aborts (spec L1.7.6).
    fn card_opening_rejected(&mut self, out: &mut CardOut, n: [u8; 32], k: u8, j: u8) {
        self.card_mutate(out.session, |asref, share_index| {
            CardMutation::OpeningRejected {
                access_structure_ref: asref,
                share_index,
                j,
            }
        });
        if self.card_rec(out.session).opening_rejects[j as usize] >= 2 {
            self.card_abort(out, CardFailReason::BadOpening);
        } else {
            out.session.opening = Some(OpeningBuilder::new(j));
            out.request(CoordinatorCard::OpenBox { n, j });
            let rec = self.card_rec(out.session);
            Self::card_preparing(out, &rec, k);
        }
    }

    /// The first unanswered box: its verified opening's question, or a request for it.
    fn card_next_box(
        &mut self,
        out: &mut CardOut,
        rec: &CoordCardRecord,
        n: [u8; 32],
        d: [u8; CHECK_BOXES],
    ) {
        let Some(k) = rec.first_unanswered() else {
            // Every box answered: the round result is next, from `card_answer`.
            return;
        };
        match out.session.current {
            // Never asked for again: `GateShown` asks its question, or asked it already.
            Some((current_k, _, _)) if current_k == k => {
                if !out.session.gate_shown {
                    out.event(CardEvent::PreparingBox { k })
                }
            }
            _ => {
                // Owner B8: verified while words 33–64 were written, so no wait.
                let ready = self
                    .card
                    .tmp_phone_words
                    .get(&out.session.key())
                    .and_then(|words| words.d_words[k as usize]);
                if let Some(y) = ready {
                    out.session.current = Some((k, d[k as usize], y));
                    if out.session.gate_shown {
                        out.event(question(k, d[k as usize], y, rec.misses & (1 << k) != 0));
                    } else {
                        out.event(CardEvent::PreparingBox { k });
                    }
                    self.card_request_next_opening(out);
                    return;
                }
                out.session.current = None;
                if out.session.opening.is_some() {
                    // Asked for already: its check asks the question.
                    out.event(CardEvent::PreparingBox { k });
                    return;
                }
                let j = d[k as usize];
                out.session.opening = Some(OpeningBuilder::new(j));
                out.request(CoordinatorCard::OpenBox { n, j });
                out.event(CardEvent::PreparingBox { k });
            }
        }
    }

    /// Records the device's verdict once the phone's round result is recorded.
    fn card_device_verdict(&mut self, out: &mut CardOut, passed: bool) {
        let rec = self.card_rec(out.session);
        let passed = match rec.device_passed {
            Some(recorded) => recorded,
            None => {
                self.card_mutate(out.session, |asref, share_index| {
                    CardMutation::DeviceVerdict {
                        access_structure_ref: asref,
                        share_index,
                        passed,
                    }
                });
                passed
            }
        };
        // The check is over: the phone half isn't needed in RAM again.
        self.card.tmp_phone_words.remove(&out.session.key());
        out.session.phase = SessionPhase::Closed;
        if passed != rec.passed_round() {
            self.card_suspect(out.device_id);
            self.card_failed_event(out, CardFailReason::DeviceVerdictMismatch);
        } else if passed {
            out.event(CardEvent::Passed);
        } else {
            let reason = rec
                .fail_reason()
                .unwrap_or(CardFailReason::DeviceVerdictMismatch);
            self.card_failed_event(out, reason);
        }
    }

    /// Abort after N (spec L1.7.5, L1.7.6): the round fails, the device is suspect, and
    /// `PhoneRound{n, false}` goes out once. Before N it's [`card_fail`](Self::card_fail).
    fn card_abort(&mut self, out: &mut CardOut, reason: CardFailReason) {
        let rec = self.card_rec(out.session);
        let Some(n) = rec.n else {
            return self.card_fail(out, reason);
        };
        out.session.phase = SessionPhase::Closed;
        out.session.current = None;
        out.session.opening = None;
        // Only the first abort records and sends it, so a refusal of it can't loop.
        let first = rec.round.is_none() || rec.passed_round();
        if first {
            self.card_mutate(out.session, |asref, share_index| match rec.round {
                None => CardMutation::RoundResult {
                    access_structure_ref: asref,
                    share_index,
                    pass: false,
                    reason: Some(reason),
                    words_hash: None,
                },
                // The round had passed, but any abort fails the card (L1.7.6).
                Some(_) => CardMutation::Failed {
                    access_structure_ref: asref,
                    share_index,
                    reason,
                },
            });
        }
        self.card_suspect(out.device_id);
        if first {
            out.request(CoordinatorCard::PhoneRound { n, pass: false });
        }
        // Owner B8: an abort before the device round (a device fault while words 33–64 are
        // written) is reported as before N, with the suspect mark.
        let after_n = self.card_rec(out.session).after_n();
        out.event(CardEvent::Failed {
            reason,
            device_suspect: true,
            after_n,
        });
    }

    /// A failure before N. After N it's an abort.
    fn card_fail(&mut self, out: &mut CardOut, reason: CardFailReason) {
        if self.card_rec(out.session).n.is_some() {
            return self.card_abort(out, reason);
        }
        self.card_mutate(out.session, |asref, share_index| CardMutation::Failed {
            access_structure_ref: asref,
            share_index,
            reason,
        });
        out.session.seal_builder = None;
        out.session.unpinned_seal = None;
        out.session.awaiting_phone_half = None;
        out.session.phase = SessionPhase::Closed;
        match reason {
            CardFailReason::DifferentCard => {
                self.card.tmp_phone_words.remove(&out.session.key());
                out.event(CardEvent::DifferentCard)
            }
            CardFailReason::SealChanged
            | CardFailReason::PhoneHalfChanged
            | CardFailReason::BadProof => {
                self.card.tmp_phone_words.remove(&out.session.key());
                self.card_failed_event(out, reason)
            }
            CardFailReason::Stopped => {
                out.request(CoordinatorCard::Stop);
                out.event(CardEvent::Failed {
                    reason,
                    device_suspect: false,
                    after_n: false,
                });
            }
            _ => self.card_failed_event(out, reason),
        }
    }

    fn card_suspect(&mut self, device_id: DeviceId) {
        self.mutate(Mutation::Card(CardMutation::DeviceSuspect { device_id }));
    }

    fn card_rec(&self, session: &CoordCardSession) -> CoordCardRecord {
        self.card
            .records
            .get(&session.key())
            .cloned()
            .unwrap_or_default()
    }

    fn card_mutate(
        &mut self,
        session: &CoordCardSession,
        mutation: impl FnOnce(AccessStructureRef, ShareIndex) -> CardMutation,
    ) {
        self.mutate(Mutation::Card(mutation(session.asref, session.share_index)));
    }

    /// Runs a user action on `device_id`'s card flow. The flow outlives an unplug, so the
    /// actions on words in RAM need no connected device. Owner B9: while an interruption waits
    /// for the user's answer, that answer is the only action taken.
    fn card_action(
        &mut self,
        device_id: DeviceId,
        action: impl FnOnce(&mut Self, &mut CardOut) -> Result<(), CardActionError>,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        self.card_action_with(device_id, false, action)
    }

    fn card_action_with(
        &mut self,
        device_id: DeviceId,
        answers_interruption: bool,
        action: impl FnOnce(&mut Self, &mut CardOut) -> Result<(), CardActionError>,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        if self.card.poisoned {
            return Err(CardActionError::Poisoned);
        }
        let pending = self
            .card
            .tmp_sessions
            .get(&device_id)
            .is_some_and(|session| self.card_rec(session).interruption_pending());
        if pending && !answers_interruption {
            return Err(CardActionError::WrongState(
                "answer whether you caused the interruption first",
            ));
        }
        let mut session = self
            .card
            .tmp_sessions
            .remove(&device_id)
            .ok_or(CardActionError::NoSession)?;
        let mut out = CardOut {
            device_id,
            session: &mut session,
            sends: vec![],
        };
        let result = action(self, &mut out);
        let sends = out.sends;
        self.card.tmp_sessions.insert(device_id, session);
        result.map(|()| sends)
    }
}
