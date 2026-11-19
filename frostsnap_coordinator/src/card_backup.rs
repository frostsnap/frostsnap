//! The verifiable backup card flow: the 64-word split card, format C′ (spec Part 4).
//!
//! [`CardBackupProtocol`] only stores state and timers: every check happens in core, inside
//! `recv_device_message` and the `card_*` actions, which the app runs through
//! [`persist_card_sends`] so nothing they send leaves before their mutations are on disk.
//!
//! The device seals the card (about 15 s) and the phone pins it, words 33–64 included, before
//! word 1. The device shows words 1–32 and quizzes on them; the user confirms them on the phone,
//! and the device computes the 311 bit proofs while the phone shows words 33–64 and quizzes on
//! them (owner B1). [Continue] needs both halves and the proofs. Then the device round and the
//! phone round each quiz 5 words of the other screen's half.
//!
//! Owner B12: once N is out (with the confirm of words 1–32, owner B8) there's no Stop. Owner
//! B9: while the D check runs, an unplug keeps the flow on its question, "did you cause it?",
//! and nothing goes to the device until the user answers; and the work the device owes has a
//! deadline ([`DEVICE_WORK_TIMEOUT`]), so a stall while plugged in is an alarm.

use crate::{
    firmware::{FirmwareUpgradeEligibility, FirmwareVersion},
    persist::{Persist, Persisted, TakeStaged},
    Completion, DeviceMode, Sink, UiProtocol, UiStack,
};
use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{
        card::{CardDeviceWork, CardEvent, CardFailReason, CardToUser, COMMIT_COUNT, PHONE_PAGES},
        CoordinatorSend, CoordinatorToUserMessage, FrostCoordinator, Mutation,
    },
    message::{
        card::{CardRefusal, DeviceCard},
        DeviceToCoordinatorMessage,
    },
    AccessStructureRef, DeviceId, KeyId, SymmetricKey,
};
use rand::seq::SliceRandom;
use rand_core::{CryptoRng, RngCore};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, Instant};
use tracing::{event, Level};

/// How long after a `Busy` the request is sent again.
const BUSY_RETRY: Duration = Duration::from_secs(1);
/// Owner B9: how long the device may take over one item of the work it owes while the D check
/// runs ([`CardDeviceWork`]: its `Status`, the coin, the next frame of bit proofs, the next
/// opening, its gate screen), from when that item began or the last one was done. Past it the
/// device is stalling, which is an abort (spec L1.7.6, review S3). The clock runs from N (the
/// coin is the first item), not from the gate screen, so a chip that saw D can't stall anywhere
/// while plugged in; an unplug is the question instead.
///
/// Why per item, not one deadline from N: the user-paced steps between them (writing words
/// 33–64, the device round, the phone round's questions) have no bound, and the device pauses
/// its job while the user touches its screen. The gap between two items of device work while
/// it owes one does have a bound. Honest times on the bench: the `Status` and the coin at once
/// (after the session's encode, a few seconds), all 311 proofs in about 63 s (a frame of them in
/// a few seconds), an opening in about 25 s. 120 s is almost 5 openings, and twice all the
/// proofs.
pub const DEVICE_WORK_TIMEOUT: Duration = Duration::from_secs(120);
/// Owner B9: the deadline for an opening a device in PROVING computes only after all its bit
/// proofs again (a resume, EARLY-N), when the phone pulled fewer than all of them from it in
/// this session (none, or only those it was missing; review P3): about 63 s + 25 s honest, so
/// twice [`DEVICE_WORK_TIMEOUT`].
pub const DEVICE_WORK_TIMEOUT_AFTER_PROOFS: Duration = Duration::from_secs(240);
/// How long the wait runs on past the deadline once an opening frame comes in. The app's loop
/// reads a frame per pass, so an honest opening's last frame is well within it; one that stops
/// part-way is still a stall.
const OPENING_GRACE: Duration = Duration::from_secs(10);

/// The deadline for one item of the device's owed work.
pub fn device_work_timeout(work: CardDeviceWork) -> Duration {
    match work {
        CardDeviceWork::Opening { proofs_first: true } => DEVICE_WORK_TIMEOUT_AFTER_PROOFS,
        _ => DEVICE_WORK_TIMEOUT,
    }
}

/// What the phone's card flow screen shows. Emitted on every change.
#[derive(Clone, Debug)]
pub struct CardFlowState {
    pub stage: CardFlowStage,
    pub device_connected: bool,
    /// Commitment points pulled from the device, out of 311 (`COMMIT_COUNT`).
    pub points_received: u16,
    /// Bit proofs verified against the pinned seal, out of 311 (`COMMIT_COUNT`).
    pub proofs_verified: u16,
    /// N was sent and the check hasn't ended: no Stop is offered (owner B12), and while the D
    /// check runs an unplug asks whether the user caused it (owner B9).
    pub check_started: bool,
    /// Words of the phone round answered correctly, 0 to 5.
    pub boxes_done: u8,
    /// Both halves are recorded on this phone (its `Checked` is persisted): the backup run is
    /// complete (design §3.10). Never set by the verdict alone.
    pub halves_recorded: bool,
    /// A later check: the device's quiz ended.
    pub recheck_device_done: bool,
    /// The phone's card record changed, so the backup run should be re-emitted.
    pub record_changed: bool,
    /// Owner B15: this install didn't run the wallet's keygen, so it never runs the verified
    /// check. It may write the card, with both quizzes, only as "written, not verified".
    pub unverified: bool,
}

/// The card flow's screens on the phone (design §3.7, §5.2). The order below is the flow's.
/// Its `Debug` never shows a word.
#[derive(Clone)]
pub enum CardFlowStage {
    /// Waiting for the device: plugged in, `Begin` sent, no `Status` yet.
    Starting,
    /// The device is making the card's seal (about 15 s, the first time only).
    PreparingCard,
    /// The phone is pulling the seal's commitment points (`points_received`) and words 33–64.
    Pinning,
    /// "Write words 1–32 from your device, then answer its quiz."
    DeviceHalf,
    /// "Have you written words 1–32?": the device's quiz passed, and the hold calls
    /// `card_device_half_written` (owner B1). Words 33–64 come only after it.
    ConfirmDeviceHalf,
    /// Three words of 33–64 (two on the last page), from word `first_word`. Page 0..=10.
    PhoneWords {
        page: u8,
        first_word: u8,
        words: Vec<String>,
    },
    /// Page 11: words 33–64 in two columns.
    PhoneAllWords {
        words: Vec<String>,
    },
    /// "Check you've written: words 33–64", the hold.
    PhoneConfirm,
    /// The phone's quiz: "Select word N" among `options` (the word and its two closest
    /// lookalikes, shuffled). `wrong`: the same item after a wrong pick.
    SelfCheck {
        k: u8,
        total: u8,
        word_number: u8,
        options: Vec<String>,
        wrong: bool,
        recheck: bool,
    },
    /// "Checking your card": the device computes the bit proofs and the phone verifies them
    /// (`proofs_verified` of 311). Keep the device plugged in. Shown when no page of words is.
    Proving,
    /// "Your card is written and checked. Last step: 5 questions on your device, then 5 here."
    /// The [Continue] (`card_final_begin`): both halves recorded and the seal proven.
    ReadyForFinalCheck,
    /// The device round runs on the device: "Answer on your device." N stays on the phone.
    DeviceRound,
    /// The device round passed and N went out: the phone waits for question `k`'s word.
    PreparingBox {
        k: u8,
    },
    /// "Select word N": the word and two of its six closest lookalikes, shuffled; the page adds
    /// "None of these". `second_look`: "Look again at word N on your card".
    Question {
        k: u8,
        word_number: u8,
        options: Vec<String>,
        second_look: bool,
    },
    TryAgain {
        k: u8,
    },
    RoundSent,
    Passed,
    /// A later check of a PASSED card: waiting for the device's words.
    RecheckWaiting,
    /// This phone's quiz in a later check passed.
    RecheckPassed,
    /// "Your device is checking its half. Words 33–64 can be checked only with the phone that
    /// made this card."
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
    /// The device was unplugged, or a `Cancel` dropped its session: the flow has ended, and
    /// its progress is saved on the device. A new flow resumes it (design addendum §6).
    Paused,
    /// A flow started after N without its device: the device is now plugged in, with firmware
    /// that isn't the app's, so the flow sends it nothing, not even its `Begin` (design §6.6).
    WrongFirmware,
    /// Owner B15: this install didn't run the wallet's keygen, and the device's card is
    /// unwritten. "This phone can't verify this backup": [Write it unverified]
    /// (`card_write_unverified`) writes it with both quizzes; nothing went to the device yet but
    /// `Begin`.
    UnverifiedIntro,
    /// Owner B15: the unverified card is written and both quizzes passed: "written, not
    /// verified". It must be verified on a trusted device that learns all 64 words.
    UnverifiedWritten,
    /// Owner B9: the check was interrupted after N, and the user is asked whether they caused
    /// it: "Yes, it was me" (`card_interruption_answer(true)`) resumes on the same N once the
    /// device is back; "No, I didn't" marks the device suspect and ends the check. Nothing goes
    /// to the device until then. `disconnected`: the device disconnected (or restarted) while
    /// this page was open; otherwise the flow opened on a question left from before (the app
    /// was closed, or the device disconnected, while the check ran).
    CheckInterrupted {
        disconnected: bool,
        /// Owner R2-2: how many more "Yes, it was me" answers resume this check. At 1 the page
        /// warns that this is the last resume; at 0 a "yes" ends the check too (review B15
        /// protocol F2).
        resumes_left: u8,
    },
}

impl fmt::Debug for CardFlowStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CardFlowStage::*;
        match self {
            DeviceHalf => write!(f, "DeviceHalf"),
            ConfirmDeviceHalf => write!(f, "ConfirmDeviceHalf"),
            PhoneWords { page, .. } => f
                .debug_struct("PhoneWords")
                .field("page", page)
                .finish_non_exhaustive(),
            PhoneAllWords { .. } => write!(f, "PhoneAllWords(..)"),
            SelfCheck { k, total, .. } => f
                .debug_struct("SelfCheck")
                .field("k", k)
                .field("total", total)
                .finish_non_exhaustive(),
            PreparingBox { k } => f.debug_struct("PreparingBox").field("k", k).finish(),
            Question { k, .. } => f
                .debug_struct("Question")
                .field("k", k)
                .finish_non_exhaustive(),
            TryAgain { k } => f.debug_struct("TryAgain").field("k", k).finish(),
            Failed { alarm } => f.debug_struct("Failed").field("alarm", alarm).finish(),
            Refused { message } => f.debug_struct("Refused").field("message", message).finish(),
            Starting => write!(f, "Starting"),
            PreparingCard => write!(f, "PreparingCard"),
            Pinning => write!(f, "Pinning"),
            PhoneConfirm => write!(f, "PhoneConfirm"),
            Proving => write!(f, "Proving"),
            ReadyForFinalCheck => write!(f, "ReadyForFinalCheck"),
            DeviceRound => write!(f, "DeviceRound"),
            RoundSent => write!(f, "RoundSent"),
            Passed => write!(f, "Passed"),
            RecheckWaiting => write!(f, "RecheckWaiting"),
            RecheckPassed => write!(f, "RecheckPassed"),
            RecheckUnavailable => write!(f, "RecheckUnavailable"),
            DifferentCard => write!(f, "DifferentCard"),
            StartedOnAnotherPhone => write!(f, "StartedOnAnotherPhone"),
            StorageError => write!(f, "StorageError"),
            Paused => write!(f, "Paused"),
            WrongFirmware => write!(f, "WrongFirmware"),
            CheckInterrupted {
                disconnected,
                resumes_left,
            } => f
                .debug_struct("CheckInterrupted")
                .field("disconnected", disconnected)
                .field("resumes_left", resumes_left)
                .finish(),
            UnverifiedIntro => write!(f, "UnverifiedIntro"),
            UnverifiedWritten => write!(f, "UnverifiedWritten"),
        }
    }
}

impl CardFlowStage {
    /// An end page: the flow's last word, which an unplug leaves standing.
    fn is_end(&self) -> bool {
        matches!(
            self,
            CardFlowStage::Passed
                | CardFlowStage::Failed { .. }
                | CardFlowStage::DifferentCard
                | CardFlowStage::Refused { .. }
                | CardFlowStage::StartedOnAnotherPhone
                | CardFlowStage::StorageError
                | CardFlowStage::Paused
                | CardFlowStage::CheckInterrupted { .. }
                | CardFlowStage::UnverifiedWritten
        )
    }

    /// A page that shows only words already in the phone's RAM, which keeps working while the
    /// device is unplugged (design §3.7).
    fn is_phone_only(&self) -> bool {
        matches!(
            self,
            CardFlowStage::PhoneWords { .. }
                | CardFlowStage::PhoneAllWords { .. }
                | CardFlowStage::PhoneConfirm
                | CardFlowStage::SelfCheck { recheck: false, .. }
        )
    }
}

/// A failed card's alarm: what failed, and the next step it offers (spec §0.6.4.4, §0.6.4.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardAlarm {
    pub kind: CardAlarmKind,
    /// The device is marked suspect, by this card or another key's.
    pub device_suspect: bool,
    pub next_step: NextStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardAlarmKind {
    /// After N, the device's answers didn't hold up (or it stalled at its gate).
    PhoneRoundFailed,
    /// After N, the check ended some other way than finishing: a refusal, or a device that
    /// can't resume.
    EndedAfterN,
    /// After N, a Stop on either screen: only in records of pre-release builds (owner B12: no
    /// Stop after N).
    StoppedAfterN,
    /// The device round failed. N never left the phone.
    DeviceRoundFailed,
    /// The check ended before N without a round failing: a Stop on either screen, or a
    /// refusal. Nobody knows D, so nobody gains from it.
    StoppedBeforeN,
    /// Before N, the device sent words 33–64 that aren't the pinned ones, or a bit proof that
    /// failed twice. The device is marked suspect.
    DeviceFault,
    /// Owner B8: the device round failed after the device took N (it knew D). Either side may
    /// be at fault, so both are left out.
    DeviceRoundFailedAfterN,
    /// The device reported a seal other than the one this phone pinned, before or after N. An
    /// honest device can't (its seal is deterministic), so it's marked suspect (owner B10): "This
    /// device showed a different backup partway through. Don't trust it."
    SealChanged,
    /// Owner B9: the check was interrupted after N and the user said they didn't cause it: the
    /// device is marked suspect.
    DisconnectedAfterN,
    /// Owner R2-2: the check was interrupted a fourth time and the user said they caused it:
    /// past the cap of 3 resumes, the device is marked suspect.
    InterruptedTooOften,
}

/// What to do after a failed card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextStep {
    /// "Set this wallet up again."
    SetUpAgain,
    /// "Set this wallet up again without this device."
    WithoutThisDevice,
    /// "Set this wallet up again with a different phone."
    DifferentPhone,
    /// "Set this wallet up again with a different phone and a different Frostsnap device."
    /// (owner B8).
    DifferentPhoneAndDevice,
}

impl CardAlarmKind {
    /// After N every end leaves the device out, and never sends the user to another phone: a
    /// stalling device would get a fresh draw of D there. Before N nobody knows D: a device
    /// round fail points at the phone, and only a device fault leaves the device out.
    pub fn next_step(self) -> NextStep {
        use CardAlarmKind::*;
        match self {
            PhoneRoundFailed | EndedAfterN | StoppedAfterN | DeviceFault | SealChanged
            | DisconnectedAfterN | InterruptedTooOften => NextStep::WithoutThisDevice,
            DeviceRoundFailed => NextStep::DifferentPhone,
            DeviceRoundFailedAfterN => NextStep::DifferentPhoneAndDevice,
            StoppedBeforeN => NextStep::SetUpAgain,
        }
    }
}

/// The firmware gate (design §6.6): a card flow or a card restore runs only with a device whose
/// firmware is the app's bundled image. The card's wire structs are a draft that may change in
/// place before the format freeze (spec PK-O3), and an unknown digest otherwise gets every
/// firmware feature, so a device with other firmware must never reach one.
pub fn check_card_firmware(
    device: &FirmwareVersion,
    bundled: Option<&FirmwareVersion>,
) -> anyhow::Result<()> {
    match bundled.map(|bundled| bundled.check_upgrade_eligibility(&device.digest)) {
        Some(FirmwareUpgradeEligibility::UpToDate) => Ok(()),
        _ => Err(anyhow::anyhow!("Update this device's firmware first")),
    }
}

/// Drives one device's card flow (design §3.7). Leaving the page is a pause, as an unplug is
/// (spec L1.7.6), and after N an interruption the next flow asks about, as an unplug is (owner
/// B9; review F1): the app sends that device alone a `Cancel`, so it goes home, and every other
/// device is left alone. An unplug ends the flow, as Frostsnap's flows do, except on a page that
/// shows only words already in the phone's RAM, which keeps working; the user resumes with a
/// new flow, whose `Begin` the device answers from its card record (design addendum §6).
pub struct CardBackupProtocol {
    device_id: DeviceId,
    access_structure_ref: AccessStructureRef,
    begin: CoordinatorSend,
    resend: Option<(Instant, CoordinatorSendMessage)>,
    send_begin: bool,
    /// Owner B9: the work the device owes while the D check runs, as core last gave it with
    /// its progress, while the device is plugged in and the flow talks to it.
    device_work: Option<(CardDeviceWork, u16)>,
    /// When that work is overdue: set when it changes, so each item done restarts the clock.
    work_due: Option<Instant>,
    /// This wait has already run on by [`OPENING_GRACE`]: it does once per item, so opening
    /// frames can't hold the deadline off for ever.
    opening_grace: bool,
    /// Owner B9: the page of words in RAM the interruption's question covered, shown again
    /// after "yes".
    covered_stage: Option<CardFlowStage>,
    /// The device is plugged in with firmware that isn't the app's: nothing goes to it.
    wrong_firmware: bool,
    /// The two lookalikes picked for each phone-round question, kept for its second look
    /// (design O11).
    round_picks: BTreeMap<u8, [String; 2]>,
    /// A later check: this phone's quiz passed, or it has none.
    recheck_phone_done: bool,
    state: CardFlowState,
    finished: Option<Completion>,
    sink: Box<dyn Sink<CardFlowState> + Send>,
}

impl CardBackupProtocol {
    /// Starts `device_id`'s card flow, as the app does. Nothing it stages needs persisting, but
    /// it runs through [`persist_card_sends`] anyway, so a storage error stops card handling.
    pub fn start<C>(
        coordinator: &mut Persisted<FrostCoordinator>,
        db: &mut C,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        rng: &mut (impl RngCore + CryptoRng),
        sink: impl Sink<CardFlowState> + 'static,
    ) -> anyhow::Result<Self>
    where
        FrostCoordinator: Persist<C> + TakeStaged<<FrostCoordinator as Persist<C>>::Update>,
    {
        let mut protocol = None;
        let started = persist_card_sends(coordinator, db, |coordinator| {
            protocol = Some(Self::new(
                coordinator,
                device_id,
                access_structure_ref,
                encryption_key,
                rng,
                sink,
            )?);
            anyhow::Ok(vec![])
        });
        match started {
            Ok(_) => Ok(protocol.expect("the handler made it")),
            Err(CardPersistError::Handler(error)) => Err(error),
            Err(CardPersistError::Storage { error, .. }) => {
                Err(error.context("couldn't save the card flow; restart the app to continue"))
            }
        }
    }

    fn new(
        coord: &mut FrostCoordinator,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        rng: &mut (impl RngCore + CryptoRng),
        sink: impl Sink<CardFlowState> + 'static,
    ) -> anyhow::Result<Self> {
        let begin = coord.card_begin(device_id, access_structure_ref, encryption_key, rng)?;
        let mut protocol = Self::with_begin(device_id, access_structure_ref, begin, sink);
        // A flow reopened after N, such as after an unplug or an app restart, offers no Stop.
        protocol.state.check_started = coord.card_check_started(device_id);
        protocol.state.halves_recorded = coord.card_halves_recorded(device_id);
        protocol.state.unverified = !coord.card_created_here(access_structure_ref.key_id);
        // Owner B9: an interruption the user hasn't answered (or an app closed mid-check, which
        // `card_begin` just recorded as one) is asked first, and nothing goes to the device
        // until then.
        if coord.card_interruption_pending(device_id) {
            protocol.state.stage = CardFlowStage::CheckInterrupted {
                disconnected: false,
                resumes_left: coord.card_resumes_left(device_id),
            };
        }
        Ok(protocol)
    }

    fn with_begin(
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        begin: CoordinatorSend,
        sink: impl Sink<CardFlowState> + 'static,
    ) -> Self {
        Self {
            device_id,
            access_structure_ref,
            begin,
            resend: None,
            send_begin: false,
            device_work: None,
            work_due: None,
            opening_grace: false,
            covered_stage: None,
            wrong_firmware: false,
            round_picks: BTreeMap::new(),
            recheck_phone_done: false,
            state: CardFlowState {
                stage: CardFlowStage::Starting,
                device_connected: false,
                points_received: 0,
                proofs_verified: 0,
                check_started: false,
                boxes_done: 0,
                halves_recorded: false,
                recheck_device_done: false,
                record_changed: false,
                unverified: false,
            },
            finished: None,
            sink: Box::new(sink),
        }
    }

    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }

    /// Emits the state with `record_changed` set for this emission only.
    fn emit_record_changed(&mut self) {
        self.state.record_changed = true;
        self.emit_state();
        self.state.record_changed = false;
    }

    /// The page the phone's Next asks core for (`card_phone_page`): the next page of words
    /// 33–64. After page 11 (all the words) it's the confirm page, which needs nothing from
    /// core: `None`.
    pub fn phone_next(&mut self) -> Option<u8> {
        match self.state.stage {
            CardFlowStage::PhoneWords { page, .. } => Some(page + 1),
            CardFlowStage::PhoneAllWords { .. } => {
                self.state.stage = CardFlowStage::PhoneConfirm;
                self.emit_state();
                None
            }
            _ => None,
        }
    }

    /// The page the phone's Back asks core for: the page before, or page 11 from the confirm
    /// page. None on page 0.
    pub fn phone_back(&self) -> Option<u8> {
        match self.state.stage {
            CardFlowStage::PhoneWords { page, .. } => page.checked_sub(1),
            CardFlowStage::PhoneAllWords { .. } => Some(PHONE_PAGES - 1),
            CardFlowStage::PhoneConfirm => Some(PHONE_PAGES),
            _ => None,
        }
    }

    /// The device sent a card message other than `Busy`: whatever a `Busy` asked to re-send
    /// has been answered. A stale `OpenBox` for a box already released would spend a run.
    pub fn device_replied(&mut self) {
        self.resend = None;
    }

    /// A persist of card mutations failed and core emitted no event to carry it.
    pub fn storage_error(&mut self) {
        self.resend = None;
        if self.finished.is_some() {
            return;
        }
        self.state.stage = CardFlowStage::StorageError;
        self.emit_state();
    }

    /// A card action's persisted sends are going out, in order. A request to the device replaces
    /// whatever a `Busy` asked to re-send. An action that sends the device nothing, such as a
    /// page of words in RAM or a Stop that is only a pause, leaves the re-send pending: it's the
    /// only thing that pulls the next frame.
    ///
    /// A request goes out behind the flow's `Begin` if the loop hasn't sent that yet, put first
    /// in `sends` here, so the device's session and screens follow the check. USB keeps the
    /// order, so nothing waits for the device. A flow held at the firmware gate, or with its
    /// device unplugged, sends the device nothing.
    pub fn action_sent(&mut self, sends: &mut Vec<CoordinatorSend>) {
        if self.wrong_firmware || !self.state.device_connected {
            sends.retain(|send| !matches!(send, CoordinatorSend::ToDevice { .. }));
            return;
        }
        if sends
            .iter()
            .any(|send| matches!(send, CoordinatorSend::ToDevice { .. }))
        {
            self.resend = None;
            if let Some(begin) = self.take_begin() {
                sends.insert(0, begin);
            }
        }
    }

    /// The re-send is cleared only by a reply from the device, a newer request, a disconnect or
    /// a storage error, never by an event: an action's events come with no reply. An ended flow
    /// takes no more events, such as one the device sent just before it was unplugged, except a
    /// paused flow's verdict: the loop handles an unplug before the messages that came ahead of
    /// it, and the verdict still stands.
    fn on_event(&mut self, event: CardEvent, now: Instant) {
        if self.finished.is_some()
            && !(matches!(self.state.stage, CardFlowStage::Paused)
                && matches!(
                    event,
                    CardEvent::Passed | CardEvent::Failed { .. } | CardEvent::DifferentCard
                ))
        {
            return;
        }
        let mut record_changed = false;
        let stage = match event {
            CardEvent::PreparingCard => CardFlowStage::PreparingCard,
            CardEvent::Pinning { points } => {
                self.state.points_received = points;
                CardFlowStage::Pinning
            }
            CardEvent::DifferentCard => {
                record_changed = true;
                CardFlowStage::DifferentCard
            }
            CardEvent::DeviceHalf => {
                // Pinning has finished: the record now holds the pin.
                record_changed = true;
                CardFlowStage::DeviceHalf
            }
            CardEvent::ConfirmDeviceHalf => CardFlowStage::ConfirmDeviceHalf,
            CardEvent::PhonePage {
                page,
                first_word,
                words,
            } => {
                let words = words.iter().map(|word| word.to_string()).collect();
                if page >= PHONE_PAGES {
                    CardFlowStage::PhoneAllWords { words }
                } else {
                    CardFlowStage::PhoneWords {
                        page,
                        first_word,
                        words,
                    }
                }
            }
            CardEvent::SelfCheckQuestion {
                k,
                total,
                word_number,
                word,
                lookalikes,
                wrong,
                recheck,
            } => {
                let mut options: Vec<String> = [word, lookalikes[0], lookalikes[1]]
                    .iter()
                    .map(|word| word.to_string())
                    .collect();
                options.shuffle(&mut rand::thread_rng());
                CardFlowStage::SelfCheck {
                    k,
                    total,
                    word_number,
                    options,
                    wrong,
                    recheck,
                }
            }
            CardEvent::PhoneSelfChecked => {
                // `Checked` is persisted: both halves are recorded.
                record_changed = true;
                self.state.halves_recorded = true;
                self.state.stage.clone()
            }
            CardEvent::CardFinished => {
                // `Finished` is persisted; the page of words that follows sets the stage.
                record_changed = true;
                self.state.stage.clone()
            }
            CardEvent::AlreadyFinished => CardFlowStage::Refused {
                message: "This card is finished.".into(),
            },
            CardEvent::DeviceRound => CardFlowStage::DeviceRound,
            // A page of words, or the quiz on them, stays up while the proofs come in behind it
            // (owner B1); once the phone's half is recorded, the proofs are what's left. The
            // confirm hold stays up too: the first page of words follows its first `Proving`.
            // So does `Starting` on a resumed flow (Paused, then Continue, or an app restart):
            // core sends `Proving` just before the page or quiz it resumes on, so "Checking your
            // card" doesn't flash in between (app review n1).
            CardEvent::Proving { verified, .. } => {
                self.state.proofs_verified = verified;
                if (self.state.stage.is_phone_only()
                    || matches!(
                        self.state.stage,
                        CardFlowStage::ConfirmDeviceHalf | CardFlowStage::Starting
                    ))
                    && !self.state.halves_recorded
                {
                    self.state.stage.clone()
                } else {
                    CardFlowStage::Proving
                }
            }
            CardEvent::Proven => {
                self.state.proofs_verified = COMMIT_COUNT;
                self.state.stage.clone()
            }
            CardEvent::ReadyForFinalCheck => {
                self.state.proofs_verified = COMMIT_COUNT;
                CardFlowStage::ReadyForFinalCheck
            }
            CardEvent::PreparingBox { k } => {
                self.state.check_started = true;
                self.state.boxes_done = k;
                CardFlowStage::PreparingBox { k }
            }
            CardEvent::Question {
                k,
                word_number,
                word,
                pool,
                second_look,
            } => {
                self.state.check_started = true;
                self.state.boxes_done = k;
                let rng = &mut rand::thread_rng();
                let picked = self.round_picks.entry(k).or_insert_with(|| {
                    let mut pool: Vec<&str> = pool.to_vec();
                    pool.shuffle(rng);
                    [pool[0].to_string(), pool[1].to_string()]
                });
                let mut options = vec![word.to_string(), picked[0].clone(), picked[1].clone()];
                options.shuffle(rng);
                CardFlowStage::Question {
                    k,
                    word_number,
                    options,
                    second_look,
                }
            }
            CardEvent::TryAgain { k } => CardFlowStage::TryAgain { k },
            CardEvent::GateShown => {
                self.state.check_started = true;
                return;
            }
            CardEvent::RoundSent { answered } => {
                self.state.check_started = true;
                self.state.boxes_done = answered;
                record_changed = true;
                CardFlowStage::RoundSent
            }
            CardEvent::Passed => {
                record_changed = true;
                CardFlowStage::Passed
            }
            CardEvent::Failed {
                reason,
                device_suspect,
                after_n,
            } => {
                record_changed = true;
                failed_stage(reason, device_suspect, after_n)
            }
            CardEvent::Refused { reason } => CardFlowStage::Refused {
                message: refusal_message(reason).into(),
            },
            CardEvent::StartedOnAnotherPhone => CardFlowStage::StartedOnAnotherPhone,
            CardEvent::RecheckWaiting => CardFlowStage::RecheckWaiting,
            CardEvent::RecheckPassed => {
                self.recheck_phone_done = true;
                CardFlowStage::RecheckPassed
            }
            CardEvent::RecheckUnavailable => {
                self.recheck_phone_done = true;
                CardFlowStage::RecheckUnavailable
            }
            CardEvent::RecheckDeviceDone => {
                self.state.recheck_device_done = true;
                self.state.stage.clone()
            }
            CardEvent::Busy { resend } => {
                self.resend = Some((
                    now + BUSY_RETRY,
                    CoordinatorSendMessage::to(self.device_id, CoordinatorSendBody::Core(*resend)),
                ));
                return;
            }
            // Owner B9: core sends `Interrupted` instead while the D check runs, so this is
            // before N, or for the verdict after the phone round.
            CardEvent::NeedsBegin => {
                self.send_begin = true;
                return;
            }
            CardEvent::StorageError => CardFlowStage::StorageError,
            // Owner B12: N is persisted; no Stop from here on.
            CardEvent::CheckStarted => {
                self.state.check_started = true;
                self.state.stage.clone()
            }
            // Owner B9: the question covers the page; nothing goes to the device until the user
            // answers. A page of words in RAM comes back after "yes".
            CardEvent::Interrupted { resumes_left } => {
                self.resend = None;
                self.send_begin = false;
                self.clear_device_work();
                if self.state.stage.is_phone_only() {
                    self.covered_stage = Some(self.state.stage.clone());
                }
                CardFlowStage::CheckInterrupted {
                    disconnected: true,
                    resumes_left,
                }
            }
            CardEvent::UnverifiedIntro => CardFlowStage::UnverifiedIntro,
            // Owner B15: written, not verified, and persisted.
            CardEvent::UnverifiedWritten => {
                record_changed = true;
                CardFlowStage::UnverifiedWritten
            }
            // Owner B9: "Yes, it was me": the flow sends `Begin` again once the device is in, and
            // the device must report this check on the same N. Until its first answer the page
            // is the one the question covered, or "Starting".
            CardEvent::Resuming => {
                self.send_begin = self.state.device_connected;
                match self.covered_stage.take() {
                    Some(stage) => stage,
                    None if self.wrong_firmware => CardFlowStage::WrongFirmware,
                    None => CardFlowStage::Starting,
                }
            }
        };

        if matches!(
            stage,
            CardFlowStage::Passed
                | CardFlowStage::Failed { .. }
                | CardFlowStage::DifferentCard
                | CardFlowStage::UnverifiedWritten
        ) {
            self.state.check_started = false;
            self.covered_stage = None;
            self.clear_device_work();
            self.finished.get_or_insert(Completion::Success);
        }
        // A later check is over once both screens' quizzes are.
        if self.recheck_phone_done && self.state.recheck_device_done {
            self.finished.get_or_insert(Completion::Success);
        }
        self.state.stage = stage;
        if record_changed {
            self.emit_record_changed();
        } else {
            self.emit_state();
        }
    }

    /// Owner B9: the interruption's question is up, so nothing goes to the device.
    fn interrupted(&self) -> bool {
        matches!(self.state.stage, CardFlowStage::CheckInterrupted { .. })
    }

    fn clear_device_work(&mut self) {
        self.device_work = None;
        self.work_due = None;
        self.opening_grace = false;
    }

    /// Owner B9: the work the device owes now ([`FrostCoordinator::card_device_work`]), which
    /// the app's loop passes on each pass. Its deadline starts when it changes, so each item of
    /// progress restarts the clock. It runs only while the device is plugged in and the flow
    /// talks to it: an unplug is the question instead, and a replug starts it again.
    pub fn track_device_work(&mut self, work: Option<(CardDeviceWork, u16)>, now: Instant) {
        let work = work.filter(|_| self.sends_to_device() && !self.interrupted());
        if work != self.device_work {
            self.device_work = work;
            self.work_due = work.map(|(work, _)| now + device_work_timeout(work));
            self.opening_grace = false;
        }
    }

    /// Whether the device's owed work is overdue while it is still plugged in. The app then
    /// aborts the check through [`CardInbox::device_timeout`].
    pub fn device_overdue(&self, now: Instant) -> bool {
        self.state.device_connected
            && self.finished.is_none()
            && self.work_due.is_some_and(|due| due <= now)
    }

    /// The device's session is gone, so the flow ends: what the page showed no longer stands
    /// (a question, for one, may show only while the device shows its gate, spec §0.6.1.2 item
    /// 2). Core keeps a verified question for the next flow, which asks it again once the
    /// device's gate is up. An end page stays, such as a storage error, whose "restart the app"
    /// a Continue couldn't get past.
    fn pause(&mut self) {
        self.resend = None;
        self.send_begin = false;
        self.clear_device_work();
        self.finished.get_or_insert(Completion::Abort {
            send_cancel_to_all_devices: false,
        });
        if !self.state.stage.is_end() {
            self.state.stage = CardFlowStage::Paused;
        }
        self.emit_state();
    }

    /// The device is plugged in with firmware that isn't the app's bundled one, so it never
    /// gets a `Begin` (design §6.6). Only a flow started after N without its device gets here
    /// (any other start needs the gate passed): it stays on its own page, or on the question an
    /// interruption asks (owner B9), and sends nothing. The app calls this before `connected`,
    /// which then does nothing.
    pub fn firmware_refused(&mut self, id: DeviceId) {
        if id == self.device_id && self.finished.is_none() {
            self.state.device_connected = true;
            self.wrong_firmware = true;
            self.send_begin = false;
            self.resend = None;
            self.clear_device_work();
            if !self.interrupted() {
                self.state.stage = CardFlowStage::WrongFirmware;
            }
            self.emit_state();
        }
    }

    fn process_to_user_message_at(
        &mut self,
        message: CoordinatorToUserMessage,
        now: Instant,
    ) -> bool {
        match message {
            CoordinatorToUserMessage::Card(CardToUser {
                device_id,
                access_structure_ref,
                event,
                ..
            }) if device_id == self.device_id
                && access_structure_ref == self.access_structure_ref =>
            {
                self.on_event(event, now);
                true
            }
            _ => false,
        }
    }

    /// An ended flow sends nothing: a `Begin` after an unplug would restart the check on the
    /// device by itself. Nor does one held at the firmware gate.
    fn sends_to_device(&self) -> bool {
        self.state.device_connected && self.finished.is_none() && !self.wrong_firmware
    }

    /// The flow's `Begin`, once, if it's due: from the loop's poll, or ahead of a card action's
    /// request ([`action_sent`](Self::action_sent)), whichever comes first.
    fn take_begin(&mut self) -> Option<CoordinatorSend> {
        (self.send_begin && self.sends_to_device()).then(|| {
            self.send_begin = false;
            self.begin.clone()
        })
    }

    fn poll_at(&mut self, now: Instant) -> Vec<CoordinatorSendMessage> {
        if !self.sends_to_device() {
            return vec![];
        }
        let mut out = vec![];
        if let Some(begin) = self.take_begin() {
            out.push(begin.try_into().expect("will only send messages to device"));
        }
        if let Some((due, _)) = &self.resend {
            if *due <= now {
                let (_, message) = self.resend.take().expect("just checked");
                out.push(message);
            }
        }
        out
    }
}

/// The user left `device_id`'s card flow, or a new one replaces it: it ends, as a pause. Says
/// whether it was live, in which case the app sends that device alone a `Cancel`, so the device
/// ends its session and goes home, as on an unplug, rather than run on with no page (a device
/// round that passed would take N and wait at a gate no phone serves). A flow that had ended
/// (paused, or at its verdict) has a device that is home or on its own verdict, and needs
/// nothing. Nor does a later check whose phone quiz is done: the device's quiz runs on to its
/// own Exit, and may be the only check left.
pub fn leave_card_flow(ui_stack: &mut UiStack, device_id: DeviceId) -> bool {
    let live = std::cell::Cell::new(false);
    // A card flow never asks for a `Cancel` to every device.
    let _ = ui_stack.cancel_where::<CardBackupProtocol>(|card| {
        let this = card.device_id == device_id;
        // Owner B15: a written unverified card leaves the device in CARD_CHECKED on "use your
        // phone", with nothing more coming: it goes home too.
        let device_waits = (card.finished.is_none() && !card.recheck_phone_done)
            || matches!(card.state.stage, CardFlowStage::UnverifiedWritten);
        if this && device_waits {
            live.set(true);
        }
        this
    });
    live.get()
}

/// The page for a card action's verdict that no flow heard, as its flow would show it. The app
/// drops a flow once it has ended, such as on an unplug, so an action can reach core with no
/// flow left: an answer to an interruption's question (owner B9), which core records without
/// the device. Its page shows this instead of the ended flow's last page, and picks up the
/// record.
pub fn unheard_verdict(
    message: &CoordinatorToUserMessage,
    device_connected: bool,
) -> Option<CardFlowState> {
    let CoordinatorToUserMessage::Card(CardToUser { event, .. }) = message else {
        return None;
    };
    let stage = match *event {
        CardEvent::Passed => CardFlowStage::Passed,
        CardEvent::Failed {
            reason,
            device_suspect,
            after_n,
        } => failed_stage(reason, device_suspect, after_n),
        CardEvent::DifferentCard => CardFlowStage::DifferentCard,
        _ => return None,
    };
    Some(CardFlowState {
        stage,
        device_connected,
        points_received: 0,
        proofs_verified: 0,
        check_started: false,
        boxes_done: 0,
        halves_recorded: false,
        recheck_device_done: false,
        record_changed: true,
        unverified: false,
    })
}

/// The next step a failed card's alarm offered, for the backup checklist to repeat once the
/// flow is closed (app review M1): a failed share is never left with no way forward. `None`
/// for a failure with its own page and no alarm (a different card at the pin).
pub fn failed_next_step(
    reason: CardFailReason,
    device_suspect: bool,
    after_n: bool,
) -> Option<NextStep> {
    match failed_stage(reason, device_suspect, after_n) {
        CardFlowStage::Failed { alarm } => Some(alarm.next_step),
        _ => None,
    }
}

/// The stage for a `Failed` event (design §6.5, spec §0.6.4.4).
///
/// Core says whether N had gone out (addendum §1.2 row 51), so nothing is guessed from the
/// suspect mark. Before N a different card (at the pin) is its own page, and words 33–64 that
/// aren't the pinned ones or a bit proof that failed twice are a device fault (build plan C16).
/// A changed seal, on either side of N, is its own alarm with the device left out (owner B10),
/// and so is an interruption the user didn't cause (owner B9). A stall after N counts as a
/// phone-round fail, as core records it (review S3). Owner B12: there's no Stop after N, so
/// `StoppedAfterN` comes only from records of pre-release builds.
fn failed_stage(reason: CardFailReason, device_suspect: bool, after_n: bool) -> CardFlowStage {
    use CardAlarmKind as Kind;
    use CardFailReason::*;
    let kind = if after_n {
        match reason {
            Stopped => Kind::StoppedAfterN,
            TwoMisses
            | BadOpening
            | WrongBox
            | WordRange
            | CoinMismatch
            | BoxesMismatch
            | DeviceVerdictMismatch
            | UnexpectedVerdict
            | PhoneHalfChanged => Kind::PhoneRoundFailed,
            // Owner B8: the chip knew D, so a failed device round leaves both out.
            DeviceRoundFailed => Kind::DeviceRoundFailedAfterN,
            // Owner B9: a stall isn't a mismatch: no words were compared (app review APP-3).
            DifferentCard | BadProof | Refused(_) | PhoneTimeout => Kind::EndedAfterN,
            SealChanged => Kind::SealChanged,
            Interrupted => Kind::DisconnectedAfterN,
            TooManyInterruptions => Kind::InterruptedTooOften,
        }
    } else {
        match reason {
            DifferentCard => return CardFlowStage::DifferentCard,
            SealChanged => Kind::SealChanged,
            // Owner B9: N went out with the confirm of words 1–32, before the device round.
            Interrupted => Kind::DisconnectedAfterN,
            TooManyInterruptions => Kind::InterruptedTooOften,
            DeviceRoundFailed => Kind::DeviceRoundFailed,
            BadOpening | WrongBox | WordRange | BadProof | PhoneHalfChanged => Kind::DeviceFault,
            Stopped => Kind::StoppedBeforeN,
            // Owner B8: N went out before the device round, so a device that refuses or
            // misbehaves while words 33–64 are written is an abort, and suspect.
            Refused(_)
            | CoinMismatch
            | TwoMisses
            | DeviceVerdictMismatch
            | UnexpectedVerdict
            | BoxesMismatch
            | PhoneTimeout
                if device_suspect =>
            {
                Kind::EndedAfterN
            }
            Refused(_)
            | CoinMismatch
            | TwoMisses
            | DeviceVerdictMismatch
            | UnexpectedVerdict
            | BoxesMismatch
            | PhoneTimeout => Kind::StoppedBeforeN,
        }
    };
    CardFlowStage::Failed {
        alarm: CardAlarm {
            kind,
            device_suspect,
            next_step: kind.next_step(),
        },
    }
}

/// The copy for a device's refusal.
pub(crate) fn refusal_message(reason: CardRefusal) -> &'static str {
    use CardRefusal::*;
    match reason {
        NotDirectUsb => "Plug the device straight into this phone or computer.",
        // Review B15 protocol F3: on an install that isn't the card keygen's, [Backup] asks the
        // device, which refuses a share that has no card. Neither message blames the device:
        // a 25-word wallet made on this phone before cards gets one of these.
        NotKeygenOrigin => {
            "This device's key was restored, or made before backup cards, so it has no card to \
             write. Keep the backup it has."
        }
        NotCardKey => {
            "This device's key was made with a 25-word backup, not a card, so it has no card to \
             write. Keep its 25-word backup."
        }
        Unsupported => {
            "This device's firmware can't make this card, or the card was started in a format \
             it doesn't support. Update it, or set this wallet up again."
        }
        MetadataRange | ThresholdOne => "This key's share can't be written on a card.",
        DeviceError => "The device stopped the card because a self-check failed.",
        Closed => "This card's check has already ended.",
        RunCap => "This card can't be finished. Make a new key.",
        NoRecord => "Check this card with the phone that made it.",
        WrongState | AlreadyUsed | NotInD | Busy | BadRequest | NoSession | NotInPhoneHalf => {
            "The device refused this step of the card."
        }
    }
}

impl UiProtocol for CardBackupProtocol {
    fn cancel(&mut self) {
        // A pause. Nothing is broadcast: the app sends this device alone its `Cancel`.
        self.finished.get_or_insert(Completion::Abort {
            send_cancel_to_all_devices: false,
        });
    }

    fn is_complete(&self) -> Option<Completion> {
        self.finished.clone()
    }

    /// The device is plugged in: the first connection, or a reconnect while the flow went on
    /// (a page of words in RAM, or after N). The device boots fresh, so it's sent `Begin`, unless
    /// the interruption's question is up (owner B9): then only after "yes".
    fn connected(&mut self, id: DeviceId, _state: DeviceMode) {
        if id == self.device_id && !self.state.device_connected && self.finished.is_none() {
            self.send_begin = !self.interrupted();
            self.state.device_connected = true;
            self.emit_state();
        }
    }

    /// The device has no battery, so an unplug reboots it to its home screen. A page of words
    /// already in RAM keeps working (design §3.7). Owner B9: after N the flow goes on too, and
    /// while the D check runs core's `Interrupted` puts the question up; the flow resumes after
    /// "yes" once the device is back. Anything else pauses.
    fn disconnected(&mut self, id: DeviceId) {
        if id != self.device_id {
            return;
        }
        self.state.device_connected = false;
        self.wrong_firmware = false;
        self.clear_device_work();
        let goes_on =
            self.state.stage.is_phone_only() || self.state.check_started || self.interrupted();
        if self.finished.is_none() && goes_on {
            self.resend = None;
            self.send_begin = false;
            self.emit_state();
        } else {
            self.pause();
        }
    }

    /// Another protocol's `Cancel` dropped the device's session, and the device went home.
    fn devices_cancelled(&mut self) {
        self.pause();
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        self.process_to_user_message_at(message, Instant::now())
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        self.poll_at(Instant::now())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Why [`persist_card_sends`] sent nothing.
#[derive(Debug)]
pub enum CardPersistError<E> {
    /// The handler refused. It staged nothing.
    Handler(E),
    /// The handler's mutations didn't persist. Its sends were dropped and card handling is
    /// poisoned until the app restarts. `storage_error` is the `StorageError` event for the
    /// user, when the handler emitted an event to address it with.
    Storage {
        error: anyhow::Error,
        storage_error: Option<Box<CoordinatorToUserMessage>>,
    },
}

/// Runs a card handler or action and persists what it staged. What it returns is handed back
/// only once that persist succeeded, so nothing sent to the device (such as N) depends on
/// state that is only in memory (design §2.1 item 5).
pub fn persist_card_sends<C, E>(
    coordinator: &mut Persisted<FrostCoordinator>,
    db: &mut C,
    handler: impl FnOnce(&mut FrostCoordinator) -> Result<Vec<CoordinatorSend>, E>,
) -> Result<Vec<CoordinatorSend>, CardPersistError<E>>
where
    FrostCoordinator: Persist<C> + TakeStaged<<FrostCoordinator as Persist<C>>::Update>,
{
    let mut handled = None;
    let persisted = coordinator.staged_mutate(db, |coordinator| {
        handled = Some(handler(coordinator));
        Ok(())
    });
    let handled = handled.expect("staged_mutate runs the mutator first");
    match persisted {
        Ok(()) => handled.map_err(CardPersistError::Handler),
        Err(error) => {
            coordinator.MUTATE_NO_PERSIST().card_poison();
            let storage_error = handled
                .ok()
                .into_iter()
                .flatten()
                .find_map(|send| match send {
                    CoordinatorSend::ToUser(CoordinatorToUserMessage::Card(card)) => {
                        Some(Box::new(CoordinatorToUserMessage::Card(CardToUser {
                            event: CardEvent::StorageError,
                            ..card
                        })))
                    }
                    _ => None,
                });
            Err(CardPersistError::Storage {
                error,
                storage_error,
            })
        }
    }
}

/// The card side of one pass of the app's loop: the card messages it received, and what they
/// tell the [`CardBackupProtocol`] once the outbox is drained.
#[derive(Default)]
pub struct CardInbox {
    /// Per device, whether its last card message was `Busy`.
    last_busy: BTreeMap<DeviceId, bool>,
    /// Devices whose message's persist failed with no event to carry the `StorageError`.
    storage_failed: BTreeSet<DeviceId>,
    /// Keys whose card records a message changed, persisted.
    changed_keys: BTreeSet<KeyId>,
    /// Devices that sent an opening frame core accepted: they're answering.
    opening_parts: BTreeSet<DeviceId>,
}

impl CardInbox {
    /// Handles a card message through [`persist_card_sends`]. What it returns goes in the
    /// outbox: the handler's sends once what it staged is persisted, and after a failed persist
    /// only the `StorageError` event. `port_shared`: the app sees another device on `from`'s
    /// USB port ([`UsbSerialManager::shares_port`](crate::UsbSerialManager::shares_port)).
    /// Only then is a `NotDirectUsb` after N a link refusal rather than an abort (re-attack
    /// RA-1).
    pub fn recv<C>(
        &mut self,
        coordinator: &mut Persisted<FrostCoordinator>,
        db: &mut C,
        from: DeviceId,
        port_shared: bool,
        message: DeviceToCoordinatorMessage,
    ) -> Vec<CoordinatorSend>
    where
        FrostCoordinator: Persist<C> + TakeStaged<<FrostCoordinator as Persist<C>>::Update>,
    {
        match message {
            // Not a reply: a re-send it came after is still due.
            DeviceToCoordinatorMessage::Card(
                DeviceCard::GateShown
                | DeviceCard::CheckDone
                | DeviceCard::DeviceRound { .. }
                | DeviceCard::RecheckDone
                | DeviceCard::RestoreState { .. }
                | DeviceCard::RestoreOutcome { .. },
            ) => {}
            DeviceToCoordinatorMessage::Card(DeviceCard::Refused {
                reason: CardRefusal::Busy,
            }) => {
                self.last_busy.insert(from, true);
            }
            _ => {
                self.last_busy.insert(from, false);
            }
        }
        let opening_part = matches!(
            &message,
            DeviceToCoordinatorMessage::Card(DeviceCard::OpeningPart(_))
        );
        let mut answering = false;
        let sends = self.persist(coordinator, db, from, |coordinator| {
            coordinator.card_port_shared(from, port_shared);
            let result = coordinator.recv_device_message(from, message);
            answering = opening_part && result.is_ok();
            result
        });
        if answering {
            self.opening_parts.insert(from);
        }
        sends
    }

    /// The phone's deadline after N (review S3, owner B9): the work `card`'s device owes is
    /// tracked from core each pass, and once it's overdue core aborts the check as for any other
    /// end after N, persisted before anything is sent. It fires once; what it returns goes in
    /// the outbox. Run after this pass's card messages: an opening frame among them that core
    /// accepted is the device answering, and the opening completes with its last frame, so the
    /// wait runs on by [`OPENING_GRACE`] instead, once per item.
    pub fn device_timeout<C>(
        &mut self,
        coordinator: &mut Persisted<FrostCoordinator>,
        db: &mut C,
        card: &mut CardBackupProtocol,
        now: Instant,
    ) -> Vec<CoordinatorSend>
    where
        FrostCoordinator: Persist<C> + TakeStaged<<FrostCoordinator as Persist<C>>::Update>,
    {
        let device_id = card.device_id();
        card.track_device_work(coordinator.card_device_work(device_id), now);
        if !card.device_overdue(now) {
            return vec![];
        }
        if self.opening_parts.contains(&device_id) && !card.opening_grace {
            card.opening_grace = true;
            card.work_due = Some(now + OPENING_GRACE);
            return vec![];
        }
        card.work_due = None;
        let mut sends = self.persist(coordinator, db, device_id, |coordinator| {
            coordinator.card_device_timeout(device_id)
        });
        card.action_sent(&mut sends);
        sends
    }

    /// Owner B9: the device was unplugged. Core persists an interruption of a running check
    /// before its `Interrupted` goes out, as for any card handler; what this returns goes in the
    /// outbox.
    pub fn disconnected<C>(
        &mut self,
        coordinator: &mut Persisted<FrostCoordinator>,
        db: &mut C,
        device_id: DeviceId,
    ) -> Vec<CoordinatorSend>
    where
        FrostCoordinator: Persist<C> + TakeStaged<<FrostCoordinator as Persist<C>>::Update>,
    {
        self.persist(coordinator, db, device_id, |coordinator| {
            Ok::<_, frostsnap_core::coordinator::card::CardActionError>(
                coordinator.card_device_disconnected(device_id),
            )
        })
    }

    /// Runs a card handler through [`persist_card_sends`]. What it returns goes in the outbox:
    /// the handler's sends once what it staged is persisted, and after a failed persist only the
    /// `StorageError` event.
    fn persist<C, E: core::fmt::Display>(
        &mut self,
        coordinator: &mut Persisted<FrostCoordinator>,
        db: &mut C,
        from: DeviceId,
        handler: impl FnOnce(&mut FrostCoordinator) -> Result<Vec<CoordinatorSend>, E>,
    ) -> Vec<CoordinatorSend>
    where
        FrostCoordinator: Persist<C> + TakeStaged<<FrostCoordinator as Persist<C>>::Update>,
    {
        let mut changed = BTreeSet::new();
        match persist_card_sends(coordinator, db, |coordinator| {
            let result = handler(coordinator);
            changed.extend(
                coordinator
                    .staged_mutations()
                    .iter()
                    .filter_map(|mutation| match mutation {
                        Mutation::Card(card) => card.tied_to_key(),
                        _ => None,
                    }),
            );
            result
        }) {
            Ok(sends) => {
                self.changed_keys.extend(changed);
                sends
            }
            Err(CardPersistError::Handler(e)) => {
                event!(
                    Level::ERROR,
                    from = from.to_string(),
                    "Card handler refused: {}",
                    e
                );
                vec![]
            }
            Err(CardPersistError::Storage {
                error,
                storage_error,
            }) => {
                event!(
                    Level::ERROR,
                    error = error.to_string(),
                    "failed to persist card changes; card flows are stopped until restart"
                );
                match storage_error {
                    Some(message) => vec![CoordinatorSend::ToUser(*message)],
                    None => {
                        self.storage_failed.insert(from);
                        vec![]
                    }
                }
            }
        }
    }

    /// The keys whose card status may have changed, so their backup run is shown again: a
    /// verdict can come after the card page was left.
    pub fn changed_keys(&self) -> impl Iterator<Item = KeyId> + '_ {
        self.changed_keys.iter().copied()
    }

    /// Run once the outbox is drained, so a `Busy` event's re-send is already set.
    pub fn apply(&self, card: &mut CardBackupProtocol) {
        let device_id = card.device_id();
        if self.last_busy.get(&device_id) == Some(&false) {
            card.device_replied();
        }
        if self.storage_failed.contains(&device_id) {
            card.storage_error();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use frostsnap_core::{
        coordinator::{
            card::{
                card_setup, BackupFormat, BoxAnswer, CardActionError, CardBackupStatus,
                CardMutation,
            },
            BeginKeygen, CoordinatorToUserKeyGenMessage,
        },
        device::{
            card::{CardScreen, CardState, CardUserAction, ToUserCard},
            card_session::{BusyTask, CardSession},
            DeviceSecretDerivation, DeviceToUserMessage, FrostSigner, KeyPurpose,
        },
        message::{
            card::{CoordinatorCard, RestoreResult},
            CoordinatorToDeviceMessage, DeviceSend,
        },
        nonce_stream::NonceStreamId,
        schnorr_fun::{
            frost::{Fingerprint, ShareIndex},
            fun::{marker::*, Scalar},
        },
        AccessStructureId, CoordShareDecryptionContrib, RestorationId,
    };
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeSink(Arc<Mutex<Vec<CardFlowState>>>);

    impl Sink<CardFlowState> for FakeSink {
        fn send(&self, state: CardFlowState) {
            self.0.lock().unwrap().push(state);
        }
    }

    impl FakeSink {
        fn last(&self) -> CardFlowState {
            self.0.lock().unwrap().last().cloned().expect("a state")
        }

        fn len(&self) -> usize {
            self.0.lock().unwrap().len()
        }
    }

    const DEVICE: DeviceId = DeviceId([2; 33]);
    const ASREF: AccessStructureRef = AccessStructureRef {
        key_id: KeyId([1; 32]),
        access_structure_id: AccessStructureId([3; 32]),
    };

    fn core(message: CoordinatorCard) -> CoordinatorSendMessage {
        CoordinatorSendMessage::to(
            DEVICE,
            CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Card(message)),
        )
    }

    fn protocol() -> (CardBackupProtocol, FakeSink) {
        let sink = FakeSink::default();
        let mut protocol = CardBackupProtocol::with_begin(
            DEVICE,
            ASREF,
            CoordinatorSend::ToDevice {
                message: CoordinatorToDeviceMessage::Card(CoordinatorCard::Stop),
                destinations: [DEVICE].into(),
            },
            sink.clone(),
        );
        protocol.connected(DEVICE, DeviceMode::Ready);
        (protocol, sink)
    }

    fn share_index() -> ShareIndex {
        Scalar::<Secret, NonZero>::one().public()
    }

    fn event(event: CardEvent) -> CoordinatorToUserMessage {
        CoordinatorToUserMessage::Card(CardToUser {
            device_id: DEVICE,
            access_structure_ref: ASREF,
            share_index: share_index(),
            event,
        })
    }

    fn kinds(messages: &[CoordinatorSendMessage]) -> Vec<String> {
        messages
            .iter()
            .map(|message| format!("{:?}", message.message_body))
            .collect()
    }

    fn page(page: u8) -> CardEvent {
        let first = 33 + 3 * page;
        CardEvent::PhonePage {
            page,
            first_word: first,
            words: if page < PHONE_PAGES {
                vec!["ABANDON", "ABILITY", "ABLE"]
            } else {
                vec!["ABANDON"; 32]
            },
        }
    }

    /// Review B15 protocol F3: a 25-word wallet made on this phone before cards has no
    /// `KeyFormat`, so the checklist offers [Backup], and the device refuses its share with
    /// `NotKeygenOrigin` (no origin recorded before cards) or `NotCardKey`. Neither message
    /// says only the creating device can make the card, which would blame the device that did.
    #[test]
    fn a_share_without_a_card_is_refused_without_blaming_the_device() {
        let restored = refusal_message(CardRefusal::NotKeygenOrigin);
        let words25 = refusal_message(CardRefusal::NotCardKey);
        assert_ne!(restored, words25);
        for message in [restored, words25] {
            assert!(!message.contains("Only the device"), "{message}");
            assert!(message.contains("no card to write"), "{message}");
        }
        assert!(restored.contains("before backup cards"));
        assert!(words25.contains("25-word"));
    }

    #[test]
    fn begin_is_sent_on_connect_and_after_needs_begin() {
        let (mut protocol, _sink) = protocol();
        let now = Instant::now();
        assert_eq!(protocol.poll_at(now).len(), 1, "begin on connect");
        assert!(protocol.poll_at(now).is_empty(), "begin only once");

        assert!(protocol.process_to_user_message(event(CardEvent::NeedsBegin)));
        assert_eq!(protocol.poll_at(now).len(), 1, "begin after NeedsBegin");

        // The same device announced again, as after a consolidation, isn't a new connection.
        protocol.connected(DEVICE, DeviceMode::Ready);
        assert!(protocol.poll_at(now).is_empty(), "begin only once");
    }

    /// An unplug ends the flow, paused, and the device is left at home: plugging it in again
    /// sends nothing, and an event the device sent before the unplug changes nothing.
    #[test]
    fn a_disconnect_ends_the_flow_and_a_reconnect_sends_no_begin() {
        let (mut protocol, sink) = protocol();
        let now = Instant::now();
        assert_eq!(protocol.poll_at(now).len(), 1, "begin on connect");
        protocol.on_event(CardEvent::DeviceHalf, now);
        protocol.on_event(
            CardEvent::Busy {
                resend: Box::new(CoordinatorToDeviceMessage::Card(
                    CoordinatorCard::PullProofs { from: 0, count: 16 },
                )),
            },
            now,
        );

        protocol.disconnected(DEVICE);
        assert!(matches!(sink.last().stage, CardFlowStage::Paused));
        assert!(!sink.last().device_connected);
        assert!(matches!(
            protocol.is_complete(),
            Some(Completion::Abort {
                send_cancel_to_all_devices: false
            })
        ));

        protocol.on_event(CardEvent::PreparingBox { k: 1 }, now);
        assert!(protocol.process_to_user_message(event(CardEvent::NeedsBegin)));
        protocol.connected(DEVICE, DeviceMode::Ready);
        protocol.storage_error();
        assert!(protocol.poll_at(now + BUSY_RETRY).is_empty());
        assert!(matches!(sink.last().stage, CardFlowStage::Paused));
    }

    /// Design §3.7: a page of words already in the phone's RAM keeps working while the device
    /// is unplugged; nothing goes to the device, and plugging it in again sends a fresh
    /// `Begin` (it booted fresh). Any other page pauses.
    #[test]
    fn a_page_of_words_in_ram_survives_an_unplug() {
        let now = Instant::now();
        for (stage, survives) in [
            (page(3), true),
            (page(PHONE_PAGES), true),
            (
                CardEvent::SelfCheckQuestion {
                    k: 2,
                    total: 32,
                    word_number: 35,
                    word: "ABLE",
                    lookalikes: ["TABLE", "CABLE"],
                    wrong: false,
                    recheck: false,
                },
                true,
            ),
            (CardEvent::DeviceHalf, false),
            (CardEvent::ConfirmDeviceHalf, false),
            (
                CardEvent::Proving {
                    verified: 16,
                    total: COMMIT_COUNT,
                },
                false,
            ),
        ] {
            let (mut protocol, sink) = protocol();
            assert_eq!(protocol.poll_at(now).len(), 1, "begin on connect");
            protocol.on_event(stage.clone(), now);
            protocol.disconnected(DEVICE);
            assert!(!sink.last().device_connected);
            if !survives {
                assert!(
                    matches!(sink.last().stage, CardFlowStage::Paused),
                    "{stage:?}"
                );
                continue;
            }
            assert!(protocol.is_complete().is_none(), "{stage:?}");
            assert!(!matches!(sink.last().stage, CardFlowStage::Paused));
            // An action's request isn't sent while unplugged.
            let mut sends = vec![CoordinatorSend::ToDevice {
                message: CoordinatorToDeviceMessage::Card(CoordinatorCard::Stop),
                destinations: [DEVICE].into(),
            }];
            protocol.action_sent(&mut sends);
            assert!(sends.is_empty());
            protocol.on_event(page(4), now);
            assert!(matches!(
                sink.last().stage,
                CardFlowStage::PhoneWords { page: 4, .. }
            ));
            assert!(protocol.poll_at(now).is_empty());
            protocol.connected(DEVICE, DeviceMode::Ready);
            assert_eq!(protocol.poll_at(now).len(), 1, "a fresh begin");
        }
    }

    /// An unplug, or another protocol's cancel, under an end page leaves it standing: a
    /// storage error's "restart the app" isn't swapped for a Continue that can't work.
    #[test]
    fn an_unplug_leaves_an_end_page() {
        type End = fn(&mut CardBackupProtocol);
        for (end, is_it) in [
            (
                (|p: &mut CardBackupProtocol| p.storage_error()) as End,
                (|s: &CardFlowStage| matches!(s, CardFlowStage::StorageError)) as fn(&_) -> bool,
            ),
            (
                |p| {
                    p.on_event(
                        CardEvent::Refused {
                            reason: CardRefusal::DeviceError,
                        },
                        Instant::now(),
                    )
                },
                |s| matches!(s, CardFlowStage::Refused { .. }),
            ),
            (
                |p| p.on_event(CardEvent::StartedOnAnotherPhone, Instant::now()),
                |s| matches!(s, CardFlowStage::StartedOnAnotherPhone),
            ),
        ] {
            for unplug in [true, false] {
                let (mut protocol, sink) = protocol();
                protocol.on_event(CardEvent::DeviceHalf, Instant::now());
                end(&mut protocol);
                if unplug {
                    protocol.disconnected(DEVICE);
                } else {
                    protocol.devices_cancelled();
                }
                let state = sink.last();
                assert!(is_it(&state.stage), "{:?}", state.stage);
                assert_eq!(state.device_connected, !unplug);
                assert!(matches!(
                    protocol.is_complete(),
                    Some(Completion::Abort {
                        send_cancel_to_all_devices: false
                    })
                ));
            }
        }
    }

    /// The loop handles an unplug before the device messages that came ahead of it, so a
    /// verdict can reach a paused flow (before N): it still shows. Nothing else does. Owner B9:
    /// after N the flow isn't paused by an unplug at all, and the verdict shows the same way.
    #[test]
    fn a_verdict_sent_before_the_unplug_still_shows() {
        let (mut paused, paused_sink) = protocol();
        let now = Instant::now();
        assert_eq!(paused.poll_at(now).len(), 1, "begin on connect");
        paused.on_event(CardEvent::ConfirmDeviceHalf, now);
        paused.disconnected(DEVICE);
        assert!(matches!(paused_sink.last().stage, CardFlowStage::Paused));
        paused.on_event(
            CardEvent::Failed {
                reason: CardFailReason::Stopped,
                device_suspect: false,
                after_n: false,
            },
            now,
        );
        assert!(matches!(
            paused_sink.last().stage,
            CardFlowStage::Failed { .. }
        ));

        let (mut protocol, sink) = protocol();
        assert_eq!(protocol.poll_at(now).len(), 1, "begin on connect");
        protocol.on_event(CardEvent::RoundSent { answered: 2 }, now);
        protocol.disconnected(DEVICE);
        assert!(matches!(sink.last().stage, CardFlowStage::RoundSent));
        assert!(protocol.is_complete().is_none());

        protocol.on_event(CardEvent::Passed, now);
        let state = sink.last();
        assert!(matches!(state.stage, CardFlowStage::Passed));
        assert!(state.record_changed);

        protocol.on_event(CardEvent::NeedsBegin, now);
        protocol.on_event(
            CardEvent::Failed {
                reason: CardFailReason::TwoMisses,
                device_suspect: true,
                after_n: true,
            },
            now,
        );
        assert!(matches!(sink.last().stage, CardFlowStage::Passed));
        assert!(protocol.poll_at(now + BUSY_RETRY).is_empty());
    }

    #[test]
    fn busy_resends_after_a_second() {
        let (mut protocol, _sink) = protocol();
        let now = Instant::now();
        protocol.poll_at(now);
        let open = CoordinatorCard::PullPhoneHalf { h_c: [0; 32] };
        protocol.on_event(
            CardEvent::Busy {
                resend: Box::new(CoordinatorToDeviceMessage::Card(open.clone())),
            },
            now,
        );
        assert!(protocol.poll_at(now).is_empty());
        assert!(protocol
            .poll_at(now + Duration::from_millis(999))
            .is_empty());
        let sent = protocol.poll_at(now + BUSY_RETRY);
        assert_eq!(kinds(&sent), kinds(&[core(open)]));
        assert!(
            protocol.poll_at(now + 2 * BUSY_RETRY).is_empty(),
            "sent once"
        );
    }

    #[test]
    fn a_reply_clears_the_pending_resend() {
        let (mut protocol, _sink) = protocol();
        let now = Instant::now();
        protocol.poll_at(now);
        let busy = || CardEvent::Busy {
            resend: Box::new(CoordinatorToDeviceMessage::Card(
                CoordinatorCard::PullProofs { from: 0, count: 16 },
            )),
        };

        protocol.on_event(busy(), now);
        protocol.device_replied();
        assert!(protocol.poll_at(now + BUSY_RETRY).is_empty());

        protocol.on_event(busy(), now);
        protocol.action_sent(&mut vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Card(CoordinatorCard::Stop),
            destinations: [DEVICE].into(),
        }]);
        assert!(
            protocol.poll_at(now + BUSY_RETRY).is_empty(),
            "a newer request replaces it"
        );

        protocol.on_event(busy(), now);
        protocol.disconnected(DEVICE);
        assert!(protocol.poll_at(now + BUSY_RETRY).is_empty());
    }

    /// A page of words, which sends the device nothing, and an event leave the `PullProofs`
    /// re-send pending.
    #[test]
    fn an_action_that_sends_nothing_keeps_the_pending_resend() {
        let (mut protocol, _sink) = protocol();
        let now = Instant::now();
        protocol.poll_at(now);
        let pull = CoordinatorCard::PullProofs { from: 0, count: 16 };
        protocol.on_event(
            CardEvent::Busy {
                resend: Box::new(CoordinatorToDeviceMessage::Card(pull.clone())),
            },
            now,
        );
        protocol.action_sent(&mut vec![CoordinatorSend::ToUser(event(page(1)))]);
        protocol.on_event(
            CardEvent::Proving {
                verified: 64,
                total: COMMIT_COUNT,
            },
            now,
        );
        assert_eq!(
            kinds(&protocol.poll_at(now + BUSY_RETRY)),
            kinds(&[core(pull)])
        );
    }

    /// A card action's request goes out behind the flow's `Begin` if the loop hasn't sent that
    /// yet. An action that sends the device nothing leaves the `Begin` to the loop. An ended
    /// flow sends no `Begin`, and a flow held at the firmware gate sends the device nothing.
    #[test]
    fn an_action_goes_out_behind_a_pending_begin() {
        let now = Instant::now();
        let stop = || CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Card(CoordinatorCard::PhoneRound {
                n: [7; 32],
                pass: false,
            }),
            destinations: [DEVICE].into(),
        };
        let sends_kinds = |sends: &[CoordinatorSend]| -> Vec<String> {
            sends.iter().map(|send| format!("{send:?}")).collect()
        };
        let sent = |flow: &mut CardBackupProtocol, mut sends: Vec<CoordinatorSend>| {
            flow.action_sent(&mut sends);
            sends_kinds(&sends)
        };

        let (mut pending, _sink) = protocol();
        let shown = CoordinatorSend::ToUser(event(page(0)));
        assert_eq!(
            sent(&mut pending, vec![shown.clone()]),
            sends_kinds(&[shown]),
            "no request: the Begin is the loop's"
        );
        let begin = pending.begin.clone();
        assert_eq!(
            sent(&mut pending, vec![stop()]),
            sends_kinds(&[begin, stop()])
        );
        assert!(pending.poll_at(now).is_empty(), "the Begin goes once");

        let (mut ended, _sink) = protocol();
        ended.on_event(
            CardEvent::Failed {
                reason: CardFailReason::Stopped,
                device_suspect: true,
                after_n: true,
            },
            now,
        );
        assert!(ended.send_begin);
        assert_eq!(sent(&mut ended, vec![stop()]), sends_kinds(&[stop()]));
        assert!(ended.poll_at(now).is_empty());

        let mut held =
            CardBackupProtocol::with_begin(DEVICE, ASREF, ended.begin.clone(), FakeSink::default());
        held.firmware_refused(DEVICE);
        held.connected(DEVICE, DeviceMode::Ready);
        let shown = CoordinatorSend::ToUser(event(page(0)));
        assert_eq!(
            sent(&mut held, vec![stop(), shown.clone()]),
            sends_kinds(&[shown])
        );
        assert!(held.poll_at(now).is_empty());
    }

    #[test]
    fn cancel_doesnt_broadcast() {
        let (mut protocol, _sink) = protocol();
        protocol.cancel();
        assert!(matches!(
            protocol.is_complete(),
            Some(Completion::Abort {
                send_cancel_to_all_devices: false
            })
        ));
    }

    #[test]
    fn terminal_stages_complete() {
        for (terminal, stage_ok) in [
            (
                CardEvent::Passed,
                (|s: &CardFlowStage| matches!(s, CardFlowStage::Passed)) as fn(&_) -> bool,
            ),
            (CardEvent::DifferentCard, |s| {
                matches!(s, CardFlowStage::DifferentCard)
            }),
            (
                CardEvent::Failed {
                    reason: CardFailReason::SealChanged,
                    device_suspect: true,
                    after_n: false,
                },
                |s| {
                    matches!(
                        s,
                        CardFlowStage::Failed {
                            alarm: CardAlarm {
                                kind: CardAlarmKind::SealChanged,
                                device_suspect: true,
                                next_step: NextStep::WithoutThisDevice,
                            }
                        }
                    )
                },
            ),
            (
                CardEvent::Failed {
                    reason: CardFailReason::TwoMisses,
                    device_suspect: true,
                    after_n: true,
                },
                |s| {
                    matches!(
                        s,
                        CardFlowStage::Failed {
                            alarm: CardAlarm {
                                kind: CardAlarmKind::PhoneRoundFailed,
                                device_suspect: true,
                                next_step: NextStep::WithoutThisDevice,
                            }
                        }
                    )
                },
            ),
        ] {
            let (mut protocol, sink) = protocol();
            protocol.on_event(CardEvent::DeviceHalf, Instant::now());
            assert!(protocol.is_complete().is_none());
            protocol.on_event(terminal, Instant::now());
            assert!(matches!(protocol.is_complete(), Some(Completion::Success)));
            let state = sink.last();
            assert!(stage_ok(&state.stage), "{:?}", state.stage);
            assert!(state.record_changed);
        }
    }

    /// Every `CardEvent` that isn't an end, in flow order, onto its stage.
    #[test]
    fn events_map_to_stages() {
        let (mut protocol, sink) = protocol();
        let other_device = CoordinatorToUserMessage::Card(CardToUser {
            device_id: DeviceId([9; 33]),
            access_structure_ref: ASREF,
            share_index: share_index(),
            event: CardEvent::Passed,
        });
        assert!(!protocol.process_to_user_message(other_device));
        assert!(matches!(sink.last().stage, CardFlowStage::Starting));

        let now = Instant::now();
        protocol.on_event(CardEvent::PreparingCard, now);
        assert!(matches!(sink.last().stage, CardFlowStage::PreparingCard));
        protocol.on_event(CardEvent::Pinning { points: 120 }, now);
        assert_eq!(sink.last().points_received, 120);
        assert!(matches!(sink.last().stage, CardFlowStage::Pinning));

        protocol.on_event(CardEvent::DeviceHalf, now);
        assert!(matches!(sink.last().stage, CardFlowStage::DeviceHalf));
        assert!(sink.last().record_changed, "the pin is recorded");

        // Owner B1: the device's quiz passed, then the confirm on the phone, then the proofs
        // behind the pages of words 33–64.
        protocol.on_event(CardEvent::CardFinished, now);
        assert!(sink.last().record_changed, "Finished");
        assert!(matches!(sink.last().stage, CardFlowStage::DeviceHalf));
        protocol.on_event(CardEvent::ConfirmDeviceHalf, now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::ConfirmDeviceHalf
        ));
        protocol.on_event(
            CardEvent::Proving {
                verified: 0,
                total: COMMIT_COUNT,
            },
            now,
        );
        // The confirm stays up until the first page, so "Checking your card" doesn't flash.
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::ConfirmDeviceHalf
        ));
        protocol.on_event(page(0), now);
        let CardFlowStage::PhoneWords {
            page: 0,
            first_word: 33,
            words,
        } = sink.last().stage
        else {
            panic!("page 0: {:?}", sink.last().stage);
        };
        assert_eq!(words, ["ABANDON", "ABILITY", "ABLE"]);
        // A page stays up while the proofs come in.
        protocol.on_event(
            CardEvent::Proving {
                verified: 176,
                total: COMMIT_COUNT,
            },
            now,
        );
        assert_eq!(sink.last().proofs_verified, 176);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::PhoneWords { page: 0, .. }
        ));
        protocol.on_event(page(PHONE_PAGES), now);
        assert!(
            matches!(sink.last().stage, CardFlowStage::PhoneAllWords { ref words } if words.len() == 32)
        );

        protocol.on_event(
            CardEvent::SelfCheckQuestion {
                k: 2,
                total: 32,
                word_number: 35,
                word: "ABLE",
                lookalikes: ["TABLE", "CABLE"],
                wrong: true,
                recheck: false,
            },
            now,
        );
        let CardFlowStage::SelfCheck {
            k: 2,
            total: 32,
            word_number: 35,
            mut options,
            wrong: true,
            recheck: false,
        } = sink.last().stage
        else {
            panic!("the quiz: {:?}", sink.last().stage);
        };
        options.sort();
        assert_eq!(options, ["ABLE", "CABLE", "TABLE"]);
        assert!(!sink.last().halves_recorded);
        protocol.on_event(CardEvent::PhoneSelfChecked, now);
        assert!(sink.last().halves_recorded, "both halves recorded");
        assert!(sink.last().record_changed);
        // The last proofs, after the phone's quiz: "Checking your card", then [Continue].
        protocol.on_event(
            CardEvent::Proving {
                verified: 300,
                total: COMMIT_COUNT,
            },
            now,
        );
        assert!(matches!(sink.last().stage, CardFlowStage::Proving));
        protocol.on_event(CardEvent::Proven, now);
        assert_eq!(sink.last().proofs_verified, COMMIT_COUNT);
        assert!(matches!(sink.last().stage, CardFlowStage::Proving));
        protocol.on_event(CardEvent::ReadyForFinalCheck, now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::ReadyForFinalCheck
        ));
        protocol.on_event(CardEvent::DeviceRound, now);
        assert!(matches!(sink.last().stage, CardFlowStage::DeviceRound));

        protocol.on_event(CardEvent::PreparingBox { k: 2 }, now);
        assert_eq!(sink.last().boxes_done, 2);
        let emitted = sink.len();
        protocol.on_event(CardEvent::GateShown, now);
        assert_eq!(sink.len(), emitted, "no new screen");
        let question = |second_look| CardEvent::Question {
            k: 4,
            word_number: 7,
            word: "SIZE",
            pool: ["SIDE", "SIZZLE", "SITE", "SIX", "PRIZE", "SEIZE"],
            second_look,
        };
        protocol.on_event(question(false), now);
        let CardFlowStage::Question {
            k: 4,
            word_number: 7,
            options: first,
            second_look: false,
        } = sink.last().stage
        else {
            panic!("{:?}", sink.last().stage);
        };
        assert_eq!(first.len(), 3);
        assert!(first.contains(&"SIZE".to_string()));
        protocol.on_event(CardEvent::TryAgain { k: 4 }, now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::TryAgain { k: 4 }
        ));
        // O11: the second look shows the same two lookalikes.
        protocol.on_event(question(true), now);
        let CardFlowStage::Question {
            options: again,
            second_look: true,
            ..
        } = sink.last().stage
        else {
            panic!("{:?}", sink.last().stage);
        };
        let sorted = |mut words: Vec<String>| {
            words.sort();
            words
        };
        assert_eq!(sorted(first), sorted(again));
        protocol.on_event(CardEvent::RoundSent { answered: 5 }, now);
        assert_eq!(sink.last().boxes_done, 5);
        assert!(matches!(sink.last().stage, CardFlowStage::RoundSent));

        protocol.on_event(
            CardEvent::Refused {
                reason: CardRefusal::NotDirectUsb,
            },
            now,
        );
        assert!(matches!(sink.last().stage, CardFlowStage::Refused { .. }));
        protocol.on_event(CardEvent::AlreadyFinished, now);
        match sink.last().stage {
            CardFlowStage::Refused { message } => assert_eq!(message, "This card is finished."),
            stage => panic!("{stage:?}"),
        }
        protocol.on_event(CardEvent::StartedOnAnotherPhone, now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::StartedOnAnotherPhone
        ));
        protocol.on_event(CardEvent::StorageError, now);
        assert!(matches!(sink.last().stage, CardFlowStage::StorageError));
        assert!(protocol.is_complete().is_none());
    }

    /// O11: across questions the protocol picks every pair of the six lookalikes.
    /// App review n1: a flow resumed in the phone half (Paused, then Continue, or an app
    /// restart) goes from `Starting` straight to its page; core's `Proving` before the page
    /// doesn't show "Checking your card" in between. With both halves recorded the proofs are
    /// what's left, so `Proving` shows.
    #[test]
    fn a_resumed_phone_half_doesnt_flash_proving() {
        let now = Instant::now();
        let (mut resumed, sink) = protocol();
        assert!(matches!(sink.last().stage, CardFlowStage::Starting));
        let shown = sink.len();
        resumed.on_event(
            CardEvent::Proving {
                verified: 40,
                total: COMMIT_COUNT,
            },
            now,
        );
        assert_eq!(sink.last().proofs_verified, 40);
        assert!(matches!(sink.last().stage, CardFlowStage::Starting));
        assert!(sink.len() > shown, "the count still goes out");
        resumed.on_event(page(2), now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::PhoneWords { page: 2, .. }
        ));

        let (mut recorded, sink) = protocol();
        recorded.state.halves_recorded = true;
        recorded.on_event(
            CardEvent::Proving {
                verified: 40,
                total: COMMIT_COUNT,
            },
            now,
        );
        assert!(matches!(sink.last().stage, CardFlowStage::Proving));
    }

    #[test]
    fn the_phone_round_picks_two_of_six() {
        let mut pairs = BTreeSet::new();
        for k in 0..200u8 {
            let (mut protocol, sink) = protocol();
            protocol.on_event(
                CardEvent::Question {
                    k,
                    word_number: 7,
                    word: "SIZE",
                    pool: ["SIDE", "SIZZLE", "SITE", "SIX", "PRIZE", "SEIZE"],
                    second_look: false,
                },
                Instant::now(),
            );
            let CardFlowStage::Question { mut options, .. } = sink.last().stage else {
                panic!("a question");
            };
            options.retain(|option| option != "SIZE");
            options.sort();
            pairs.insert(options);
        }
        assert_eq!(pairs.len(), 15);
    }

    /// A later check ends once both screens' quizzes have: the phone's (or its absence) and the
    /// device's, in either order.
    #[test]
    fn a_recheck_completes_with_both_quizzes() {
        let now = Instant::now();
        for (phone, device_first) in [
            (CardEvent::RecheckPassed, false),
            (CardEvent::RecheckPassed, true),
            (CardEvent::RecheckUnavailable, false),
            (CardEvent::RecheckUnavailable, true),
        ] {
            let (mut protocol, sink) = protocol();
            protocol.on_event(CardEvent::RecheckWaiting, now);
            assert!(matches!(sink.last().stage, CardFlowStage::RecheckWaiting));
            if device_first {
                protocol.on_event(CardEvent::RecheckDeviceDone, now);
                assert!(protocol.is_complete().is_none());
            }
            protocol.on_event(phone.clone(), now);
            if !device_first {
                assert!(protocol.is_complete().is_none());
                protocol.on_event(CardEvent::RecheckDeviceDone, now);
            }
            assert!(matches!(protocol.is_complete(), Some(Completion::Success)));
            assert!(sink.last().recheck_device_done);
        }
    }

    /// Leaving a later check once the phone's quiz is done cancels nothing on the device, whose
    /// quiz runs to its own Exit; leaving before then does.
    #[test]
    fn leaving_a_recheck_after_the_phone_quiz_leaves_the_device_quiz() {
        let now = Instant::now();
        for (phone, live) in [
            (None, true),
            (Some(CardEvent::RecheckPassed), false),
            (Some(CardEvent::RecheckUnavailable), false),
        ] {
            let (mut protocol, _sink) = protocol();
            let device_id = protocol.device_id;
            protocol.on_event(CardEvent::RecheckWaiting, now);
            if let Some(phone) = phone {
                protocol.on_event(phone, now);
            }
            assert!(
                protocol.is_complete().is_none(),
                "the device's quiz runs on"
            );
            let mut stack = UiStack::default();
            stack.push(protocol);
            assert_eq!(leave_card_flow(&mut stack, device_id), live);
            assert!(stack.get_mut::<CardBackupProtocol>().is_none());
        }
    }

    /// The phone's Next and Back over the pages of words 33–64: page 0 has no Back, page 11 (all
    /// the words) leads to the confirm page, and Back from it to page 11.
    #[test]
    fn the_phone_pages() {
        let (mut protocol, sink) = protocol();
        let now = Instant::now();
        protocol.on_event(page(0), now);
        assert_eq!(protocol.phone_back(), None);
        assert_eq!(protocol.phone_next(), Some(1));
        protocol.on_event(page(5), now);
        assert_eq!(protocol.phone_next(), Some(6));
        assert_eq!(protocol.phone_back(), Some(4));
        protocol.on_event(page(PHONE_PAGES), now);
        assert_eq!(protocol.phone_back(), Some(PHONE_PAGES - 1));
        assert_eq!(protocol.phone_next(), None);
        assert!(matches!(sink.last().stage, CardFlowStage::PhoneConfirm));
        assert_eq!(protocol.phone_back(), Some(PHONE_PAGES));
    }

    /// A stage's `Debug` names it, never a word.
    #[test]
    fn a_stage_debug_shows_no_word() {
        let (mut protocol, sink) = protocol();
        let now = Instant::now();
        for event in [
            page(0),
            page(PHONE_PAGES),
            CardEvent::SelfCheckQuestion {
                k: 2,
                total: 32,
                word_number: 35,
                word: "ABLE",
                lookalikes: ["TABLE", "CABLE"],
                wrong: false,
                recheck: false,
            },
            CardEvent::Question {
                k: 4,
                word_number: 7,
                word: "SIZE",
                pool: ["SIDE", "SIZZLE", "SITE", "SIX", "PRIZE", "SEIZE"],
                second_look: false,
            },
        ] {
            protocol.on_event(event.clone(), now);
            for debug in [format!("{:?}", sink.last()), format!("{event:?}")] {
                for leak in ["ABANDON", "ABLE", "SIZE", "SIDE"] {
                    assert!(!debug.contains(leak), "{debug}");
                }
            }
        }
    }

    /// The device lost its session while plugged in: the flow sends `Begin` again, and the
    /// stage stays.
    #[test]
    fn needs_begin_sends_begin_again() {
        let (mut protocol, sink) = protocol();
        let now = Instant::now();
        assert_eq!(protocol.poll_at(now).len(), 1, "begin on connect");
        protocol.on_event(CardEvent::DeviceHalf, now);
        let emitted = sink.len();
        protocol.on_event(CardEvent::NeedsBegin, now);
        assert_eq!(sink.len(), emitted, "no new screen");
        assert_eq!(protocol.poll_at(now).len(), 1, "begin again");
        assert!(protocol.is_complete().is_none());
    }

    /// The alarm and its next step for every way a card fails, before and after N.
    #[test]
    fn failures_before_and_after_n() {
        use CardFailReason::*;
        let alarm = |reason, suspect, after_n| match failed_stage(reason, suspect, after_n) {
            CardFlowStage::Failed { alarm } => {
                assert_eq!(alarm.device_suspect, suspect);
                assert_eq!(alarm.next_step, alarm.kind.next_step());
                (alarm.kind, alarm.next_step)
            }
            stage => panic!("{reason:?}: {stage:?}"),
        };
        use CardAlarmKind as K;
        use NextStep as S;

        // Before N.
        assert!(matches!(
            failed_stage(DifferentCard, false, false),
            CardFlowStage::DifferentCard
        ));
        // Owner B10: a changed seal is a suspect device, on either side of N. Owner B9: so is
        // an interruption the user didn't cause, before the device round or after it.
        for after_n in [false, true] {
            assert_eq!(
                alarm(SealChanged, true, after_n),
                (K::SealChanged, S::WithoutThisDevice)
            );
            assert_eq!(
                alarm(Interrupted, true, after_n),
                (K::DisconnectedAfterN, S::WithoutThisDevice)
            );
            // Owner R2-2: a 4th "yes" past the cap.
            assert_eq!(
                alarm(TooManyInterruptions, true, after_n),
                (K::InterruptedTooOften, S::WithoutThisDevice)
            );
        }
        assert_eq!(
            alarm(Stopped, false, false),
            (K::StoppedBeforeN, S::SetUpAgain)
        );
        assert_eq!(
            alarm(DeviceRoundFailed, false, false),
            (K::DeviceRoundFailed, S::DifferentPhone)
        );
        // Words 33–64 that aren't the pinned ones, or a bit proof that failed twice once words
        // 1–32 are written, are a device fault: it leaves the device out (build plan C16).
        for reason in [BadOpening, WrongBox, WordRange, BadProof, PhoneHalfChanged] {
            assert_eq!(
                alarm(reason, true, false),
                (K::DeviceFault, S::WithoutThisDevice)
            );
        }

        // After N every end leaves the device out, and none sends the user to another phone.
        assert_eq!(
            alarm(Stopped, true, true),
            (K::StoppedAfterN, S::WithoutThisDevice)
        );
        for reason in [
            TwoMisses,
            BadOpening,
            WrongBox,
            WordRange,
            CoinMismatch,
            BoxesMismatch,
            DeviceVerdictMismatch,
            UnexpectedVerdict,
            PhoneHalfChanged,
        ] {
            assert_eq!(
                alarm(reason, true, true),
                (K::PhoneRoundFailed, S::WithoutThisDevice),
                "{reason:?}"
            );
        }
        // Owner B8: the chip knew D, so both are left out.
        assert_eq!(
            alarm(DeviceRoundFailed, true, true),
            (K::DeviceRoundFailedAfterN, S::DifferentPhoneAndDevice)
        );
        // A stall after the device round (the gate, an opening) says no words mismatched (app
        // review APP-3).
        for reason in [
            DifferentCard,
            BadProof,
            Refused(CardRefusal::RunCap),
            PhoneTimeout,
        ] {
            assert_eq!(
                alarm(reason, true, true),
                (K::EndedAfterN, S::WithoutThisDevice),
                "{reason:?}"
            );
        }
    }

    /// Owner B9: the question covers a page of words in RAM, and "yes" gives it back. While the
    /// question is up nothing goes to the device, not even the `Begin` a replug would send, and a
    /// device with other firmware never gets one, after "yes" either.
    #[test]
    fn the_question_covers_a_page_and_gives_it_back() {
        let now = Instant::now();
        let (mut protocol, sink) = protocol();
        assert_eq!(protocol.poll_at(now).len(), 1, "begin on connect");
        protocol.on_event(CardEvent::CheckStarted, now);
        assert!(sink.last().check_started);
        protocol.on_event(page(3), now);
        protocol.disconnected(DEVICE);
        assert!(protocol.is_complete().is_none());
        protocol.on_event(CardEvent::Interrupted { resumes_left: 3 }, now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::CheckInterrupted {
                disconnected: true,
                ..
            }
        ));
        protocol.connected(DEVICE, DeviceMode::Ready);
        assert!(
            protocol.poll_at(now).is_empty(),
            "no Begin before the answer"
        );
        protocol.on_event(CardEvent::Resuming, now);
        assert!(matches!(
            sink.last().stage,
            CardFlowStage::PhoneWords { page: 3, .. }
        ));
        assert_eq!(protocol.poll_at(now).len(), 1, "Begin after yes");

        // Other firmware: the question stays up, and "yes" leaves the flow on its own page.
        let (mut other, other_sink) = self::protocol();
        other.poll_at(now);
        other.on_event(CardEvent::CheckStarted, now);
        other.on_event(CardEvent::DeviceRound, now);
        other.disconnected(DEVICE);
        other.on_event(CardEvent::Interrupted { resumes_left: 3 }, now);
        other.firmware_refused(DEVICE);
        assert!(matches!(
            other_sink.last().stage,
            CardFlowStage::CheckInterrupted { .. }
        ));
        other.on_event(CardEvent::Resuming, now);
        assert!(matches!(
            other_sink.last().stage,
            CardFlowStage::WrongFirmware
        ));
        assert!(other.poll_at(now).is_empty());
    }

    /// The firmware gate takes only the app's bundled image.
    #[test]
    fn the_firmware_gate_takes_only_the_bundled_image() {
        use frostsnap_comms::Sha256Digest;
        let bundled = FirmwareVersion::new(Sha256Digest([1; 32]));
        assert!(check_card_firmware(&bundled, Some(&bundled)).is_ok());
        let older = FirmwareVersion::new(Sha256Digest([2; 32]));
        let refused = check_card_firmware(&older, Some(&bundled)).unwrap_err();
        assert_eq!(refused.to_string(), "Update this device's firmware first");
        // An app without a bundled image can't tell, so it refuses.
        assert!(check_card_firmware(&bundled, None).is_err());
    }

    fn persisted() -> (Persisted<FrostCoordinator>, rusqlite::Connection) {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let coordinator = Persisted::<FrostCoordinator>::new(&mut db, ()).unwrap();
        (coordinator, db)
    }

    /// Stages a mutation and returns a card send and a card event, as a card handler does.
    fn handler(
        coordinator: &mut FrostCoordinator,
    ) -> Result<Vec<CoordinatorSend>, CardActionError> {
        coordinator.start_restoring_key(
            "card".into(),
            Some(2),
            KeyPurpose::Test,
            RestorationId::new(&mut rand::thread_rng()),
        );
        Ok(vec![
            CoordinatorSend::ToDevice {
                message: CoordinatorToDeviceMessage::Card(CoordinatorCard::OpenStart {
                    n: [5; 32],
                }),
                destinations: [DEVICE].into(),
            },
            CoordinatorSend::ToUser(event(CardEvent::PreparingBox { k: 0 })),
        ])
    }

    #[test]
    fn sends_wait_for_the_persist() {
        let (mut coordinator, mut db) = persisted();
        let sends = persist_card_sends(&mut coordinator, &mut db, handler).unwrap();
        assert_eq!(sends.len(), 2);
        assert_eq!(
            mutation_rows(&db),
            1,
            "persisted before the sends were handed back"
        );
    }

    #[test]
    fn a_failed_persist_sends_nothing_and_poisons() {
        let (mut coordinator, mut db) = persisted();
        db.execute("DROP TABLE fs_coordinator_mutations", [])
            .unwrap();
        let result = persist_card_sends(&mut coordinator, &mut db, handler);
        let Err(CardPersistError::Storage { storage_error, .. }) = result else {
            panic!("the persist must fail: {result:?}");
        };
        assert!(matches!(
            storage_error.as_deref(),
            Some(CoordinatorToUserMessage::Card(CardToUser {
                event: CardEvent::StorageError,
                device_id: DEVICE,
                ..
            }))
        ));

        // Every card action refuses from now on, so N is never sent.
        let refused = coordinator
            .MUTATE_NO_PERSIST()
            .card_final_begin(DEVICE, &mut rand::thread_rng());
        assert_eq!(refused.unwrap_err(), CardActionError::Poisoned);
    }

    #[test]
    fn a_handler_refusal_is_passed_through() {
        let (mut coordinator, mut db) = persisted();
        let result = persist_card_sends(&mut coordinator, &mut db, |coordinator| {
            coordinator.card_phone_page(DEVICE, 1)
        });
        assert!(matches!(
            result,
            Err(CardPersistError::Handler(CardActionError::NoSession))
        ));
    }

    const KEY: SymmetricKey = SymmetricKey([42; 32]);

    /// A cheap fingerprint, so the keygen doesn't grind.
    const FINGERPRINT: Fingerprint = Fingerprint {
        bits_per_coeff: 2,
        max_bits_total: 6,
        tag: "test",
    };

    struct TestDerivation;

    impl DeviceSecretDerivation for TestDerivation {
        fn get_share_encryption_key(
            &mut self,
            _: AccessStructureRef,
            _: ShareIndex,
            _: CoordShareDecryptionContrib,
        ) -> SymmetricKey {
            KEY
        }

        fn derive_nonce_seed(&mut self, _: NonceStreamId, _: u32, seed: &[u8; 32]) -> [u8; 32] {
            *seed
        }
    }

    /// A persisted coordinator holding a 2-of-2 card key, made by a keygen with real devices,
    /// and those devices. The coordinator is the install that ran the keygen (`created_here`),
    /// or (owner B15) another install with the wallet, which has the key without the keygen's
    /// `KeyFormat`, as a restoration leaves it.
    fn card_key_held(
        created_here: bool,
    ) -> (
        Persisted<FrostCoordinator>,
        rusqlite::Connection,
        AccessStructureRef,
        BTreeMap<DeviceId, FrostSigner>,
    ) {
        let rng = &mut rand::thread_rng();
        let (mut persisted, mut db) = persisted();
        let coordinator = persisted.MUTATE_NO_PERSIST();
        coordinator.keygen_fingerprint = FINGERPRINT;
        let mut devices: BTreeMap<_, _> = (0..2)
            .map(|_| {
                let mut signer = FrostSigner::new_random_with_nonce_batch_size(rng, 8, 10);
                signer.keygen_fingerprint = FINGERPRINT;
                (signer.device_id(), signer)
            })
            .collect();
        let ids: Vec<_> = devices.keys().copied().collect();
        let begin = BeginKeygen::new(ids.clone(), 2, "card".into(), KeyPurpose::Test, rng)
            .with_backup_format(BackupFormat::Card64);
        let mut queue: VecDeque<_> = coordinator
            .begin_keygen(begin, rng)
            .unwrap()
            .into_iter()
            .collect();
        while let Some(send) = queue.pop_front() {
            match send {
                CoordinatorSend::ToDevice {
                    message,
                    destinations,
                } => {
                    for id in destinations {
                        let device = devices.get_mut(&id).unwrap();
                        let mut replies: VecDeque<_> = device
                            .recv_coordinator_message(message.clone(), rng)
                            .unwrap()
                            .into();
                        while let Some(reply) = replies.pop_front() {
                            match reply {
                                DeviceSend::ToCoordinator(message) => queue
                                    .extend(coordinator.recv_device_message(id, *message).unwrap()),
                                DeviceSend::ToUser(message) => {
                                    if let DeviceToUserMessage::CheckKeyGen { phase } = *message {
                                        replies.extend(
                                            device
                                                .keygen_ack(*phase, &mut TestDerivation, rng)
                                                .unwrap(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                CoordinatorSend::ToUser(CoordinatorToUserMessage::KeyGen {
                    keygen_id,
                    inner:
                        CoordinatorToUserKeyGenMessage::KeyGenAck {
                            all_acks_received: true,
                            ..
                        },
                }) => queue.extend(coordinator.finalize_keygen(keygen_id, KEY, rng).unwrap()),
                CoordinatorSend::ToUser(_) => {}
            }
        }
        let asref = coordinator
            .iter_access_structures()
            .next()
            .expect("the keygen finished")
            .access_structure_ref();
        assert_eq!(
            coordinator.key_backup_format(asref.key_id),
            BackupFormat::Card64
        );
        if !created_here {
            let mut other = FrostCoordinator::new();
            other.keygen_fingerprint = FINGERPRINT;
            for mutation in coordinator.take_staged_mutations() {
                if !matches!(
                    mutation,
                    frostsnap_core::coordinator::Mutation::Card(CardMutation::KeyFormat { .. })
                ) {
                    other.mutate(mutation);
                }
            }
            *coordinator = other;
        }
        assert_eq!(coordinator.card_created_here(asref.key_id), created_here);
        persisted.staged_mutate(&mut db, |_| Ok(())).unwrap();
        (persisted, db, asref, devices)
    }

    /// A device in a card flow, as the firmware runs one: core's handlers with its session.
    struct TestDevice {
        signer: FrostSigner,
        session: Option<CardSession<'static>>,
        screens: Vec<CardScreen>,
        /// A stalling device: it ignores every `OpenBox`, so no opening of D ever comes.
        withhold_openings: bool,
    }

    impl TestDevice {
        fn new(signer: FrostSigner) -> Self {
            Self {
                signer,
                session: None,
                screens: vec![],
                withhold_openings: false,
            }
        }

        /// The user's action on the device's screen: the replies that sends.
        fn act(&mut self, action: CardUserAction) -> Vec<DeviceToCoordinatorMessage> {
            let sends = self.signer.card_user_action(self.session.as_mut(), action);
            self.handle(sends)
        }

        fn job_running(&self) -> bool {
            self.session
                .as_ref()
                .is_some_and(|session| session.progress().is_some())
        }

        /// The device's replies to a message from the phone.
        fn recv(&mut self, message: CoordinatorToDeviceMessage) -> Vec<DeviceToCoordinatorMessage> {
            let CoordinatorToDeviceMessage::Card(message) = message else {
                panic!("not a card message: {message:?}");
            };
            if self.withhold_openings && matches!(message, CoordinatorCard::OpenBox { .. }) {
                return vec![];
            }
            let sends = self
                .signer
                .recv_card_message(message, self.session.as_mut());
            self.handle(sends)
        }

        /// Runs the session's job until it reports: the replies that sends.
        fn run_job(&mut self) -> Vec<DeviceToCoordinatorMessage> {
            let session = self.session.as_mut().expect("a session");
            let event = loop {
                if let Some(event) = session.step(&mut rand::thread_rng()) {
                    break event;
                }
            };
            let sends = self.signer.card_job_event(session, event, true);
            self.handle(sends)
        }

        fn handle(&mut self, sends: Vec<DeviceSend>) -> Vec<DeviceToCoordinatorMessage> {
            let mut replies = vec![];
            for send in sends {
                match send {
                    DeviceSend::ToCoordinator(message) => replies.push(*message),
                    DeviceSend::ToUser(message) => {
                        let DeviceToUserMessage::Card(card) = *message else {
                            continue;
                        };
                        match *card {
                            ToUserCard::OpenSession(start) => {
                                let mut session = start
                                    .open(
                                        &mut TestDerivation,
                                        card_setup(),
                                        &mut rand::thread_rng(),
                                    )
                                    .unwrap();
                                assert!(self.signer.card_session_opened(&mut session).is_empty());
                                self.session = Some(session);
                            }
                            ToUserCard::CloseSession => self.session = None,
                            ToUserCard::Screen(screen) => self.screens.push(screen),
                            _ => {}
                        }
                    }
                }
            }
            replies
        }
    }

    /// A new flow on a connected device, as the app starts one.
    fn new_flow(
        coordinator: &mut Persisted<FrostCoordinator>,
        db: &mut rusqlite::Connection,
        id: DeviceId,
        asref: AccessStructureRef,
        sink: FakeSink,
    ) -> CardBackupProtocol {
        let mut protocol = CardBackupProtocol::start(
            coordinator,
            db,
            id,
            asref,
            KEY,
            &mut rand::thread_rng(),
            sink,
        )
        .unwrap();
        protocol.connected(id, DeviceMode::Ready);
        protocol
    }

    fn mutation_rows(db: &rusqlite::Connection) -> u32 {
        db.query_row("SELECT COUNT(*) FROM fs_coordinator_mutations", [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    fn word_index(word: &str) -> u16 {
        frostsnap_core::coordinator::card::word_index(word).unwrap()
    }

    /// A card flow with a real device and a persisted phone, driven as the app's loop and the
    /// firmware drive them, and a scripted user who writes off both screens and answers both
    /// screens' quizzes.
    struct Flow {
        coordinator: Persisted<FrostCoordinator>,
        db: rusqlite::Connection,
        asref: AccessStructureRef,
        id: DeviceId,
        device: TestDevice,
        protocol: CardBackupProtocol,
        sink: FakeSink,
        /// The card as the user wrote it.
        card: [Option<u16>; 64],
        /// How far the flow's clock runs ahead of the real one.
        clock: Duration,
        /// The device screens and phone stages the user acted on already.
        device_acted: usize,
        phone_acted: usize,
        /// The user stops acting on the phone at the final check.
        hold_final: bool,
        /// The app sees another device on this device's USB port.
        port_shared: bool,
    }

    impl Flow {
        fn new() -> Flow {
            Flow::held(true)
        }

        /// Owner B15: the flow on an install that holds the wallet but didn't run its keygen.
        fn on_other_install() -> Flow {
            Flow::held(false)
        }

        fn held(created_here: bool) -> Flow {
            let (mut coordinator, mut db, asref, devices) = card_key_held(created_here);
            let (&id, signer) = devices.iter().next().unwrap();
            let sink = FakeSink::default();
            let protocol = new_flow(&mut coordinator, &mut db, id, asref, sink.clone());
            let mut flow = Flow {
                coordinator,
                db,
                asref,
                id,
                device: TestDevice::new(signer.clone()),
                protocol,
                sink,
                card: [None; 64],
                clock: Duration::ZERO,
                device_acted: 0,
                phone_acted: 0,
                hold_final: false,
                port_shared: false,
            };
            flow.begin();
            flow
        }

        fn now(&self) -> Instant {
            Instant::now() + self.clock
        }

        /// The protocol's `Begin`, from the loop's poll.
        fn begin(&mut self) {
            let now = self.now();
            let begin = self
                .protocol
                .poll_at(now)
                .into_iter()
                .map(|message| match message.message_body {
                    CoordinatorSendBody::Core(message) => message,
                    other => panic!("not a core message: {other:?}"),
                })
                .collect();
            self.pump(begin, true);
        }

        /// Delivers the phone's requests and the device's replies until both are idle. With
        /// `jobs` the device computes, and the protocol re-sends after a `Busy`; without, the
        /// device stalls.
        fn pump(&mut self, mut queue: VecDeque<CoordinatorToDeviceMessage>, jobs: bool) {
            loop {
                let replies = if let Some(message) = queue.pop_front() {
                    self.device.recv(message)
                } else if jobs && self.device.job_running() {
                    self.device.run_job()
                } else if let (true, resend @ [_, ..]) =
                    (jobs, &self.protocol.poll_at(self.now() + BUSY_RETRY)[..])
                {
                    queue.extend(resend.iter().map(|message| match &message.message_body {
                        CoordinatorSendBody::Core(message) => message.clone(),
                        other => panic!("not a core message: {other:?}"),
                    }));
                    continue;
                } else {
                    break;
                };
                queue.extend(self.pass(replies));
            }
        }

        /// One pass of the app's loop: the device's messages, then the question timeout on the
        /// flow's clock, before the outbox is drained.
        fn pass(
            &mut self,
            messages: impl IntoIterator<Item = DeviceToCoordinatorMessage>,
        ) -> Vec<CoordinatorToDeviceMessage> {
            let mut inbox = CardInbox::default();
            let mut outbox = vec![];
            for message in messages {
                outbox.extend(inbox.recv(
                    &mut self.coordinator,
                    &mut self.db,
                    self.id,
                    self.port_shared,
                    message,
                ));
            }
            let now = self.now();
            outbox.extend(inbox.device_timeout(
                &mut self.coordinator,
                &mut self.db,
                &mut self.protocol,
                now,
            ));
            let to_device = self.drain(outbox);
            inbox.apply(&mut self.protocol);
            to_device.into()
        }

        /// The outbox: the phone's events to the protocol, and its requests for the device.
        fn drain(&mut self, outbox: Vec<CoordinatorSend>) -> VecDeque<CoordinatorToDeviceMessage> {
            let mut to_device = VecDeque::new();
            for send in outbox {
                match send {
                    CoordinatorSend::ToDevice { message, .. } => to_device.push_back(message),
                    CoordinatorSend::ToUser(message) => {
                        let now = self.now();
                        self.protocol.process_to_user_message_at(message, now);
                    }
                }
            }
            to_device
        }

        fn act(&mut self, action: CardUserAction, jobs: bool) {
            let replies = self.device.act(action);
            let sent = self.pass(replies);
            self.pump(sent.into(), jobs);
        }

        /// A phone action, as `card_action` runs it.
        fn phone(
            &mut self,
            action: impl FnOnce(&mut FrostCoordinator) -> Result<Vec<CoordinatorSend>, CardActionError>,
            jobs: bool,
        ) {
            let mut sends =
                persist_card_sends(&mut self.coordinator, &mut self.db, action).unwrap();
            self.protocol.action_sent(&mut sends);
            let to_device = self.drain(sends);
            self.pump(to_device, jobs);
        }

        fn screen(&self) -> Option<&CardScreen> {
            self.device.screens.last()
        }

        fn stage(&self) -> CardFlowStage {
            self.sink.last().stage
        }

        /// The user acts on whatever either screen asks for, until `until` or nothing asks.
        fn run(&mut self, until: impl Fn(&Flow) -> bool, jobs: bool) {
            for _ in 0..10_000 {
                if until(self) {
                    return;
                }
                if self.device_acted < self.device.screens.len() {
                    self.device_acted = self.device.screens.len();
                    if let Some(action) = self.device_action() {
                        self.act(action, jobs);
                        continue;
                    }
                }
                if self.phone_acted < self.sink.len() {
                    self.phone_acted = self.sink.len();
                    if self.phone_step(jobs) {
                        continue;
                    }
                }
                return;
            }
            panic!("the flow doesn't settle");
        }

        fn device_action(&mut self) -> Option<CardUserAction> {
            Some(match self.screen()?.clone() {
                CardScreen::DeviceHalf { words } => {
                    for (j, word) in words.0.iter().enumerate() {
                        self.card[j] = Some(*word);
                    }
                    CardUserAction::DeviceHalfWritten
                }
                CardScreen::SelfCheck { .. } => CardUserAction::SelfCheckPassed,
                CardScreen::Recheck { .. } => CardUserAction::RecheckPassed,
                CardScreen::DeviceRoundIntro => CardUserAction::DeviceRoundIntroDone,
                CardScreen::DeviceRoundAnswer {
                    j,
                    options,
                    display_id,
                    ..
                } => {
                    let pick = options
                        .0
                        .iter()
                        .position(|&option| Some(option) == self.card[j as usize])
                        .map(|pick| pick as u8);
                    CardUserAction::DeviceRoundPick { display_id, pick }
                }
                CardScreen::DeviceRoundMismatch { .. } => CardUserAction::DeviceRoundMismatchAck,
                CardScreen::Verdict { passed: true } => CardUserAction::VerdictAck,
                _ => return None,
            })
        }

        /// The user's step on the phone's stage; false if it asks for nothing.
        fn phone_step(&mut self, jobs: bool) -> bool {
            let id = self.id;
            match self.stage() {
                CardFlowStage::PhoneWords {
                    first_word, words, ..
                } => {
                    for (i, word) in words.iter().enumerate() {
                        self.card[first_word as usize - 1 + i] = Some(word_index(word));
                    }
                    if let Some(page) = self.protocol.phone_next() {
                        self.phone(|c| c.card_phone_page(id, page), jobs);
                    }
                }
                CardFlowStage::PhoneAllWords { .. } => {
                    assert_eq!(self.protocol.phone_next(), None);
                }
                CardFlowStage::UnverifiedIntro => self.phone(|c| c.card_write_unverified(id), jobs),
                CardFlowStage::ConfirmDeviceHalf => self.phone(
                    |c| c.card_device_half_written(id, &mut rand::thread_rng()),
                    jobs,
                ),
                CardFlowStage::PhoneConfirm => self.phone(|c| c.card_phone_half_written(id), jobs),
                CardFlowStage::SelfCheck {
                    word_number,
                    options,
                    ..
                } => {
                    let written = self.card[word_number as usize - 1].expect("written");
                    assert!(options.iter().any(|o| word_index(o) == written));
                    self.phone(|c| c.card_self_check_answer(id, written), jobs);
                }
                CardFlowStage::ReadyForFinalCheck if !self.hold_final => {
                    self.phone(|c| c.card_final_begin(id, &mut rand::thread_rng()), jobs)
                }
                CardFlowStage::Question {
                    word_number,
                    options,
                    ..
                } => {
                    let written = self.card[word_number as usize - 1];
                    let answer =
                        match written.filter(|w| options.iter().any(|o| word_index(o) == *w)) {
                            Some(w) => BoxAnswer::Picked(w),
                            None => BoxAnswer::NoneOfThese,
                        };
                    self.phone(|c| c.card_answer(id, answer), jobs);
                }
                _ => return false,
            }
            true
        }

        fn status(&mut self) -> CardBackupStatus {
            let share_index = self
                .coordinator
                .get_access_structure(self.asref)
                .unwrap()
                .device_to_share_indicies()[&self.id];
            self.coordinator
                .card_backup_status(self.asref, share_index)
                .unwrap_or(CardBackupStatus::NoCard)
        }

        /// The app killed and started again: the soft reset ends the device's card session and
        /// it goes home (addendum §6), the phone loads from disk, and the user's Continue is a
        /// new flow.
        fn restart_app(&mut self) {
            self.device.signer.clear_tmp_data();
            self.device.session = None;
            self.coordinator = Persisted::<FrostCoordinator>::new(&mut self.db, ()).unwrap();
            self.sink = FakeSink::default();
            self.phone_acted = 0;
            self.protocol = new_flow(
                &mut self.coordinator,
                &mut self.db,
                self.id,
                self.asref,
                self.sink.clone(),
            );
            self.begin();
        }
    }

    /// The whole card through the app's loop (owner B1): words 1–32 on the device and its quiz,
    /// the confirm on the phone, words 33–64 on the phone's pages while the device proves, the
    /// phone's quiz, [Continue], then the device round and the phone round. Both halves are
    /// recorded at the phone's quiz (design §3.10), not at the verdict, and a later check
    /// through a new flow quizzes both screens and records nothing.
    #[test]
    fn a_card_through_the_app() {
        let mut flow = Flow::new();
        let mut recorded_at = None;
        flow.run(|_| false, true);
        let stages: Vec<String> = flow
            .sink
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|state| format!("{:?}", state.stage))
            .collect();
        let mut at = 0;
        for name in [
            "Pinning",
            "DeviceHalf",
            "ConfirmDeviceHalf",
            "PhoneWords",
            "PhoneConfirm",
            "SelfCheck",
            "ReadyForFinalCheck",
            "DeviceRound",
            "Question",
            "Passed",
        ] {
            at += stages[at..]
                .iter()
                .position(|stage| stage.starts_with(name))
                .unwrap_or_else(|| panic!("{name} after {:?}", &stages[..at]));
        }
        // The confirm goes straight to the first page: "Checking your card" doesn't flash.
        let first_page = stages
            .iter()
            .position(|stage| stage.starts_with("PhoneWords"))
            .expect("a page");
        assert!(
            !stages[..first_page]
                .iter()
                .any(|stage| stage.starts_with("Proving")),
            "{:?}",
            &stages[..first_page]
        );
        for (i, state) in flow.sink.0.lock().unwrap().iter().enumerate() {
            if state.halves_recorded && recorded_at.is_none() {
                recorded_at = Some((i, state.stage.clone()));
            }
        }
        let (_, at) = recorded_at.expect("both halves recorded");
        assert!(
            matches!(at, CardFlowStage::SelfCheck { .. }),
            "recorded at the phone's quiz: {at:?}"
        );
        assert!(
            matches!(flow.stage(), CardFlowStage::Passed),
            "{:?}",
            flow.stage()
        );
        assert_eq!(flow.status(), CardBackupStatus::Checked);
        assert!(matches!(
            flow.protocol.is_complete(),
            Some(Completion::Success)
        ));

        // A later check: a new flow.
        flow.restart_app();
        let rows = mutation_rows(&flow.db);
        flow.run(|_| false, true);
        assert!(matches!(flow.stage(), CardFlowStage::RecheckPassed));
        assert!(flow.sink.last().recheck_device_done);
        assert!(matches!(
            flow.protocol.is_complete(),
            Some(Completion::Success)
        ));
        assert_eq!(mutation_rows(&flow.db), rows, "nothing recorded");
    }

    /// Owner B15 through the app's loop: on an install that holds the wallet but didn't run its
    /// keygen, the flow says it can't verify the backup and waits for the user's choice; then
    /// words 1–32 on the device and its quiz, the confirm, words 33–64 on the phone's pages and
    /// the phone's quiz, and "written, not verified". It never pins, draws N, pulls proofs or
    /// openings, never records both halves for the backup run, and never reaches Checked; the
    /// device is left in CARD_CHECKED. A new flow after an app restart shows the written card
    /// and records nothing.
    #[test]
    fn an_unverified_card_through_the_app() {
        let mut flow = Flow::on_other_install();
        assert!(flow.sink.last().unverified);
        assert!(matches!(flow.stage(), CardFlowStage::UnverifiedIntro));
        assert_eq!(flow.status(), CardBackupStatus::NoCard);
        let rows = mutation_rows(&flow.db);
        flow.run(|_| false, true);
        let states = flow.sink.0.lock().unwrap().clone();
        let stages: Vec<String> = states
            .iter()
            .map(|state| format!("{:?}", state.stage))
            .collect();
        let mut at = 0;
        for name in [
            "UnverifiedIntro",
            "DeviceHalf",
            "ConfirmDeviceHalf",
            "PhoneWords",
            "PhoneAllWords",
            "PhoneConfirm",
            "SelfCheck",
            "UnverifiedWritten",
        ] {
            at += stages[at..]
                .iter()
                .position(|stage| stage.starts_with(name))
                .unwrap_or_else(|| panic!("{name} after {:?}", &stages[..at]));
        }
        for state in &states {
            assert!(state.unverified);
            assert!(!state.halves_recorded && !state.check_started);
            assert_eq!(state.proofs_verified, 0);
            assert!(!matches!(
                state.stage,
                CardFlowStage::Pinning
                    | CardFlowStage::Proving
                    | CardFlowStage::ReadyForFinalCheck
                    | CardFlowStage::Passed
            ));
        }
        assert!(flow.sink.last().record_changed || states.iter().any(|s| s.record_changed));
        assert_eq!(flow.status(), CardBackupStatus::Unverified);
        assert_eq!(
            mutation_rows(&flow.db),
            rows + 2,
            "the start and the written card"
        );
        assert!(matches!(
            flow.protocol.is_complete(),
            Some(Completion::Success)
        ));
        let share_index = flow
            .coordinator
            .get_access_structure(flow.asref)
            .unwrap()
            .device_to_share_indicies()[&flow.id];
        let share_image = flow
            .coordinator
            .root_shared_key(flow.asref, KEY)
            .unwrap()
            .share_image(share_index);
        let rec = flow
            .device
            .signer
            .card_record(&share_image)
            .unwrap()
            .clone();
        assert_eq!(rec.state, CardState::CardChecked);
        assert_eq!(rec.n_tag, None);
        assert!(
            !flow.device.screens.iter().any(|screen| matches!(
                screen,
                CardScreen::Preparing {
                    task: BusyTask::Proofs
                } | CardScreen::DeviceRoundIntro
            )),
            "no proofs, no device round"
        );
        assert!(
            flow.card.iter().all(Option::is_some),
            "all 64 words written"
        );

        // Opened again: the written card, nothing recorded.
        flow.restart_app();
        let rows = mutation_rows(&flow.db);
        flow.run(|_| false, true);
        assert!(matches!(flow.stage(), CardFlowStage::UnverifiedWritten));
        assert_eq!(mutation_rows(&flow.db), rows);
        assert_eq!(flow.status(), CardBackupStatus::Unverified);
    }

    /// Owner B9: the phone's deadline runs from N, not from the gate. A device that withholds
    /// an opening while words 33–64 are written (plugged in, no unplug) is aborted once the
    /// work is overdue, persisted, with the suspect alarm, and the deadline fires once. Before N
    /// nothing is owed, and an honest device round, with every opening in, owes nothing either:
    /// the user takes as long as they need.
    #[test]
    fn a_stall_after_n_is_an_alarm() {
        let later = Instant::now() + Duration::from_secs(3600);
        let fire = |flow: &mut Flow, inbox: &mut CardInbox, now| {
            inbox.device_timeout(&mut flow.coordinator, &mut flow.db, &mut flow.protocol, now)
        };

        // An honest device round: nothing owed.
        let mut flow = Flow::new();
        flow.run(
            |flow| matches!(flow.screen(), Some(CardScreen::DeviceRoundIntro)),
            true,
        );
        flow.pump(VecDeque::new(), true);
        let mut inbox = CardInbox::default();
        assert!(fire(&mut flow, &mut inbox, later).is_empty());
        assert_eq!(flow.protocol.device_work, None);
        flow.run(|_| false, true);
        assert!(matches!(flow.stage(), CardFlowStage::Passed));

        let mut flow = Flow::new();
        flow.device.withhold_openings = true;
        flow.run(
            |flow| matches!(flow.stage(), CardFlowStage::ConfirmDeviceHalf),
            true,
        );
        let mut inbox = CardInbox::default();
        assert!(fire(&mut flow, &mut inbox, later).is_empty(), "before N");
        // N goes out, the proofs come in, and the first opening is asked for and withheld.
        flow.run(
            |flow| {
                matches!(
                    flow.protocol.device_work,
                    Some((
                        CardDeviceWork::Opening {
                            proofs_first: false
                        },
                        _
                    ))
                )
            },
            true,
        );
        assert!(flow.sink.last().check_started);
        let due = flow.protocol.work_due.expect("a deadline");
        assert!(fire(&mut flow, &mut inbox, due - Duration::from_secs(1)).is_empty());
        let sends = fire(&mut flow, &mut inbox, due + Duration::from_secs(1));
        assert_eq!(flow.status(), CardBackupStatus::Failed);
        let to_device = flow.drain(sends);
        assert!(matches!(
            &to_device.iter().collect::<Vec<_>>()[..],
            [CoordinatorToDeviceMessage::Card(
                CoordinatorCard::PhoneRound { pass: false, .. }
            )]
        ));
        assert!(
            matches!(
                flow.stage(),
                CardFlowStage::Failed {
                    alarm: CardAlarm {
                        device_suspect: true,
                        next_step: NextStep::WithoutThisDevice,
                        ..
                    },
                }
            ),
            "{:?}",
            flow.stage()
        );
        assert!(
            fire(&mut flow, &mut inbox, later).is_empty(),
            "it fires once"
        );
        flow.pump(to_device, false);
        assert_eq!(flow.screen(), Some(&CardScreen::PhoneRoundFailed));
    }

    /// Owner B9: each item of work has its own deadline from when it began, so an honest
    /// device that keeps making progress is never overdue, and the deadline for an opening a
    /// resumed device computes only after all its proofs again is the longer one.
    #[test]
    fn the_deadline_follows_the_work() {
        let (mut protocol, _sink) = protocol();
        let now = Instant::now();
        protocol.poll_at(now);
        protocol.track_device_work(Some((CardDeviceWork::Proofs, 16)), now);
        let first = protocol.work_due.unwrap();
        assert_eq!(first, now + DEVICE_WORK_TIMEOUT);
        // The same item still owed: the clock runs on.
        protocol.track_device_work(
            Some((CardDeviceWork::Proofs, 16)),
            now + Duration::from_secs(60),
        );
        assert_eq!(protocol.work_due, Some(first));
        // Progress: a new deadline.
        let later = now + Duration::from_secs(100);
        protocol.track_device_work(Some((CardDeviceWork::Proofs, 32)), later);
        assert_eq!(protocol.work_due, Some(later + DEVICE_WORK_TIMEOUT));
        assert!(!protocol.device_overdue(now + DEVICE_WORK_TIMEOUT + Duration::from_secs(1)));
        protocol.track_device_work(
            Some((CardDeviceWork::Opening { proofs_first: true }, 0)),
            later,
        );
        assert_eq!(
            protocol.work_due,
            Some(later + DEVICE_WORK_TIMEOUT_AFTER_PROOFS)
        );
        // Unplugged: no clock; plugged in again, a fresh one.
        protocol.disconnected(DEVICE);
        protocol.track_device_work(Some((CardDeviceWork::Status, 0)), later);
        assert_eq!(protocol.work_due, None);
    }

    impl Flow {
        /// The device is unplugged, as the app's loop handles it: core first (its events go in
        /// the outbox), then the flow, then the outbox. The device reboots to its home screen.
        fn unplug(&mut self) {
            self.device.signer.clear_tmp_data();
            self.device.session = None;
            let mut inbox = CardInbox::default();
            let sends = inbox.disconnected(&mut self.coordinator, &mut self.db, self.id);
            self.protocol.disconnected(self.id);
            assert!(
                self.drain(sends).is_empty(),
                "nothing to an unplugged device"
            );
        }

        /// Plugged in again: the loop's `connected`, then its poll.
        fn replug(&mut self) {
            self.protocol.connected(self.id, DeviceMode::Ready);
            self.begin();
        }
    }

    /// Owner B9: an unplug after N keeps the flow on its question, "did you cause it?": it
    /// sends the device nothing, not even `Begin` once it's back, until the user answers. "Yes,
    /// it was me" goes back to the page of words the question covered, `Begin` follows, and
    /// the card passes on the same N.
    #[test]
    fn an_unplug_after_n_asks_whether_it_was_you() {
        let mut flow = Flow::new();
        flow.run(
            |flow| matches!(flow.stage(), CardFlowStage::PhoneWords { page: 2, .. }),
            true,
        );
        assert!(flow.sink.last().check_started, "N is out: no Stop");
        flow.unplug();
        assert!(matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted {
                disconnected: true,
                ..
            }
        ));
        assert!(flow.protocol.is_complete().is_none(), "the flow goes on");
        assert!(!flow.sink.last().device_connected);
        let screens = flow.device.screens.len();
        flow.replug();
        assert_eq!(
            flow.device.screens.len(),
            screens,
            "no Begin before the answer"
        );
        assert!(matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted { .. }
        ));
        // Nothing else is taken meanwhile.
        let id = flow.id;
        assert!(
            persist_card_sends(&mut flow.coordinator, &mut flow.db, |c| {
                c.card_phone_page(id, 3)
            })
            .is_err()
        );

        flow.phone(|c| c.card_interruption_answer(id, true), true);
        assert!(
            matches!(flow.stage(), CardFlowStage::PhoneWords { page: 2, .. }),
            "{:?}",
            flow.stage()
        );
        flow.begin();
        flow.phone_acted = 0;
        flow.run(|_| false, true);
        assert!(
            matches!(flow.stage(), CardFlowStage::Passed),
            "{:?}",
            flow.stage()
        );
        assert_eq!(flow.status(), CardBackupStatus::Checked);
    }

    /// Re-attack RA-1 at the app layer: after an unplug and "yes", the device answers the
    /// `Begin` sent again with `NotDirectUsb`. Alone on its port, it's plugged straight in, so
    /// that's a lie: the suspect alarm. Behind another device the app can see, it's the
    /// link's refusal: "plug it in straight", the check stays running, and no deadline runs.
    #[test]
    fn a_not_direct_usb_after_n_follows_the_port() {
        for port_shared in [false, true] {
            let mut flow = Flow::new();
            flow.run(
                |flow| matches!(flow.stage(), CardFlowStage::PhoneWords { page: 2, .. }),
                true,
            );
            flow.unplug();
            flow.replug();
            let id = flow.id;
            // Not `flow.phone`: its pump would poll the `Begin` out and drop it.
            let mut sends = persist_card_sends(&mut flow.coordinator, &mut flow.db, |c| {
                c.card_interruption_answer(id, true)
            })
            .unwrap();
            flow.protocol.action_sent(&mut sends);
            assert!(flow.drain(sends).is_empty());
            // The `Begin` goes out; the device's reply is the refusal.
            let now = flow.now();
            assert!(!flow.protocol.poll_at(now).is_empty(), "the Begin");
            flow.port_shared = port_shared;
            flow.pass([DeviceToCoordinatorMessage::Card(DeviceCard::Refused {
                reason: frostsnap_core::message::card::CardRefusal::NotDirectUsb,
            })]);
            if port_shared {
                assert!(
                    matches!(
                        flow.stage(),
                        CardFlowStage::Refused { ref message }
                            if message == "Plug the device straight into this phone or computer."
                    ),
                    "{:?}",
                    flow.stage()
                );
                assert_eq!(flow.status(), CardBackupStatus::NotChecked);
                assert!(!flow.coordinator.device_is_suspect(id));
                assert_eq!(flow.protocol.work_due, None);
            } else {
                assert!(
                    matches!(
                        flow.stage(),
                        CardFlowStage::Failed {
                            alarm: CardAlarm {
                                device_suspect: true,
                                next_step: NextStep::WithoutThisDevice,
                                ..
                            },
                        }
                    ),
                    "{:?}",
                    flow.stage()
                );
                assert_eq!(flow.status(), CardBackupStatus::Failed);
                assert!(flow.coordinator.device_is_suspect(id));
            }
        }
    }

    /// Owner B9: "No, I didn't" marks the device suspect and ends the check with its own
    /// alarm, whose next step leaves the device out.
    #[test]
    fn no_i_didnt_is_the_suspect_alarm() {
        let mut flow = Flow::new();
        flow.run(
            |flow| matches!(flow.screen(), Some(CardScreen::DeviceRoundAnswer { .. })),
            true,
        );
        flow.unplug();
        let id = flow.id;
        flow.phone(|c| c.card_interruption_answer(id, false), true);
        assert!(
            matches!(
                flow.stage(),
                CardFlowStage::Failed {
                    alarm: CardAlarm {
                        kind: CardAlarmKind::DisconnectedAfterN,
                        device_suspect: true,
                        next_step: NextStep::WithoutThisDevice,
                    },
                }
            ),
            "{:?}",
            flow.stage()
        );
        assert_eq!(flow.status(), CardBackupStatus::Failed);
        assert!(flow.coordinator.device_is_suspect(id));
        assert!(matches!(
            flow.protocol.is_complete(),
            Some(Completion::Success)
        ));
    }

    /// Owner R2-2 at the app layer: three unplugs answered "Yes, it was me" each resume the
    /// check; the fourth "yes" ends it with its own alarm, the device suspect, and the next
    /// step leaving it out.
    #[test]
    fn a_fourth_yes_is_the_interrupted_too_often_alarm() {
        let mut flow = Flow::new();
        flow.run(
            |flow| matches!(flow.stage(), CardFlowStage::PhoneWords { page: 2, .. }),
            true,
        );
        let id = flow.id;
        for resume in 0..=frostsnap_core::coordinator::card::MAX_INTERRUPTION_RESUMES {
            flow.unplug();
            // Review B15 protocol F2: the page knows when this "yes" is the last resume.
            let left = frostsnap_core::coordinator::card::MAX_INTERRUPTION_RESUMES - resume;
            assert!(
                matches!(
                    flow.stage(),
                    CardFlowStage::CheckInterrupted {
                        disconnected: true,
                        resumes_left,
                    } if resumes_left == left
                ),
                "{resume}: {:?}",
                flow.stage()
            );
            flow.replug();
            flow.phone(|c| c.card_interruption_answer(id, true), true);
            if resume < frostsnap_core::coordinator::card::MAX_INTERRUPTION_RESUMES {
                assert!(
                    matches!(flow.stage(), CardFlowStage::PhoneWords { .. }),
                    "{resume}: {:?}",
                    flow.stage()
                );
                assert!(!flow.coordinator.device_is_suspect(id));
                flow.begin();
            }
        }
        assert!(
            matches!(
                flow.stage(),
                CardFlowStage::Failed {
                    alarm: CardAlarm {
                        kind: CardAlarmKind::InterruptedTooOften,
                        device_suspect: true,
                        next_step: NextStep::WithoutThisDevice,
                    },
                }
            ),
            "{:?}",
            flow.stage()
        );
        assert_eq!(flow.status(), CardBackupStatus::Failed);
        assert!(flow.coordinator.device_is_suspect(id));
    }

    /// Owner B9: an app restart while the check runs opens the new flow on the same question
    /// (the wording for a closed app), with no `Begin` until the user answers.
    #[test]
    fn an_app_restart_after_n_opens_on_the_question() {
        let mut flow = Flow::new();
        flow.run(
            |flow| matches!(flow.stage(), CardFlowStage::PhoneConfirm),
            true,
        );
        let screens = flow.device.screens.len();
        flow.restart_app();
        assert!(matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted {
                disconnected: false,
                resumes_left: 3,
            }
        ));
        assert_eq!(flow.device.screens.len(), screens, "no Begin");
        // Closed again before answering: the question is still there.
        flow.restart_app();
        assert!(matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted { .. }
        ));
        let id = flow.id;
        flow.phone(|c| c.card_interruption_answer(id, true), true);
        assert!(!matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted { .. }
        ));
        flow.run(|_| false, true);
        assert!(
            matches!(flow.stage(), CardFlowStage::Passed),
            "{:?}",
            flow.stage()
        );
    }

    /// The loop as the app runs it: a `Busy` sets the re-send, the device's unsolicited frames
    /// leave it pending, and a reply other than `Busy` clears it (addendum U6).
    #[test]
    fn unsolicited_frames_keep_the_resend() {
        let (mut protocol, _sink) = protocol();
        let (mut coordinator, mut db) = persisted();
        let now = Instant::now();
        protocol.poll_at(now);
        protocol.on_event(
            CardEvent::Busy {
                resend: Box::new(CoordinatorToDeviceMessage::Card(
                    CoordinatorCard::PullProofs { from: 0, count: 16 },
                )),
            },
            now,
        );
        for frame in [
            DeviceCard::GateShown,
            DeviceCard::CheckDone,
            DeviceCard::DeviceRound { pass: true },
            DeviceCard::RecheckDone,
            DeviceCard::RestoreState {
                device_half_in: false,
                phone_half_in: false,
                decoding: false,
            },
            DeviceCard::RestoreOutcome {
                result: RestoreResult::CouldNotRead,
            },
            DeviceCard::Refused {
                reason: CardRefusal::Busy,
            },
        ] {
            let mut inbox = CardInbox::default();
            inbox.recv(
                &mut coordinator,
                &mut db,
                DEVICE,
                false,
                DeviceToCoordinatorMessage::Card(frame),
            );
            inbox.apply(&mut protocol);
        }
        assert!(protocol.resend.is_some(), "still pending");
        let mut inbox = CardInbox::default();
        inbox.recv(
            &mut coordinator,
            &mut db,
            DEVICE,
            false,
            DeviceToCoordinatorMessage::Card(DeviceCard::Refused {
                reason: CardRefusal::NotDirectUsb,
            }),
        );
        inbox.apply(&mut protocol);
        assert!(protocol.poll_at(now + BUSY_RETRY).is_empty(), "a reply");
    }

    /// Audit F1 at the app layer, inverted (2 Nov 2026): once both halves are recorded, a chip
    /// that never sent a proof claims PASSED under the pinned seal. Unasked, its `Status` is
    /// refused; as the reply to the `Begin` a `NoSession` makes the protocol send again, it's
    /// another phone's check. Either way the phone sends no `Recheck`, its record stays
    /// NotChecked, and no stage is ever `RecheckPassed` or `Passed`: the app's "Backup checked"
    /// needs this phone's own final check.
    #[test]
    fn app_never_shows_backup_checked_without_the_final_check() {
        use frostsnap_core::message::card::{
            CardCheckReport, CardRefusal, CardSealed, CardStateReport, CardStatus,
        };
        let mut flow = Flow::new();
        flow.hold_final = true;
        // The chip never computes a proof: the honest writing runs until this phone's half is
        // recorded.
        flow.run(|flow| flow.protocol.state.halves_recorded, false);
        assert!(flow.protocol.state.halves_recorded);
        let share_index = flow
            .coordinator
            .get_access_structure(flow.asref)
            .unwrap()
            .device_to_share_indicies()[&flow.id];
        let rec = flow
            .coordinator
            .card_record(flow.asref, share_index)
            .unwrap()
            .clone();
        // Owner B8: N went out with the confirm of words 1–32.
        assert!(!rec.proven());
        let pin = rec.pin.unwrap();
        let share_image = flow
            .coordinator
            .root_shared_key(flow.asref, KEY)
            .unwrap()
            .share_image(share_index);
        let passed = || {
            DeviceToCoordinatorMessage::Card(DeviceCard::Status(Box::new(CardStatus {
                share_image,
                format_id: frostsnap_core::coordinator::card::CARD_FORMAT,
                proof_version: pin.proof_version,
                sealed: CardSealed {
                    format_id: frostsnap_core::coordinator::card::CARD_FORMAT,
                    h_c: pin.h_c,
                    coin_commit: pin.coin_commit,
                },
                state: CardStateReport::Passed,
                check: CardCheckReport::Ended { passed: true },
                reason: None,
            })))
        };
        let recheck = |sent: &[CoordinatorToDeviceMessage]| {
            sent.iter().any(|m| {
                matches!(
                    m,
                    CoordinatorToDeviceMessage::Card(CoordinatorCard::Recheck { .. })
                )
            })
        };
        // Unasked.
        let before = flow.stage();
        let sent = flow.pass([passed()]);
        assert!(sent.is_empty(), "{sent:?}");
        assert_eq!(format!("{:?}", flow.stage()), format!("{before:?}"));
        // After a `NoSession`: owner B9, the check runs, so it's an interruption, and nothing
        // from the device is taken until the user answers.
        flow.pass([DeviceToCoordinatorMessage::Card(DeviceCard::Refused {
            reason: CardRefusal::NoSession,
        })]);
        assert!(matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted {
                disconnected: true,
                ..
            }
        ));
        let sent = flow.pass([passed()]);
        assert!(sent.is_empty(), "{sent:?}");
        assert!(matches!(
            flow.stage(),
            CardFlowStage::CheckInterrupted { .. }
        ));
        // "Yes, it was me", and the chip's PASSED as the reply to the `Begin` sent again.
        let id = flow.id;
        flow.phone(|c| c.card_interruption_answer(id, true), false);
        let sent = flow.pass([passed()]);
        assert!(!recheck(&sent), "{sent:?}");
        // Owner B8: this phone sent N with the confirm, so the chip's PASSED is an abort.
        assert!(
            matches!(
                flow.stage(),
                CardFlowStage::Failed {
                    alarm: CardAlarm {
                        device_suspect: true,
                        ..
                    }
                }
            ),
            "{:?}",
            flow.stage()
        );
        assert_eq!(flow.status(), CardBackupStatus::Failed);
        assert!(!flow.sink.0.lock().unwrap().iter().any(|state| matches!(
            state.stage,
            CardFlowStage::RecheckPassed | CardFlowStage::Passed
        )));
        let rec = flow
            .coordinator
            .card_record(flow.asref, share_index)
            .unwrap();
        assert!(rec.device_round.is_none() && rec.round != Some(Ok(())));
    }
}
