//! The app API for the 64-word split card, design §5.5, in the packed format C′ (spec_v3 Part 4).
//!
//! The card flow is one stream of [`CardFlowState`] from [`Coordinator::start_card_backup`];
//! its stage says what the page shows. The device makes the card's seal, the phone pins it, and
//! then the device shows words 1–32 and quizzes on them. The user confirms on the phone that
//! they're written ([`Coordinator::card_device_half_written`]), and only then does the phone
//! show words 33–64 and quiz on them, while the device proves its seal and the phone verifies
//! each proof. [Continue] ([`Coordinator::card_final_begin`]) needs both halves recorded and
//! every proof verified. Then each screen quizzes 5 words of the other's half. The card restore
//! is the [`EnterPhysicalBackupState`] stream from [`Coordinator::tell_device_to_enter_card`],
//! whose `card_restore` says what the phone shows while the user enters words 33–64 here and
//! words 1–32 on the device.
//!
//! [`Coordinator::start_card_backup`]: crate::api::coordinator::Coordinator::start_card_backup
//! [`Coordinator::card_device_half_written`]: crate::api::coordinator::Coordinator::card_device_half_written
//! [`Coordinator::card_final_begin`]: crate::api::coordinator::Coordinator::card_final_begin
//! [`Coordinator::tell_device_to_enter_card`]: crate::api::coordinator::Coordinator::tell_device_to_enter_card

use crate::api::recovery::EnterPhysicalBackupState;
use crate::frb_generated::StreamSink;
use crate::sink_wrap::report_start_failure;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::card_backup::{
    CardAlarm, CardAlarmKind, CardFlowStage, CardFlowState, NextStep,
};
pub use frostsnap_coordinator::enter_physical_backup::CardRestoreView;
use frostsnap_core::coordinator::card;
use frostsnap_core::{AccessStructureRef, DeviceId, KeyId, RestorationId, SymmetricKey};

/// Words in each half of a card.
const CARD_HALF: usize = 32;

#[frb(mirror(CardFlowState))]
pub struct _CardFlowState {
    pub stage: CardFlowStage,
    pub device_connected: bool,
    /// Commitment points pulled from the device, out of [`card_seal_size`] (311).
    pub points_received: u16,
    /// Bit proofs verified against the pinned seal, out of [`card_seal_size`] (311). They come
    /// in while the user writes words 33–64.
    pub proofs_verified: u16,
    /// N was sent and the check hasn't ended: no Stop is offered (owner B12), and while the D
    /// check runs an unplug asks whether the user caused it (owner B9).
    pub check_started: bool,
    /// Words of the phone round answered correctly, 0 to 5.
    pub boxes_done: u8,
    /// Both halves are recorded on this phone: the app marks the backup complete the first
    /// time it reads true (design §3.10). Never set by the verdict alone.
    pub halves_recorded: bool,
    /// A later check: the device's quiz ended.
    pub recheck_device_done: bool,
    /// The phone's card record changed: re-emit the backup run.
    pub record_changed: bool,
    /// Owner B15: this install didn't create the wallet, so it never runs the verified check:
    /// the card is "written, not verified", and every page says so.
    pub unverified: bool,
}

/// The card flow's pages, in flow order (design §5.2, spec PK.7 as amended by owner B1).
#[frb(mirror(CardFlowStage))]
pub enum _CardFlowStage {
    /// Waiting for the device's first answer.
    Starting,
    /// The device is making the card's seal (about 20 s, the first time only).
    PreparingCard,
    /// Pulling the seal's points (`points_received`) and words 33–64, and pinning the seal.
    Pinning,
    /// P2: "Write words 1–32 from your device, then answer its quiz." Moves on by itself.
    DeviceHalf,
    /// "Have you written words 1–32?": the device's quiz passed. The hold is
    /// `card_device_half_written`; words 33–64 show only after it (owner B1).
    ConfirmDeviceHalf,
    /// P3: three words of 33–64 (two on the last page), from word `first_word`. Page 0..=10.
    /// Next and Back are `card_phone_next` and `card_phone_back`. The proofs come in behind it
    /// (`proofs_verified`).
    PhoneWords {
        page: u8,
        first_word: u8,
        words: Vec<String>,
    },
    /// P4: words 33–64 in two columns.
    PhoneAllWords {
        words: Vec<String>,
    },
    /// P5: "Check you've written: words 33–64", the hold (`card_phone_half_written`).
    PhoneConfirm,
    /// P6 and P15: "Select word N" among `options`; a pick is `card_self_check_answer`.
    /// `wrong`: the same item again after a wrong pick.
    SelfCheck {
        k: u8,
        total: u8,
        word_number: u8,
        options: Vec<String>,
        wrong: bool,
        recheck: bool,
    },
    /// "Checking your card": both halves are recorded and the device's proofs are still coming
    /// in (`proofs_verified`). Keep the device plugged in.
    Proving,
    /// P8: both halves recorded and every proof verified. [Continue] is `card_final_begin`.
    ReadyForFinalCheck,
    /// P9: the device round runs on the device: "Answer on your device."
    DeviceRound,
    /// P10: the device round passed: waiting for question `k`'s word.
    PreparingBox {
        k: u8,
    },
    /// P11: "Select word N": the options, then "None of these". The first pick is final.
    Question {
        k: u8,
        word_number: u8,
        options: Vec<String>,
        second_look: bool,
    },
    /// The first miss on question `k`: the same question follows as a second look.
    TryAgain {
        k: u8,
    },
    RoundSent,
    Passed,
    /// P15: a later check of a PASSED card, waiting for the device's words.
    RecheckWaiting,
    /// This phone's quiz in a later check passed.
    RecheckPassed,
    /// This phone can't check words 33–64; the device still checks its half.
    RecheckUnavailable,
    Failed {
        alarm: CardAlarm,
    },
    DifferentCard,
    Refused {
        message: String,
    },
    StartedOnAnotherPhone,
    StorageError,
    /// The device was unplugged: the flow ended and its progress is saved on the device.
    /// Continue starts a new flow (`start_card_backup`).
    Paused,
    /// After N, the device is plugged in with firmware that isn't the app's: the flow sends it
    /// nothing. "Update this device's firmware first."
    WrongFirmware,
    /// Owner B9: the check was interrupted after N. The page warns that this matters for
    /// security and asks whether the user caused it: `card_interruption_answer(true)` ("Yes, it
    /// was me") resumes on the same N once the device is back; `false` ("No, I didn't") marks
    /// the device suspect and ends the check. `disconnected`: the device disconnected (or
    /// restarted) while this page was open; otherwise the flow opened on a question left from
    /// before (the app was closed, or the device disconnected, while the check ran).
    CheckInterrupted {
        disconnected: bool,
        /// Owner R2-2: how many more "Yes, it was me" answers resume this check. At 1 the page
        /// warns that this is the last resume; at 0 a "yes" ends the check too (review B15
        /// protocol F2).
        resumes_left: u8,
    },
    /// Owner B15: this install didn't create the wallet and the device's card is unwritten:
    /// "This phone can't verify this backup", with [Write it unverified]
    /// (`card_write_unverified`).
    UnverifiedIntro,
    /// Owner B15: the card is written and both quizzes passed: "written, not verified". It
    /// must be verified on a trusted device that learns all 64 words.
    UnverifiedWritten,
}

#[frb(mirror(CardAlarm))]
pub struct _CardAlarm {
    pub kind: CardAlarmKind,
    pub device_suspect: bool,
    pub next_step: NextStep,
}

#[frb(mirror(CardAlarmKind))]
pub enum _CardAlarmKind {
    /// After N, the device's answers didn't hold up (or it stalled at its gate).
    PhoneRoundFailed,
    /// After N, the check ended some other way than finishing.
    EndedAfterN,
    /// After N, a Stop on either screen: only in records of pre-release builds (owner B12).
    StoppedAfterN,
    /// The device round failed; N never left the phone.
    DeviceRoundFailed,
    /// Before N, a Stop on either screen or a refusal.
    StoppedBeforeN,
    /// Before N, the device sent words 33–64 that aren't the pinned ones, or a bit proof that
    /// failed twice.
    DeviceFault,
    /// Owner B8: the device round failed after the device took N.
    DeviceRoundFailedAfterN,
    /// The device reported a seal other than the pinned one: it's marked suspect (owner B10).
    SealChanged,
    /// Owner B9: the check was interrupted after N and the user said they didn't cause it: the
    /// device is marked suspect.
    DisconnectedAfterN,
    /// Owner R2-2: a fourth interruption of the check, which the user said they caused: past
    /// the cap of 3 resumes, the device is marked suspect.
    InterruptedTooOften,
}

#[frb(mirror(NextStep))]
pub enum _NextStep {
    /// "Set this wallet up again."
    SetUpAgain,
    /// "Set this wallet up again without this device."
    WithoutThisDevice,
    /// "Set this wallet up again with a different phone."
    DifferentPhone,
    /// "Set this wallet up again with a different phone and a different Frostsnap device."
    DifferentPhoneAndDevice,
}

/// What the phone shows while a card restore runs (design §5.3 E4).
#[frb(mirror(CardRestoreView))]
pub enum _CardRestoreView {
    /// "Connecting to your device…"
    WaitingForDevice,
    /// The app's half isn't on the device yet. `device_half_in`: words 1–32 are in on the
    /// device.
    EnterHalves { device_half_in: bool },
    /// "Sent. Now finish words 1–32 on your device."
    WaitingForDeviceHalf,
    /// Both halves are in and the device decodes them, for a fixed 10 s.
    Checking,
    /// "Your card couldn't be read": the words don't decode, or decode to a card whose masks
    /// fail (owner B7). An edit of either half decodes again, or another phone.
    CouldNotRead,
    /// "Finish on your device: hold to restore." The device shows the report, threshold
    /// warnings included; nothing decoded reaches the app before the hold (owner B7).
    FinishOnDevice,
    /// "This card doesn't match this wallet. It may be another wallet's card, or it may have been
    /// written wrong." (owner B11 and its follow-up 4): it decoded and its self-checking masks
    /// passed, but it isn't for this wallet. Never counted as a failed read.
    WrongWallet,
}

/// A card share's status as the backup checklist and the wallet banner show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[frb(non_opaque)]
pub enum CardBackupStatus {
    NoCard,
    NotChecked,
    Checked,
    Failed,
    /// Owner B15: written on an install that didn't create the wallet, both quizzes passed,
    /// never verified.
    Unverified,
}

impl From<card::CardBackupStatus> for CardBackupStatus {
    fn from(status: card::CardBackupStatus) -> Self {
        match status {
            card::CardBackupStatus::NoCard => CardBackupStatus::NoCard,
            card::CardBackupStatus::NotChecked => CardBackupStatus::NotChecked,
            card::CardBackupStatus::Checked => CardBackupStatus::Checked,
            card::CardBackupStatus::Failed => CardBackupStatus::Failed,
            card::CardBackupStatus::Unverified => CardBackupStatus::Unverified,
        }
    }
}

/// The user's answer to one phone-round question (design §3.6). Its `Debug` names no word.
#[derive(Clone)]
pub enum CardAnswer {
    /// The user picked `word`, one of the question's options.
    Picked { word: String },
    /// "None of these": the word they wrote isn't among the options.
    NoneOfThese,
}

impl core::fmt::Debug for CardAnswer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CardAnswer::Picked { .. } => f.write_str("Picked"),
            CardAnswer::NoneOfThese => f.write_str("NoneOfThese"),
        }
    }
}

impl TryFrom<CardAnswer> for card::BoxAnswer {
    type Error = anyhow::Error;

    fn try_from(answer: CardAnswer) -> Result<Self> {
        Ok(match answer {
            CardAnswer::Picked { word } => card::BoxAnswer::Picked(word_index(&word)?),
            CardAnswer::NoneOfThese => card::BoxAnswer::NoneOfThese,
        })
    }
}

/// A word's index in the BIP39 list, in any case. The error never names the word: it may be a
/// card word.
pub(crate) fn word_index(word: &str) -> Result<u16> {
    card::word_index(&word.to_uppercase()).ok_or(anyhow!("that isn't a backup word"))
}

/// What a card restore decodes against (design §2.5).
#[derive(Clone, Debug)]
pub enum CardRestoreTarget {
    None,
    Wallet {
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    },
    Restoration {
        restoration_id: RestorationId,
    },
}

/// The number of commitment points in a card's seal, and of the bit proofs the phone verifies
/// against them (`layout::COMMITTED`).
#[frb(sync)]
pub fn card_seal_size() -> u16 {
    card::COMMIT_COUNT
}

/// Whether `word` (any case, spaces around it ignored) is a backup word: the check on each
/// field of the restore's words 33–64.
#[frb(sync)]
pub fn bip39_is_word(word: String) -> bool {
    word_index(word.trim()).is_ok()
}

/// The backup words that start with `prefix` (any case), in capitals: the suggestions of the
/// restore's keyboard for words 33–64.
#[frb(sync)]
pub fn bip39_words_with_prefix(prefix: String) -> Vec<String> {
    card::words_with_prefix(&prefix.trim().to_uppercase())
        .iter()
        .map(|word| word.to_string())
        .collect()
}

/// The letters (capitals) that follow `prefix` (any case) in some backup word: the keys the
/// restore's keyboard enables.
#[frb(sync)]
pub fn bip39_valid_next_letters(prefix: String) -> String {
    card::valid_next_letters(&prefix.trim().to_uppercase())
}

impl crate::api::coordinator::Coordinator {
    /// Starts (or, after a pause, resumes) `device_id`'s card flow. The device must be plugged
    /// in with the app's bundled firmware; a refusal arrives as the stream's error. A PASSED
    /// card goes to its later check (design §3.9).
    pub fn start_card_backup(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        sink: StreamSink<CardFlowState>,
    ) -> Result<()> {
        report_start_failure(sink, |sink| {
            self.0
                .start_card_backup(device_id, access_structure_ref, encryption_key, sink)
        })
    }

    /// Leaves the card flow (a pause). Sync, so it's done before a new flow can start.
    #[frb(sync)]
    pub fn cancel_card_backup(&self, device_id: DeviceId) {
        self.0.cancel_card_backup(device_id)
    }

    /// Next on a page of words 33–64: the next page, the page of all the words, then the
    /// confirm page.
    pub fn card_phone_next(&self, device_id: DeviceId) -> Result<()> {
        self.0.card_phone_next(device_id)
    }

    /// Back on a page of words 33–64, or on the confirm page.
    pub fn card_phone_back(&self, device_id: DeviceId) -> Result<()> {
        self.0.card_phone_back(device_id)
    }

    /// The hold on "Check you've written: words 33–64": the phone's quiz starts.
    pub fn card_phone_half_written(&self, device_id: DeviceId) -> Result<()> {
        self.0.card_phone_half_written(device_id)
    }

    /// One pick in the phone's quiz on words 33–64 (or a later check's). Says whether it was
    /// right; a wrong pick asks the same word again.
    pub fn card_self_check_answer(&self, device_id: DeviceId, word: String) -> Result<bool> {
        self.0.card_self_check_answer(device_id, word_index(&word)?)
    }

    /// The hold on "Have you written words 1–32?", offered once the device's quiz on them passed
    /// (owner B1). The device then starts proving its seal, and the phone shows words 33–64.
    pub fn card_device_half_written(&self, device_id: DeviceId) -> Result<()> {
        self.0.card_device_half_written(device_id)
    }

    /// Owner B15: [Write it unverified] on the unverified card's intro, on an install that
    /// didn't create the wallet: the device shows words 1–32, then this phone words 33–64, with
    /// both quizzes, and nothing is verified.
    pub fn card_write_unverified(&self, device_id: DeviceId) -> Result<()> {
        self.0.card_write_unverified(device_id)
    }

    /// [Continue] on "Your card is written and checked", offered only once both halves are
    /// recorded and every proof verified: N is drawn and kept on the phone, and the device round
    /// starts on the device.
    pub fn card_final_begin(&self, device_id: DeviceId) -> Result<()> {
        self.0.card_final_begin(device_id)
    }

    pub fn card_answer(&self, device_id: DeviceId, answer: CardAnswer) -> Result<()> {
        self.0.card_answer(device_id, answer.try_into()?)
    }

    /// Owner B9: the answer on the `CheckInterrupted` page. `caused_by_user`: "Yes, it was me"
    /// (the check resumes on the same N once the device is back) or "No, I didn't" (the device
    /// is marked suspect and the check ends). Returns its verdict if no flow heard it.
    pub fn card_interruption_answer(
        &self,
        device_id: DeviceId,
        caused_by_user: bool,
    ) -> Result<Option<CardFlowState>> {
        self.0.card_interruption_answer(device_id, caused_by_user)
    }

    #[frb(sync)]
    pub fn key_uses_card(&self, key_id: KeyId) -> bool {
        self.0.key_uses_card(key_id)
    }

    #[frb(sync)]
    pub fn devices_support_card(&self, devices: Vec<DeviceId>) -> bool {
        self.0.devices_support_card(&devices)
    }

    #[frb(sync)]
    pub fn device_is_suspect(&self, device_id: DeviceId) -> bool {
        self.0.inner().device_is_suspect(device_id)
    }

    pub fn emit_backup_run(&self, key_id: KeyId) -> Result<()> {
        self.0.backup_stream_emit(key_id)
    }

    /// Starts a card restore on `device_id`, which must be plugged in with the app's bundled
    /// firmware: the device opens its entry for words 1–32, and the app's words 33–64 go to it
    /// through `card_restore_phone_half`. The stream's `card_restore` is what the page shows.
    pub fn tell_device_to_enter_card(
        &self,
        device_id: DeviceId,
        target: CardRestoreTarget,
        sink: StreamSink<EnterPhysicalBackupState>,
    ) -> Result<()> {
        report_start_failure(sink, |sink| {
            self.0.tell_device_to_enter_card(device_id, target, sink)
        })
    }

    /// The app's words 33–64 for the card restore running on `device_id`: exactly 32, `None`
    /// for "Can't read this word". They go to the device once it has acknowledged the restore,
    /// and again after every call.
    pub fn card_restore_phone_half(
        &self,
        device_id: DeviceId,
        words: Vec<Option<String>>,
    ) -> Result<()> {
        if words.len() != CARD_HALF {
            return Err(anyhow!("words 33–64 are {CARD_HALF} words"));
        }
        let mut half = [None; CARD_HALF];
        for (slot, word) in half.iter_mut().zip(&words) {
            *slot = word.as_deref().map(word_index).transpose()?;
        }
        let result = self.0.card_restore_phone_half(device_id, half);
        half.iter_mut().for_each(|slot| *slot = None);
        result
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn backup_words() {
        assert!(bip39_is_word("zoo".into()));
        assert!(bip39_is_word(" Abandon ".into()));
        assert!(!bip39_is_word("zo".into()));
        assert!(!bip39_is_word(String::new()));
    }

    #[test]
    fn keyboard_helpers() {
        assert_eq!(bip39_words_with_prefix("zo".into()), ["ZONE", "ZOO"]);
        assert_eq!(bip39_words_with_prefix("ABANDON".into()), ["ABANDON"]);
        assert!(bip39_words_with_prefix("zz".into()).is_empty());
        assert_eq!(bip39_words_with_prefix(String::new()).len(), 2048);
        assert_eq!(bip39_valid_next_letters("q".into()), "U");
        assert_eq!(bip39_valid_next_letters("zo".into()), "NO");
        assert_eq!(bip39_valid_next_letters("zoo".into()), "");
        assert!(!bip39_valid_next_letters(String::new()).contains('X'));
    }

    #[test]
    fn the_seal_is_311_points() {
        assert_eq!(card_seal_size(), 311);
    }

    #[test]
    fn answers_name_no_word_in_their_error() {
        let error = word_index("notaword").unwrap_err().to_string();
        assert!(!error.contains("notaword"));
        let answer = CardAnswer::Picked { word: "zoo".into() };
        assert_eq!(format!("{answer:?}"), "Picked");
        assert_eq!(word_index("Zoo").unwrap(), 2047);
        assert!(matches!(
            card::BoxAnswer::try_from(CardAnswer::Picked { word: "zoo".into() }),
            Ok(card::BoxAnswer::Picked(2047))
        ));
    }
}
