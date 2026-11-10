use alloc::boxed::Box;
use alloc::string::ToString;
use bitcoin::Address;
use frost_backup::ShareBackup;
use frostsnap_comms::{DeviceName, Sha256Digest};
use frostsnap_core::{
    device::{
        card::{CardScreen, CardUserAction, HomeLine},
        card_restore::{
            CardRestoreDecoded, CardRestorePhase, CardRestoreReport, RestoreFailure,
            RestoreFindings,
        },
        card_session::BusyTask as CardTask,
        restoration::{ConsolidatePhase, EnterBackupPhase, ThresholdWarning},
        KeyGenPhase3, SignPhase1,
    },
    message::{
        card::{CardHalfEntries, CardRefusal},
        HeldShare2,
    },
    schnorr_fun::frost::ShareIndex,
    tweak::BitcoinBip32Path,
    AccessStructureRef, SignTask,
};
use frostsnap_widgets::{
    backup::{BackupDisplay, CheckBackupScreen, EnterShareScreen},
    card::*,
    keygen_check::KeygenCheck,
    layout::*,
    sign_prompt::SignTxPrompt,
    AddressWithIndex, DeviceNameScreen, EraseDevice, EraseProgress, FirmwareUpgradeConfirm,
    FirmwareUpgradeProgress, SignMessageConfirm, Standby,
};

use crate::ui::{FirmwareUpgradeStatus, UiEvent};
use embedded_graphics::geometry::Size;
use frostsnap_widgets::DynWidget;

/// The widget tree represents the current UI state as a tree of widgets
#[derive(frostsnap_macros::Widget)]
#[widget_crate(frostsnap_widgets)]
pub enum WidgetTree {
    /// Standby screen (can show startup/empty, welcome, or key info)
    Standby(Box<Standby>),

    /// Device naming screen
    DeviceNaming(Box<DeviceNameScreen>),

    /// Keygen confirmation screen
    KeygenCheck {
        widget: Box<KeygenCheck>,
        phase: Option<Box<KeyGenPhase3>>,
    },

    /// Sign transaction prompt screen
    SignTxPrompt {
        widget: Box<SignTxPrompt>,
        phase: Option<Box<SignPhase1>>,
    },

    /// Sign test message prompt screen
    SignTestPrompt {
        widget: Box<SignMessageConfirm>,
        phase: Option<Box<SignPhase1>>,
    },

    /// Firmware upgrade confirmation screen
    FirmwareUpgradeConfirm {
        widget: Box<FirmwareUpgradeConfirm>,
        firmware_hash: [u8; 32],
        firmware_size: u32,
        confirmed: bool,
    },

    /// Firmware upgrade progress screen
    FirmwareUpgradeProgress {
        widget: Box<FirmwareUpgradeProgress>,
        status: FirmwareUpgradeStatus,
    },

    /// Device erase confirmation prompt
    EraseDevicePrompt {
        widget: Box<EraseDevice>,
        confirmed: bool,
    },

    /// Display backup screen
    DisplayBackup {
        widget: Box<BackupDisplay>,
        fired: bool,
    },

    /// Display Bitcoin address screen with derivation path
    AddressDisplay(Box<Center<frostsnap_widgets::AddressWithIndex>>),

    /// Enter backup screen
    EnterBackup {
        widget: Box<EnterShareScreen>,
        phase: Option<EnterBackupPhase>,
    },

    /// Erase progress screen
    EraseProgress { widget: Box<EraseProgress> },

    /// Check backup quiz screen
    CheckBackup {
        widget: Box<CheckBackupScreen>,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        fired: bool,
    },

    /// A card page left by a swipe, a hold or a button: `on_done` is sent once.
    CardPage {
        widget: Box<CardInfoPage>,
        on_done: Option<CardPageDone>,
    },

    /// "Preparing your card", while the encode or the seal is computed
    CardPreparing {
        widget: Box<CardBusyScreen>,
        task: CardTask,
    },

    /// D2: words 1-32 to write, on the standard backup display. `fired` once the hold is done.
    CardDeviceHalf {
        widget: Box<BackupDisplay>,
        fired: bool,
    },

    /// D3 and D9: the standard check quiz on words 1-32 (and, in a later check, the 5 words of
    /// the device round). `fired` once the last word is picked right. The quiz is built from
    /// `pending` by [`WidgetTree::build_pending`] on the next UI poll, not in `set_workflow`,
    /// whose call chains are the deepest on the stack.
    CardSelfCheck {
        widget: Option<Box<CheckBackupScreen>>,
        pending: Option<Box<PendingQuiz>>,
        recheck: bool,
        fired: bool,
    },

    /// "Backup checked" with the standard check circle; `on_done` is sent once it has been up long
    /// enough
    CardSuccess {
        widget: Box<CardSuccess>,
        on_done: Option<CardPageDone>,
    },

    /// The device round's answer screen, moved from word to word in place
    CardDeviceRound(Box<DeviceRoundAnswer>),

    /// The dots of the phone round, updated in place
    CardGate(Box<GateScreen>),

    /// R1: the entry of words 1-32 for a restore; `phase` is taken once the user holds "Done"
    CardRestoreEntry {
        widget: Box<CardRestoreEntry>,
        phase: Option<Box<CardRestorePhase>>,
    },

    /// "Checking your card", over the whole decode time
    CardRestoreChecking(Box<CardBusyScreen>),

    /// The pages of a decoded card, ending with the hold "Hold to restore this key"
    CardRestoreReport {
        widget: Box<CardInfoPage>,
        page: RestoreReportPage,
        findings: RestoreFindings,
        decoded: Option<Box<CardRestoreDecoded>>,
    },

    /// Consolidation of a card share whose threshold isn't the wallet's, or doesn't read: the
    /// threshold warning, left only by its hold (build plan A11). `phase` is taken once held.
    ConsolidateThresholdHold {
        widget: Box<CardInfoPage>,
        phase: Option<Box<ConsolidatePhase>>,
    },
}

/// A quiz to build: (word number, word) per item, and the option shuffle's seed.
pub struct PendingQuiz {
    items: alloc::vec::Vec<(u8, u16)>,
    seed: u32,
}

/// What leaving a card page sends.
#[derive(Clone, Debug)]
pub enum CardPageDone {
    Action(CardUserAction),
    Exit,
    /// "Edit words 1-32" after a failed restore decode.
    RestoreEdit,
}

impl From<CardPageDone> for UiEvent {
    fn from(done: CardPageDone) -> Self {
        match done {
            CardPageDone::Action(action) => UiEvent::Card(action),
            CardPageDone::Exit => UiEvent::CardExit,
            CardPageDone::RestoreEdit => UiEvent::CardRestoreEdit,
        }
    }
}

/// The pages of a decoded card, in order (R4). Each page after the first is shown only when
/// its finding is there. A decoded card's masks passed (owner B7): one whose masks fail is
/// "Couldn't read the card", with no report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreReportPage {
    KeyRestored,
    DidntFit,
    Unreadable,
    /// The card's threshold isn't the wallet record's, or doesn't read (spec PK.3).
    Threshold,
    /// The canonical check failed: "wallet record not confirmed".
    RecordDiffers,
    Hold,
}

impl RestoreReportPage {
    /// The page after this one, or `None` after the hold. None of the warnings stops the
    /// restore (spec PK.3, PK.8).
    pub fn next(self, findings: &RestoreFindings) -> Option<Self> {
        use RestoreReportPage::*;
        let shown = |page: Self| match page {
            KeyRestored | Hold => true,
            DidntFit => findings.corrected != 0,
            Unreadable => findings.erased != 0,
            Threshold => findings.threshold_differs || findings.threshold.is_none(),
            RecordDiffers => findings.canonical == Some(false),
        };
        [
            KeyRestored,
            DidntFit,
            Unreadable,
            Threshold,
            RecordDiffers,
            Hold,
        ]
        .into_iter()
        .skip_while(|page| *page != self)
        .skip(1)
        .find(|page| shown(*page))
    }

    /// `wallet_threshold` is the threshold of the wallet record the phone sent, if it sent one.
    fn page(self, findings: &RestoreFindings, wallet_threshold: Option<u16>) -> CardInfoPage {
        match self {
            RestoreReportPage::KeyRestored => key_restored_page(
                findings
                    .index
                    .try_into()
                    .expect("a card's share index fits in u16"),
                findings.threshold,
                findings.canonical == Some(true),
            ),
            RestoreReportPage::DidntFit => words_didnt_fit_page(findings.corrected),
            RestoreReportPage::Unreadable => words_unreadable_page(findings.erased),
            RestoreReportPage::Threshold => {
                threshold_warning_page(findings.threshold, wallet_threshold)
            }
            RestoreReportPage::RecordDiffers => COPY_RECORD_DIFFERS.page(),
            RestoreReportPage::Hold => COPY_HOLD_TO_RESTORE.page(),
        }
    }
}

impl WidgetTree {
    // Keeping widget construction in non-inlined constructors prevents its
    // temporaries from inflating FrostyUi::set_workflow's checked stack frame.
    /// The home screen: blank at startup, the welcome, or the key with its card line.
    #[inline(never)]
    pub(crate) fn build_standby(
        key: Option<(DeviceName, HeldShare2, Option<HomeLine>)>,
        welcome: bool,
    ) -> Self {
        let mut standby = Box::new(Standby::new(crate::FIRMWARE_VERSION));
        match key {
            Some((device_name, held_share, card_line)) => {
                standby.set_key_with_card_line(device_name.to_string(), held_share, card_line)
            }
            None if welcome => standby.set_welcome(),
            None => {}
        }
        Self::Standby(standby)
    }

    #[inline(never)]
    pub(crate) fn build_device_naming(new_name: &DeviceName) -> Self {
        Self::DeviceNaming(Box::new(DeviceNameScreen::new(new_name.to_string())))
    }

    #[inline(never)]
    pub(crate) fn build_firmware_upgrade(status: FirmwareUpgradeStatus) -> Self {
        let widget = Box::new(match status {
            FirmwareUpgradeStatus::Erase { progress } => FirmwareUpgradeProgress::erasing(progress),
            FirmwareUpgradeStatus::Download { progress } => {
                FirmwareUpgradeProgress::downloading(progress)
            }
            FirmwareUpgradeStatus::Passive => FirmwareUpgradeProgress::passive(),
            FirmwareUpgradeStatus::Rejected { reason } => {
                FirmwareUpgradeProgress::rejected(&reason.to_string())
            }
        });
        Self::FirmwareUpgradeProgress { widget, status }
    }

    #[inline(never)]
    pub(crate) fn build_erase_progress(progress: frostsnap_widgets::Frac) -> Self {
        Self::EraseProgress {
            widget: Box::new(EraseProgress::new(progress)),
        }
    }

    #[inline(never)]
    pub(crate) fn build_keygen_check(phase: Box<KeyGenPhase3>) -> Self {
        let session_hash = phase.session_hash();
        let mut security_check_code = [0u8; 4];
        security_check_code.copy_from_slice(&session_hash.0[..4]);
        let widget = KeygenCheck::new(phase.t_of_n(), security_check_code);
        Self::KeygenCheck {
            widget: Box::new(widget),
            phase: Some(phase),
        }
    }

    #[inline(never)]
    pub(crate) fn build_confirm_firmware_upgrade(firmware_digest: Sha256Digest, size: u32) -> Self {
        let widget = Box::new(FirmwareUpgradeConfirm::new(firmware_digest.0, size));
        Self::FirmwareUpgradeConfirm {
            widget,
            firmware_hash: firmware_digest.0,
            firmware_size: size,
            confirmed: false,
        }
    }

    #[inline(never)]
    pub(crate) fn build_erase_device() -> Self {
        Self::EraseDevicePrompt {
            widget: Box::new(EraseDevice::new()),
            confirmed: false,
        }
    }

    #[inline(never)]
    pub(crate) fn build_signing_prompt(phase: Box<SignPhase1>, rand_seed: u32) -> Self {
        let sign_task = phase.sign_task();

        match &sign_task.inner {
            SignTask::BitcoinTransaction {
                tx_template,
                network,
            } => {
                let prompt = tx_template.user_prompt(*network);
                let widget = Box::new(SignTxPrompt::new_with_seed(prompt, rand_seed));
                Self::SignTxPrompt {
                    widget,
                    phase: Some(phase),
                }
            }
            SignTask::Test { message } => {
                let widget = Box::new(SignMessageConfirm::new(message.clone()));
                Self::SignTestPrompt {
                    widget,
                    phase: Some(phase),
                }
            }
            SignTask::Nostr { .. } => {
                let mut standby = Standby::new(crate::FIRMWARE_VERSION);
                standby.set_welcome();
                Self::Standby(Box::new(standby))
            }
        }
    }

    #[inline(never)]
    pub(crate) fn build_display_backup(backup: ShareBackup) -> Self {
        let word_indices = backup.to_word_indices();
        let share_index: u16 = backup
            .index()
            .try_into()
            .expect("Share index should fit in u16");
        let backup_display = BackupDisplay::new(word_indices, share_index);
        Self::DisplayBackup {
            widget: Box::new(backup_display),
            fired: false,
        }
    }

    #[inline(never)]
    pub(crate) fn build_check_backup(
        backup: ShareBackup,
        access_structure_ref: AccessStructureRef,
        rand_seed: u32,
    ) -> Self {
        let word_indices = backup.to_word_indices();
        let share_index = backup.index();
        let display_share_index: u16 = share_index
            .try_into()
            .expect("Share index should fit in u16");
        let check_backup = CheckBackupScreen::new(word_indices, display_share_index, rand_seed);
        Self::CheckBackup {
            widget: Box::new(check_backup),
            access_structure_ref,
            share_index,
            fired: false,
        }
    }

    #[inline(never)]
    pub(crate) fn build_entering_backup(phase: EnterBackupPhase) -> Self {
        let mut widget = Box::new(EnterShareScreen::new());
        if cfg!(feature = "prefill-words") {
            widget.prefill_test_words();
        }
        Self::EnterBackup {
            widget,
            phase: Some(phase),
        }
    }

    #[inline(never)]
    pub(crate) fn build_display_address(
        address: Address,
        bip32_path: BitcoinBip32Path,
        rand_seed: u32,
    ) -> Self {
        let address_display =
            AddressWithIndex::new_with_seed(address, bip32_path.index.to_u32() as usize, rand_seed);
        Self::AddressDisplay(Box::new(Center::new(address_display)))
    }
}

// Card screens (design §4.1). Only a half is ever put into a screen: words 1-32, or the 5 words
// of the device round; core wipes them where it put them.
impl WidgetTree {
    #[inline(never)]
    pub(crate) fn build_card_page(copy: CardCopy, on_done: Option<CardPageDone>) -> Self {
        Self::CardPage {
            widget: Box::new(copy.page()),
            on_done,
        }
    }

    /// D2: words 1-32 on the standard backup display.
    #[inline(never)]
    pub(crate) fn build_card_device_half(words: &[u16; 32]) -> Self {
        Self::CardDeviceHalf {
            widget: Box::new(BackupDisplay::card_half(*words)),
            fired: false,
        }
    }

    /// D3 and D9: the standard quiz over `items`, (word number, word) each, built on the next UI
    /// poll.
    #[inline(never)]
    pub(crate) fn build_card_self_check(items: &[(u8, u16)], seed: u32, recheck: bool) -> Self {
        Self::CardSelfCheck {
            widget: None,
            pending: Some(Box::new(PendingQuiz {
                items: items.to_vec(),
                seed,
            })),
            recheck,
            fired: false,
        }
    }

    /// Builds a screen that `set_workflow` left to the UI poll (the quiz), sized for the whole
    /// display.
    #[inline(never)]
    pub(crate) fn build_pending(&mut self, screen: Size) {
        if let Self::CardSelfCheck {
            widget: widget @ None,
            pending,
            recheck,
            ..
        } = self
        {
            let Some(pending) = pending.take() else {
                return;
            };
            let done_line = if *recheck {
                "Backup checked"
            } else {
                "Words 1-32 checked"
            };
            let PendingQuiz { items, seed } = *pending;
            let mut quiz = Box::new(CheckBackupScreen::for_words(items, done_line, seed));
            quiz.set_constraints(screen);
            *widget = Some(quiz);
        }
    }

    /// R1: the entry of words 1-32, from word 1 or at the review of `review`.
    #[inline(never)]
    pub(crate) fn build_card_restore_entry(
        phase: Box<CardRestorePhase>,
        review: Option<&CardHalfEntries>,
    ) -> Self {
        let mut widget = Box::new(CardRestoreEntry::new());
        if let Some(entries) = review {
            widget.review(entries.0);
        }
        Self::CardRestoreEntry {
            widget,
            phase: Some(phase),
        }
    }

    /// Core shows `Preparing` for the seal before word 1, the encode, and the proofs (the
    /// PROVING screen, owner B2). An opening runs behind the gate, never under `Preparing`.
    fn card_busy_copy(task: CardTask) -> (&'static str, &'static str) {
        match task {
            CardTask::Seal => BUSY_PREPARING_CARD,
            CardTask::Proofs => BUSY_PROVING,
            CardTask::Opening => (COPY_GATE_PREPARING.title, COPY_GATE_PREPARING.body),
            CardTask::Card => BUSY_OPENING_CARD,
        }
    }

    /// Two busy tasks show the same copy, so one can replace the other in place.
    pub(crate) fn same_busy_copy(shown: CardTask, task: CardTask) -> bool {
        Self::card_busy_copy(shown) == Self::card_busy_copy(task)
    }

    /// The screen core asked for. `round_undos` are the undos left on the last answer screen,
    /// by word: a second look at the same word built fresh (another screen came between)
    /// keeps them.
    #[inline(never)]
    pub(crate) fn build_card_screen(
        screen: &CardScreen,
        round_undos: Option<(u8, u8)>,
    ) -> Option<Self> {
        use CardPageDone::{Action, Exit};
        use CardUserAction as A;
        let (copy, on_done) = match screen {
            CardScreen::Preparing { task } => {
                let (title, status) = Self::card_busy_copy(*task);
                return Some(Self::CardPreparing {
                    widget: Box::new(CardBusyScreen::new(title, status)),
                    task: *task,
                });
            }
            CardScreen::RestoreChecking => return Some(Self::build_card_restore_checking()),
            CardScreen::DeviceHalf { words } => {
                return Some(Self::build_card_device_half(&words.0))
            }
            CardScreen::SelfCheck { items, seed } => {
                return Some(Self::build_card_self_check(&items.0, *seed, false))
            }
            CardScreen::Recheck { items, seed } => {
                return Some(Self::build_card_self_check(&items.0, *seed, true))
            }
            CardScreen::RecheckDone => {
                return Some(Self::CardSuccess {
                    widget: Box::new(CardSuccess::new("Backup checked")),
                    on_done: Some(Exit),
                })
            }
            // D8: both checks passed. "Card done" follows by itself.
            CardScreen::Verdict { passed: true } => {
                return Some(Self::CardSuccess {
                    widget: Box::new(CardSuccess::new("Backup checked")),
                    on_done: Some(Action(A::VerdictAck)),
                })
            }
            CardScreen::DeviceRoundAnswer {
                j,
                k,
                options,
                display_id,
            } => {
                let undos = round_undos
                    .filter(|(kept_j, _)| kept_j == j)
                    .map_or(UNDO_CAP_PER_BOX, |(_, undos)| undos);
                let widget =
                    DeviceRoundAnswer::new(*j, *k, options.0, *display_id).with_undos_left(undos);
                return Some(Self::CardDeviceRound(Box::new(widget)));
            }
            CardScreen::Gate { dots, preparing } => {
                return Some(Self::CardGate(Box::new(GateScreen::new(*dots, *preparing))))
            }
            // Owner B12: the device round runs after N, so it has no Stop.
            CardScreen::DeviceRoundIntro => {
                return Some(Self::CardPage {
                    widget: Box::new(COPY_DEVICE_ROUND_INTRO.page()),
                    on_done: Some(Action(A::DeviceRoundIntroDone)),
                })
            }
            CardScreen::DeviceRoundMismatch { j } => {
                return Some(Self::CardPage {
                    widget: Box::new(device_round_mismatch_page(*j)),
                    on_done: Some(Action(A::DeviceRoundMismatchAck)),
                })
            }
            CardScreen::WaitingForPhone => (COPY_WAITING_FOR_PHONE, None),
            CardScreen::PhoneHalfNext => (COPY_PHONE_HALF_NEXT, None),
            CardScreen::WaitingForPhoneHalf => (COPY_WAITING_FOR_PHONE_HALF, None),
            CardScreen::CardFinishedWaiting => (COPY_CARD_FINISHED_WAITING, None),
            CardScreen::DeviceRoundFailed => (COPY_DEVICE_ROUND_FAILED, Some(Action(A::FailAck))),
            CardScreen::DeviceRoundNextStep => (COPY_DEVICE_ROUND_NEXT_STEP, Some(Exit)),
            CardScreen::Verdict { passed: false } | CardScreen::PhoneRoundFailed => {
                (COPY_PHONE_ROUND_FAILED, Some(Action(A::FailAck)))
            }
            CardScreen::Stopped => (COPY_STOPPED, Some(Action(A::FailAck))),
            CardScreen::StoppedNextStep => (COPY_NEXT_STEP_BEFORE_N, Some(Exit)),
            CardScreen::CardDone => (COPY_CARD_DONE, Some(Exit)),
            CardScreen::NextStep => (COPY_NEXT_STEP, Some(Exit)),
            CardScreen::RedisplayRefused => (COPY_REDISPLAY_REFUSED, Some(Exit)),
            CardScreen::Refused { reason } => (Self::refusal_copy(*reason), Some(Exit)),
            CardScreen::DeviceError => (COPY_DEVICE_ERROR, Some(Exit)),
        };
        Some(Self::build_card_page(copy, on_done))
    }

    /// Refusals with no page of their own get the device-error page: they mean the phone and
    /// the device disagree, so the card flow stops here.
    fn refusal_copy(reason: CardRefusal) -> CardCopy {
        match reason {
            CardRefusal::ThresholdOne => COPY_REFUSED_THRESHOLD_ONE,
            CardRefusal::NotKeygenOrigin => COPY_REFUSED_NOT_KEYGEN,
            CardRefusal::NotCardKey => COPY_REFUSED_NOT_CARD_KEY,
            CardRefusal::NotDirectUsb => COPY_REFUSED_NOT_DIRECT_USB,
            CardRefusal::Unsupported => COPY_NEEDS_NEWER_FIRMWARE,
            CardRefusal::RunCap => COPY_REFUSED_RUN_CAP,
            _ => COPY_DEVICE_ERROR,
        }
    }

    #[inline(never)]
    pub(crate) fn build_card_restore_checking() -> Self {
        let (title, status) = BUSY_CHECKING_CARD;
        Self::CardRestoreChecking(Box::new(CardBusyScreen::new(title, status)))
    }

    #[inline(never)]
    pub(crate) fn build_card_restore_report(report: &CardRestoreReport) -> Self {
        use CardPageDone::{Exit, RestoreEdit};
        let (copy, on_done) = match report {
            // cloned, so the caller's copy is wiped where core put it
            CardRestoreReport::Restored { decoded, findings } => {
                return Self::build_card_restore_report_page(
                    RestoreReportPage::KeyRestored,
                    *findings,
                    Box::new(decoded.clone()),
                )
            }
            CardRestoreReport::AlreadyHeld => (COPY_ALREADY_HELD, Exit),
            // "Edit words 1-32": back to the review, with the entries kept
            CardRestoreReport::Failed(RestoreFailure::DontFit) => {
                (COPY_RESTORE_FAILED, RestoreEdit)
            }
            CardRestoreReport::Failed(RestoreFailure::TooManyUnreadable) => {
                (COPY_TOO_MANY_UNREADABLE, RestoreEdit)
            }
            CardRestoreReport::Failed(RestoreFailure::NeedsNewerFirmware) => {
                (COPY_NEEDS_NEWER_FIRMWARE, Exit)
            }
            // owner B11: the words read, so editing them can't help; the restore ends
            CardRestoreReport::Failed(RestoreFailure::WrongWallet) => (COPY_WRONG_WALLET, Exit),
        };
        Self::build_card_page(copy, Some(on_done))
    }

    #[inline(never)]
    pub(crate) fn build_card_restore_report_page(
        page: RestoreReportPage,
        findings: RestoreFindings,
        decoded: Box<CardRestoreDecoded>,
    ) -> Self {
        let wallet_threshold = decoded
            .phase
            .target
            .as_ref()
            .map(|key| key.threshold() as u16);
        Self::CardRestoreReport {
            widget: Box::new(page.page(&findings, wallet_threshold)),
            page,
            findings,
            decoded: Some(decoded),
        }
    }

    /// The consolidation hold on the phase's threshold warning. Without a warning the firmware
    /// finishes the consolidation by itself and never builds this.
    #[inline(never)]
    pub(crate) fn build_consolidate_threshold_hold(
        phase: Box<ConsolidatePhase>,
        warning: ThresholdWarning,
    ) -> Self {
        Self::ConsolidateThresholdHold {
            widget: Box::new(threshold_hold_page(warning.card, warning.wallet)),
            phase: Some(phase),
        }
    }

    /// A card screen is animating (a page sliding, a pick's feedback, a fade, the device
    /// round's flash), so card jobs wait and the animation stays smooth; a device round
    /// pick's frame then goes out at the same time after the tap whatever the job was doing.
    pub(crate) fn is_animating(&self) -> bool {
        match self {
            Self::CardDeviceHalf { widget, .. } => widget.is_sliding(),
            Self::CardSelfCheck {
                widget: Some(widget),
                ..
            } => widget.is_animating(),
            Self::CardRestoreEntry { widget, .. } => widget.is_sliding(),
            Self::CardDeviceRound(widget) => widget.is_animating(),
            _ => false,
        }
    }

    /// A screen of a card session, which a card job's next screen may replace. The restore
    /// screens belong to no session.
    pub(crate) fn is_card_session_screen(&self) -> bool {
        matches!(
            self,
            Self::CardPage { .. }
                | Self::CardPreparing { .. }
                | Self::CardDeviceHalf { .. }
                | Self::CardSelfCheck { .. }
                | Self::CardSuccess { .. }
                | Self::CardDeviceRound(_)
                | Self::CardGate(_)
        )
    }

    /// The event of a card screen, once: what the user did, as the action core takes.
    #[inline(never)]
    pub(crate) fn take_card_event(&mut self) -> Option<UiEvent> {
        match self {
            Self::CardPage { widget, on_done } => {
                if !widget.is_done() {
                    return None;
                }
                // the page is left once
                on_done.take().map(UiEvent::from)
            }
            Self::CardDeviceHalf { widget, fired } => {
                if *fired || !widget.is_finished() {
                    return None;
                }
                *fired = true;
                Some(UiEvent::Card(CardUserAction::DeviceHalfWritten))
            }
            // at the end of the standard success dwell, so "Success!" is seen in full
            Self::CardSelfCheck {
                widget,
                recheck,
                fired,
                ..
            } => {
                if *fired || !widget.as_ref().is_some_and(|widget| widget.is_finished()) {
                    return None;
                }
                *fired = true;
                Some(UiEvent::Card(if *recheck {
                    CardUserAction::RecheckPassed
                } else {
                    CardUserAction::SelfCheckPassed
                }))
            }
            Self::CardSuccess { widget, on_done } => {
                if !widget.is_finished() {
                    return None;
                }
                on_done.take().map(UiEvent::from)
            }
            Self::CardRestoreEntry { widget, phase } => {
                let entries = widget.entries()?;
                let phase = phase.take()?;
                Some(UiEvent::CardRestoreEntered {
                    phase,
                    entries: CardHalfEntries(entries),
                })
            }
            Self::CardDeviceRound(widget) => Some(UiEvent::Card(match widget.take_event()? {
                DeviceRoundEvent::Pick { display_id, pick } => {
                    CardUserAction::DeviceRoundPick { display_id, pick }
                }
            })),
            _ => None,
        }
    }
}

impl Default for WidgetTree {
    fn default() -> Self {
        WidgetTree::Standby(Box::new(Standby::new(crate::FIRMWARE_VERSION)))
    }
}
