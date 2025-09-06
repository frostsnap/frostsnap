// Imports removed - legacy screens are not used in stateless Workflow
use alloc::{boxed::Box, string::String};
use frost_backup::ShareBackup;
use frostsnap_comms::Sha256Digest;
use frostsnap_core::{
    device::{
        restoration::{BackupDisplayPhase, EnterBackupPhase},
        KeyGenPhase3, SignPhase1,
    },
    message::HeldShare,
};

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState);
    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnectionState);
    fn set_workflow(&mut self, workflow: Workflow);
    fn set_busy_task(&mut self, task: BusyTask);
    fn clear_busy_task(&mut self);
    fn poll(&mut self) -> Option<UiEvent>;
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

    fn set_busy_task(&mut self, task: BusyTask) {
        (**self).set_busy_task(task)
    }

    fn clear_busy_task(&mut self) {
        (**self).clear_busy_task()
    }

    fn poll(&mut self) -> Option<UiEvent> {
        (**self).poll()
    }
}

#[derive(Debug)]
pub enum Workflow {
    None,
    Standby {
        device_name: String,
        held_share: HeldShare,
    },
    UserPrompt(Prompt),
    NamingDevice {
        new_name: String,
    },
    DisplayBackup {
        key_name: String,
        backup: ShareBackup,
    },
    EnteringBackup(EnterBackupPhase),
    DisplayAddress {
        address: bitcoin::Address,
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

impl Default for Workflow {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen {
        phase: Box<KeyGenPhase3>,
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
        phase: Box<KeyGenPhase3>,
    },
    SigningConfirm {
        phase: Box<SignPhase1>,
    },
    NameConfirm(String),
    EnteredShareBackup {
        phase: EnterBackupPhase,
        share_backup: ShareBackup,
    },
    BackupRequestConfirm {
        phase: Box<BackupDisplayPhase>,
    },
    UpgradeConfirm,
    WipeDataConfirm,
}
