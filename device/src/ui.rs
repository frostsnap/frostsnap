// Imports removed - legacy screens are not used in stateless Workflow
use alloc::{boxed::Box, string::String};
use frost_backup::ShareBackup;
use frostsnap_comms::{DeviceName, Sha256Digest};
use frostsnap_core::{
    device::{
        restoration::{BackupDisplayPhase, EnterBackupPhase},
        KeyGenPhase3, SignPhase1,
    },
    message::HeldShare2,
    tweak::BitcoinBip32Path,
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

#[derive(Debug, Default)]
pub enum Workflow {
    #[default]
    Startup,
    None,
    Standby {
        device_name: DeviceName,
        held_share: HeldShare2,
    },
    UserPrompt(Prompt),
    NamingDevice {
        new_name: DeviceName,
    },
    DisplayBackup {
        key_name: String,
        backup: ShareBackup,
    },
    EnteringBackup(EnterBackupPhase),
    DisplayAddress {
        address: bitcoin::Address,
        bip32_path: BitcoinBip32Path,
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

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen {
        phase: Box<KeyGenPhase3>,
    },
    Signing {
        phase: Box<SignPhase1>,
    },
    NewName {
        old_name: Option<DeviceName>,
        new_name: DeviceName,
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
    NameConfirm(frostsnap_comms::DeviceName),
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
