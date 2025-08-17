// Imports removed - legacy screens are not used in stateless Workflow
use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use frostsnap_comms::Sha256Digest;
use frostsnap_core::{
    device::{
        restoration::{BackupDisplayPhase, EnterBackupPhase},
        KeyGenPhase2, SignPhase1,
    },
    schnorr_fun::frost::SecretShare,
};

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState);
    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnectionState);

    fn set_workflow(&mut self, workflow: Workflow);

    fn set_busy_task(&mut self, task: BusyTask);

    fn set_recovery_mode(&mut self, value: bool);

    fn clear_busy_task(&mut self);

    fn clear_workflow(&mut self) {}

    fn take_workflow(&mut self) -> Workflow;

    fn poll(&mut self) -> Option<UiEvent>;

    fn debug<S: ToString>(&mut self, _debug: S) {
        // Debug functionality is now handled by RootWidget's debug_text field
        // This is a no-op for now, but implementations can use this to set debug text
    }

    fn cancel(&mut self) {}
}

// These will be used when implementing HoldToConfirm widgets
#[allow(dead_code)]
const HOLD_TO_CONFIRM_TIME_MS: crate::Duration = crate::Duration::millis(600);
#[allow(dead_code)]
const LONG_HOLD_TO_CONFIRM_TIME_MS: crate::Duration = crate::Duration::millis(6000);

#[derive(Debug)]
pub enum Workflow {
    None,
    Standby {
        name: String,
        key_name: String,
    },
    UserPrompt(Prompt),
    NamingDevice {
        new_name: String,
    },
    DisplayBackup {
        key_name: String,
        backup: String,
    },
    EnteringBackup(EnterBackupPhase),
    DisplayAddress {
        address: String,
        bip32_path: String,
        rand_seed: u32,
    },
    FirmwareUpgrade(FirmwareUpgradeStatus),
}

impl Workflow {
    #[must_use]
    pub fn prompt(prompt: Prompt) -> Self {
        Self::UserPrompt(prompt)
    }
}

// No longer needed - backup entry stages are handled directly in WidgetTree
// #[derive(Debug)]
// pub enum EnteringBackupStage {
//     //HACK So the creator of the workflow doesn't have to construct the screen
//     Init {
//         phase: EnterBackupPhase,
//     },
//     ShareIndex {
//         phase: EnterBackupPhase,
//         screen: EnterShareIndexScreen,
//     },
//     Share {
//         phase: EnterBackupPhase,
//         screen: EnterShareScreen,
//     },
// }

impl Default for Workflow {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen {
        phase: Box<KeyGenPhase2>,
    },
    Signing {
        phase: Box<SignPhase1>,
    },
    NewName {
        old_name: Option<String>,
        new_name: String,
    },
    DisplayBackupRequest {
        phase: Box<BackupDisplayPhase>,
    },
    ConfirmFirmwareUpgrade {
        firmware_digest: Sha256Digest,
        size: u32,
    },
    ConfirmEnterBackup {
        share_backup: SecretShare,
        phase: EnterBackupPhase,
    },
    WipeDevice,
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
    Erase { progress: f32 },
    Download { progress: f32 },
    Passive,
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    KeyGenConfirm {
        phase: Box<KeyGenPhase2>,
    },
    SigningConfirm {
        phase: Box<SignPhase1>,
    },
    NameConfirm(String),
    EnteredShareBackup {
        phase: EnterBackupPhase,
        share_backup: SecretShare,
    },
    BackupRequestConfirm {
        phase: Box<BackupDisplayPhase>,
    },
    UpgradeConfirm,
    WipeDataConfirm,
}
