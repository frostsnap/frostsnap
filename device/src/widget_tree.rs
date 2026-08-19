use alloc::boxed::Box;
use bitcoin::Address;
use frost_backup::ShareBackup;
use frostsnap_comms::Sha256Digest;
use frostsnap_core::{
    device::{restoration::EnterBackupPhase, KeyGenPhase3, SignPhase1},
    schnorr_fun::frost::ShareIndex,
    tweak::BitcoinBip32Path,
    AccessStructureRef, SignTask,
};
use frostsnap_widgets::{
    backup::{BackupDisplay, CheckBackupScreen, EnterShareScreen},
    keygen_check::KeygenCheck,
    layout::*,
    sign_prompt::SignTxPrompt,
    AddressWithIndex, DeviceNameScreen, EraseDevice, EraseProgress, FirmwareUpgradeConfirm,
    FirmwareUpgradeProgress, SignMessageConfirm, Standby,
};

use crate::ui::FirmwareUpgradeStatus;

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
}

impl WidgetTree {
    // Keeping widget construction in non-inlined constructors prevents its
    // temporaries from inflating FrostyUi::set_workflow's checked stack frame.
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
                let mut standby = Standby::new();
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

impl Default for WidgetTree {
    fn default() -> Self {
        WidgetTree::Standby(Box::new(Standby::new()))
    }
}
