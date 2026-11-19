//! Wire messages of the verifiable backup card: the 64-word split card, format C′ (spec PK.5).
//! Words 1–32 (j < 32) are the device half, shown only on the device; words 33–64 (32 ≤ j < 64)
//! are the phone half P, which reaches the phone in one 44-byte frame (`PhoneHalf`) and is
//! checked against H(P) inside the pinned h_C.
//!
//! Word positions are j = 0..63 everywhere: word number j + 1. Payloads are byte arrays, never
//! points. Every field that carries words is a redacted, wiped newtype. The wire is append-only:
//! retired requests keep their tags and are refused with `Unsupported`.

use super::*;
use crate::device::card::{redacted_secret, VerdictReason};
use frostsnap_macros::Kind as KindDerive;

/// What the device stores once its seal is computed and self-checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct CardSealed {
    pub format_id: u8,
    pub h_c: [u8; 32],
    pub coin_commit: [u8; 32],
}

/// 32 words of a card: a half, word j + 32 · half + 1 holding `0[j]`. Its `Debug` shows none of
/// them and it's wiped when dropped.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct CardWords32(pub [u16; 32]);

/// The phone round's 5 words, in `derive_boxes` order. Its `Debug` shows none of them and it's
/// wiped when dropped.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct CardWords5(pub [u16; 5]);

/// The app's words 33–64 for a restore; `None` is "Can't read this word". Its `Debug` shows none
/// of them and it's wiped when dropped.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct CardHalfEntries(pub [Option<u16>; 32]);

redacted_secret!(CardWords32, [0; 32]);
redacted_secret!(CardWords5, [0; 5]);
redacted_secret!(CardHalfEntries, [None; 32]);

/// Its `Debug` never shows N, which is the phone's secret until its round ends.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode, KindDerive)]
pub enum CoordinatorCard {
    /// Opens (or re-attaches to) a session for this share.
    Begin(Box<CardBegin>),
    /// count ≤ 32. Reply: `CommitPoints`, or `Refused{Busy | WrongState | BadRequest | NoSession}`.
    /// In SEALED and PROVING with no seal in the session it starts the seal, and is `Busy` until
    /// it's done. In WRITING, SELF_CHECK and CARD_CHECKED it's served from RAM only, else
    /// `WrongState`: no seal is computed while the user writes words 1–32 (spec PK.7).
    PullCommit { from: u16, count: u8 },
    /// The pinned h_C. SEALED → WRITING (persisted); later states up to PROVING resume their
    /// screen.
    WriteStart { h_c: [u8; 32] },
    /// count ≤ 16. Served in PROVING only, `Busy` until computed. Before PROVING it's `Busy` and
    /// arms nothing; in CHECKING it's `WrongState` (spec PK.7, build plan A4).
    PullProofs { from: u16, count: u8 },
    /// The phone's coin share N (B4). Owner B8: taken in PROVING only (the device's quiz passed
    /// and the phone's `DeviceHalfWritten` came), persisted before the coin is revealed; again
    /// with the same N it resumes. Any other N is `AlreadyUsed`.
    OpenStart { n: [u8; 32] },
    /// Reply: 3 × `OpeningPart`, or `Refused{Busy | NotInD | RunCap | AlreadyUsed | WrongState}`.
    /// Owner B8: served from PROVING (once the bit proofs are done) through the phone round.
    OpenBox { n: [u8; 32], j: u8 },
    /// Carries no result; the device's dots don't move on it.
    BoxAnswered { j: u8 },
    /// After the phone persisted its round result. A check the phone aborts after N sends
    /// `pass = false`.
    PhoneRound { n: [u8; 32], pass: bool },
    /// A phone Stop before N: CARD_CHECKED or PROVING → FAILED (spec L1.0.3 rule 6). Refused
    /// once N is taken (owner B12).
    Stop,
    /// Spec `CardFinalBegin`: start the device round, or resume it in a new session (B2).
    FinalBegin,
    /// Retired (format C′ has no phone-half openings): always `Refused{Unsupported}`.
    OpenPhoneWord { j: u8 },
    /// A later check of a PASSED card. `words_hash` is the phone's persisted h37, or else its
    /// h32, or `None`. The device sends `RecheckWords` only if it matches the h37 or h32 of its
    /// own card, otherwise `Refused{NoRecord}`; either way it runs its own quiz and sends
    /// `RecheckDone` when it ends. `Refused{WrongState}` unless PASSED.
    Recheck { words_hash: Option<[u8; 32]> },
    /// Retired (format C′ has no prepare stage): always `Refused{Unsupported}`.
    Prepare { h_c: [u8; 32] },
    /// Words 33–64, for the seal the phone pins or has pinned: `h_c` must be the device's own
    /// sealed h_C, as for `WriteStart` and `DeviceHalfWritten`, so words 33–64 go out only for
    /// a seal the phone names. That binds P to the seal it's checked against; it doesn't narrow
    /// who can ask, since every `Status` carries h_C (the audit-secrecy info item stays open
    /// until the authenticated link, design §8; review T5). Reply: `PhoneHalf`, or
    /// `Refused{BadRequest | Busy | WrongState | Closed | NoSession}`. Served from SEALED
    /// through CHECKING, before the verdict (spec PK.7, build plan A10).
    PullPhoneHalf { h_c: [u8; 32] },
    /// The user confirmed on the phone that words 1–32 are written (owner B1). With the
    /// persisted h_C, in CARD_CHECKED only: CARD_CHECKED → PROVING (persisted), and the 311 bit
    /// proofs start. No reply on success; `Refused{BadRequest | WrongState | Closed |
    /// Unsupported}` otherwise, and nothing is remembered.
    DeviceHalfWritten { h_c: [u8; 32] },
}

impl core::fmt::Debug for CoordinatorCard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use CoordinatorCard::*;
        match self {
            Begin(begin) => f.debug_tuple("Begin").field(begin).finish(),
            PullCommit { from, count } => f
                .debug_struct("PullCommit")
                .field("from", from)
                .field("count", count)
                .finish(),
            WriteStart { h_c } => f.debug_struct("WriteStart").field("h_c", h_c).finish(),
            PullProofs { from, count } => f
                .debug_struct("PullProofs")
                .field("from", from)
                .field("count", count)
                .finish(),
            OpenStart { .. } => f
                .debug_struct("OpenStart")
                .field("n", &format_args!(".."))
                .finish(),
            OpenBox { j, .. } => f
                .debug_struct("OpenBox")
                .field("n", &format_args!(".."))
                .field("j", j)
                .finish(),
            BoxAnswered { j } => f.debug_struct("BoxAnswered").field("j", j).finish(),
            PhoneRound { pass, .. } => f
                .debug_struct("PhoneRound")
                .field("n", &format_args!(".."))
                .field("pass", pass)
                .finish(),
            Stop => f.write_str("Stop"),
            FinalBegin => f.write_str("FinalBegin"),
            // The variant name only, as for every word-carrying or new message.
            OpenPhoneWord { .. } => f.write_str("OpenPhoneWord"),
            Recheck { .. } => f.write_str("Recheck"),
            Prepare { h_c } => f.debug_struct("Prepare").field("h_c", h_c).finish(),
            PullPhoneHalf { .. } => f.write_str("PullPhoneHalf"),
            DeviceHalfWritten { h_c } => f
                .debug_struct("DeviceHalfWritten")
                .field("h_c", h_c)
                .finish(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct CardBegin {
    pub share_index: ShareIndex,
    pub root_shared_key: SharedKey,
    pub format_id: u8,
    pub proof_version: u8,
}

#[derive(Clone, bincode::Encode, bincode::Decode, KindDerive)]
pub enum DeviceCard {
    /// In reply to `Begin` when `CardSealed` is stored (or the state is terminal); otherwise
    /// unsolicited, right after the seal job persists `CardSealed`. Boxed only to keep the
    /// enum small; it encodes as the status itself.
    Status(Box<CardStatus>),
    /// At most 32 points.
    CommitPoints {
        from: u16,
        points: Vec<[u8; 33]>,
    },
    /// At most 16 proofs.
    BitProofs {
        from: u16,
        proofs: Vec<[u8; 96]>,
    },
    /// Unsolicited, after CARD_CHECKED (the device's self-check of words 1–32 passed) is
    /// persisted.
    CheckDone,
    /// After `PhoneRound{n_tag, d}` is persisted (in PROVING, owner B8), or on a resume.
    OpenAccepted {
        coin: [u8; 32],
    },
    /// Three per opening of a word of D, sent back to back (every D word is a carry opening,
    /// owner B6).
    OpeningPart(CardOpeningPart),
    /// After PASSED or FAILED is persisted; sent again on a repeated `PhoneRound` or `Stop`.
    Verdict {
        passed: bool,
    },
    Refused {
        reason: CardRefusal,
    },
    /// Unsolicited: the gate screen is up in this device session, so the phone may show its
    /// question (spec §0.6.1.2 item 2). It carries nothing secret and answers no request.
    GateShown,
    /// After the device-round result is persisted. Sent again on `FinalBegin` once finished.
    DeviceRound {
        pass: bool,
    },
    /// Spec `RestoreOutcome`: at T_dec after the decode of the current halves, before any
    /// report screen. It carries the result alone: nothing decoded leaves the device before the
    /// user's hold, and a card whose masks fail is `CouldNotRead` like any other (owner B7). A
    /// genuine card for another wallet is `WrongWallet` (owner B11).
    RestoreOutcome {
        result: RestoreResult,
    },
    /// Restore only: the device's view of the two halves. The acknowledgement of `EnterCard`,
    /// and again on every change (a half in or out, a decode starting or ending).
    RestoreState {
        device_half_in: bool,
        phone_half_in: bool,
        decoding: bool,
    },
    /// A later check: words 33–64 (j ascending) and, when the phone's hash was its h37, the 5
    /// words of D in `derive_boxes` order.
    RecheckWords {
        phone_half: CardWords32,
        d_words: Option<CardWords5>,
    },
    /// The device's later-check quiz ended (retry until right, so always passed).
    RecheckDone,
    /// Words 33–64, in reply to `PullPhoneHalf`. The phone checks them against the hP inside
    /// the h_C it pins or has pinned (binding (1), spec PK.6).
    PhoneHalf(CardWords32),
}

impl core::fmt::Debug for DeviceCard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use DeviceCard::*;
        match self {
            Status(status) => f.debug_tuple("Status").field(status).finish(),
            CommitPoints { from, points } => f
                .debug_struct("CommitPoints")
                .field("from", from)
                .field("count", &points.len())
                .finish(),
            BitProofs { from, proofs } => f
                .debug_struct("BitProofs")
                .field("from", from)
                .field("count", &proofs.len())
                .finish(),
            OpenAccepted { coin } => f.debug_struct("OpenAccepted").field("coin", coin).finish(),
            Verdict { passed } => f.debug_struct("Verdict").field("passed", passed).finish(),
            Refused { reason } => f.debug_struct("Refused").field("reason", reason).finish(),
            DeviceRound { pass } => f.debug_struct("DeviceRound").field("pass", pass).finish(),
            RestoreOutcome { result } => f
                .debug_struct("RestoreOutcome")
                .field("result", result)
                .finish(),
            // The variant name only: an opening carries a word, and the rest are new.
            other => f.write_str(other.kind()),
        }
    }
}

/// What a restore's decode came to. It names no word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum RestoreResult {
    /// A share this device doesn't hold, from a card whose masks pass: its report and the
    /// hold are on the device.
    Restored,
    /// The words don't make a card, or make one whose masks fail (owner B7): the same answer
    /// whatever the target or the reason. An edit of either half decodes again.
    CouldNotRead,
    /// The device already holds this share or has a card record for it. The restore ends.
    AlreadyHeld,
    /// A card version this firmware doesn't know. The restore ends.
    NeedsNewerFirmware,
    /// A genuine card for another wallet (owner B11): it decoded uniquely and its self-checking
    /// masks passed, but it isn't for the F the phone sent. Never sent for a card whose self
    /// masks fail, which is `CouldNotRead`. The restore ends.
    WrongWallet,
}

#[derive(Clone, Debug, PartialEq, bincode::Encode, bincode::Decode)]
pub struct CardStatus {
    pub share_image: ShareImage,
    pub format_id: u8,
    pub proof_version: u8,
    pub sealed: CardSealed,
    pub state: CardStateReport,
    pub check: CardCheckReport,
    /// Some only in PASSED or FAILED, so a reconnecting phone can pick its alarm.
    pub reason: Option<VerdictReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum CardStateReport {
    Sealed,
    /// Words 1–32 on the device.
    Writing,
    /// The device's quiz on words 1–32.
    SelfCheck,
    /// The device's self-check passed; the phone hasn't confirmed words 1–32 yet.
    CardChecked,
    Checking,
    Passed,
    Failed,
    /// The user confirmed words 1–32 on the phone: the device computes the bit proofs while the
    /// phone half is written (owner B1).
    Proving,
}

/// Never carries E, k, the attempt or any answer (spec L2.7.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum CardCheckReport {
    None,
    /// `runs` counts the `OpenRun` markers of D so far.
    Started {
        n_tag: [u8; 32],
        runs: u8,
    },
    Ended {
        passed: bool,
    },
    /// CHECKING, the device round is running, no n_tag.
    DeviceRoundPending,
    /// CHECKING, the device round passed, no n_tag yet (`OpenStart` not received).
    DeviceRoundPassed,
    /// CHECKING, the device round is running, and N was taken before it (owner B8: the phone
    /// sends N right after "I've written words 1–32"). `Started` in PROVING or after a passed
    /// device round.
    StartedDeviceRoundPending {
        n_tag: [u8; 32],
        runs: u8,
    },
}

#[derive(Clone, bincode::Encode, bincode::Decode)]
pub enum CardOpeningPart {
    /// Part 0: the word and the 77 Q's.
    Carries {
        j: u8,
        y: u16,
        carries: Vec<[u8; 33]>,
    },
    /// Part 1 (first = 0, 39 proofs) and part 2 (first = 39, 38 proofs).
    Proofs {
        j: u8,
        first: u8,
        proofs: Vec<[u8; 96]>,
    },
}

/// The part and the word position only: never the word.
impl core::fmt::Debug for CardOpeningPart {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CardOpeningPart::Carries { j, .. } => f
                .debug_struct("Carries")
                .field("j", j)
                .finish_non_exhaustive(),
            CardOpeningPart::Proofs { j, first, .. } => f
                .debug_struct("Proofs")
                .field("j", j)
                .field("first", first)
                .finish_non_exhaustive(),
        }
    }
}

/// Why the device refuses a card request. The coordinator persists it (inside
/// `CardFailReason`), so it's append-only: a variant is never removed or reordered, and
/// `tests/coordinator_backward_compat.rs` pins the encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum CardRefusal {
    /// The format or proof version isn't this firmware's, or the card record was sealed in
    /// another card format.
    Unsupported,
    /// The index or the threshold is above 255.
    MetadataRange,
    /// t = 1 and i ≠ 1.
    ThresholdOne,
    /// A restored or legacy share.
    NotKeygenOrigin,
    /// A keygen share made without the card option.
    NotCardKey,
    WrongState,
    /// CHECKING with another n_tag.
    AlreadyUsed,
    NotInD,
    /// A run cap for the word is reached.
    RunCap,
    /// PASSED or FAILED.
    Closed,
    /// Not ready yet: retry.
    Busy,
    /// An unknown key or share, an image mismatch, a range, or an h_C mismatch.
    BadRequest,
    /// A session-bound request with no open session: send `Begin`.
    NoSession,
    /// The firmware runs card flows only on direct USB.
    NotDirectUsb,
    /// A self-check failed and the card job stopped.
    DeviceError,
    /// Retired with `OpenPhoneWord`; kept because the coordinator persists `CardRefusal`.
    NotInPhoneHalf,
    /// A recheck hash that matches neither the h37 nor the h32 of the device's card.
    NoRecord,
}
