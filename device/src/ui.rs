// Imports removed - legacy screens are not used in stateless Workflow
use alloc::{boxed::Box, string::String};
use frost_backup::ShareBackup;
use frostsnap_comms::{DeviceName, Sha256Digest};
use frostsnap_core::{
    device::{
        card::{CardScreen, CardUserAction, HomeLine},
        card_restore::{CardRestoreDecoded, CardRestorePhase, CardRestoreReport},
        restoration::{ConsolidatePhase, EnterBackupPhase, ThresholdWarning},
        KeyGenPhase3, SignPhase1,
    },
    message::{card::CardHalfEntries, HeldShare2},
    schnorr_fun::frost::ShareIndex,
    tweak::BitcoinBip32Path,
    AccessStructureRef,
};
use frostsnap_widgets::Frac;

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState);
    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnectionState);
    fn set_workflow(&mut self, workflow: Workflow);
    fn set_default_workflow(&mut self, workflow: Workflow);
    fn go_to_default(&mut self);
    fn set_busy_task(&mut self, task: BusyTask);
    fn clear_busy_task(&mut self);
    fn poll(&mut self) -> Option<UiEvent>;
    fn force_redraw(&mut self);
}

// Implement UserInteraction for Box<T> where T implements UserInteraction
impl<T: UserInteraction + ?Sized> UserInteraction for Box<T> {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState) {
        (**self).set_downstream_connection_state(state)
    }

    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnectionState) {
        (**self).set_upstream_connection_state(state)
    }

    fn set_workflow(&mut self, workflow: Workflow) {
        (**self).set_workflow(workflow)
    }

    fn set_default_workflow(&mut self, workflow: Workflow) {
        (**self).set_default_workflow(workflow)
    }

    fn go_to_default(&mut self) {
        (**self).go_to_default()
    }

    fn set_busy_task(&mut self, task: BusyTask) {
        (**self).set_busy_task(task)
    }

    fn clear_busy_task(&mut self) {
        (**self).clear_busy_task()
    }

    fn poll(&mut self) -> Option<UiEvent> {
        (**self).poll()
    }

    fn force_redraw(&mut self) {
        (**self).force_redraw()
    }
}

#[derive(Debug, Default, Clone)]
pub enum Workflow {
    #[default]
    Startup,
    None,
    Standby {
        device_name: DeviceName,
        held_share: HeldShare2,
        /// "Backup not checked" or "Backup failed: don't use" under the key name (design §7.5
        /// D21).
        card_line: Option<HomeLine>,
    },
    UserPrompt(Prompt),
    NamingDevice {
        new_name: DeviceName,
    },
    DisplayBackup {
        key_name: String,
        backup: ShareBackup,
        access_structure_ref: AccessStructureRef,
    },
    EnteringBackup(EnterBackupPhase),
    DisplayAddress {
        address: bitcoin::Address,
        bip32_path: BitcoinBip32Path,
        rand_seed: u32,
    },
    FirmwareUpgrade(FirmwareUpgradeStatus),
    EraseProgress {
        progress: Frac,
    },
    CheckBackup {
        key_name: String,
        backup: ShareBackup,
        access_structure_ref: AccessStructureRef,
        rand_seed: u32,
    },
    /// A screen of a card session, or a restore's wait (design §4.1).
    Card(CardScreen),
    /// R1: the entry of words 1-32 for a card restore, from word 1, or at the review with
    /// the entries kept ("Edit words 1-32" after a failed decode).
    CardRestoreEntry {
        phase: Box<CardRestorePhase>,
        review: Option<CardHalfEntries>,
    },
    CardRestoreReport(Box<CardRestoreReport>),
}

impl Workflow {
    #[must_use]
    pub fn prompt(prompt: Prompt) -> Self {
        Self::UserPrompt(prompt)
    }
}

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen {
        phase: Box<KeyGenPhase3>,
    },
    Signing {
        phase: Box<SignPhase1>,
        rand_seed: u32,
    },
    ConfirmFirmwareUpgrade {
        firmware_digest: Sha256Digest,
        size: u32,
    },
    EraseDevice,
    /// Consolidating a card share whose threshold isn't the wallet's, or doesn't read: the
    /// user holds to go on (build plan A11). Without a warning it isn't prompted.
    ConsolidateThreshold {
        phase: Box<ConsolidatePhase>,
        warning: ThresholdWarning,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BusyTask {
    KeyGen,
    Signing,
    VerifyingShare,
    Loading,
    GeneratingNonces,
}

#[derive(Clone, Copy, Debug)]
pub enum FirmwareUpgradeStatus {
    Erase { progress: Frac },
    Download { progress: Frac },
    Passive,
    Rejected { reason: crate::ota::RefuseReason },
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    KeyGenConfirm {
        phase: Box<KeyGenPhase3>,
    },
    SigningConfirm {
        phase: Box<SignPhase1>,
    },
    EnteredShareBackup {
        phase: EnterBackupPhase,
        share_backup: ShareBackup,
    },
    BackupRecorded,
    UpgradeConfirm,
    EraseDataConfirm,
    BackupChecked {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    Card(CardUserAction),
    /// Leave a card screen for the home screen. It needs no session, so a terminal screen
    /// can always be left.
    CardExit,
    /// The user held "Done" on the review of words 1-32: `None` for "Can't read this word".
    /// Its `Debug` shows no word.
    CardRestoreEntered {
        phase: Box<CardRestorePhase>,
        entries: CardHalfEntries,
    },
    /// "Edit words 1-32" after a failed decode.
    CardRestoreEdit,
    CardRestoreConfirm(Box<CardRestoreDecoded>),
    /// The user held through the threshold warning: the consolidation finishes.
    ConsolidateConfirm {
        phase: Box<ConsolidatePhase>,
    },
}
