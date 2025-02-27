use crate::graphics::{
    animation::AnimationState,
    widgets::{EnterShareIndexScreen, EnterShareScreen},
};
use alloc::string::{String, ToString};
use frostsnap_comms::Sha256Digest;
use frostsnap_core::{schnorr_fun::frost::SecretShare, KeyId, SessionHash};

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState);
    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnectionState);

    fn set_device_name(&mut self, name: String);

    fn get_device_label(&self) -> Option<&str>;

    fn set_workflow(&mut self, workflow: Workflow);

    fn set_busy_task(&mut self, task: BusyTask);

    fn clear_busy_task(&mut self);

    fn clear_workflow(&mut self) {
        self.set_workflow(Workflow::WaitingFor(WaitingFor::CoordinatorInstruction {
            completed_task: None,
        }));
    }

    fn take_workflow(&mut self) -> Workflow;

    fn poll(&mut self) -> Option<UiEvent>;

    fn debug<S: ToString>(&mut self, debug: S) {
        self.set_workflow(Workflow::Debug(debug.to_string()));
    }

    fn cancel(&mut self) {
        let workflow = self.take_workflow();
        let new_workflow = match workflow {
            Workflow::UserPrompt {
                prompt: Prompt::NewName { old_name, new_name },
                ..
            } => Workflow::NamingDevice { old_name, new_name },
            Workflow::NamingDevice { .. }
            | Workflow::DisplayBackup { .. }
            | Workflow::UserPrompt { .. }
            | Workflow::DisplayAddress { .. }
            | Workflow::EnteringBackup { .. }
            | Workflow::FirmwareUpgrade(_)
            | Workflow::WaitingFor(_) => Workflow::WaitingFor(WaitingFor::CoordinatorInstruction {
                completed_task: None,
            }),
            Workflow::None | Workflow::Debug(_) => workflow,
        };
        self.set_workflow(new_workflow);
    }
}

const HOLD_TO_CONFIRM_TIME_MS: crate::Duration = crate::Duration::millis(600);
const LONG_HOLD_TO_CONFIRM_TIME_MS: crate::Duration = crate::Duration::millis(6000);

#[derive(Clone, Debug)]
pub enum WaitingFor {
    /// Looking for upstream device
    LookingForUpstream { jtag: bool },
    /// Waiting for the announce ack
    CoordinatorAnnounceAck,
    /// Waiting to be told to do something
    CoordinatorInstruction { completed_task: Option<UiEvent> },
    /// Waiting for the coordinator to respond to a message its sent
    CoordinatorResponse(WaitingResponse),
}

#[derive(Clone, Debug)]
pub enum WaitingResponse {
    KeyGen,
}

#[derive(Debug)]
pub enum Workflow {
    None,
    WaitingFor(WaitingFor),
    UserPrompt {
        prompt: Prompt,
        animation: AnimationState,
        confirm_emitted: bool,
    },
    Debug(String),
    NamingDevice {
        old_name: Option<String>,
        new_name: String,
    },
    DisplayBackup {
        key_name: String,
        backup: String,
    },
    EnteringBackup(EnteringBackupStage),
    DisplayAddress {
        address: String,
        bip32_path: String,
        rand_seed: u32,
    },
    FirmwareUpgrade(FirmwareUpgradeStatus),
}

impl Workflow {
    pub fn prompt(prompt: Prompt) -> Self {
        let hold_duration = match prompt {
            Prompt::WipeDevice => LONG_HOLD_TO_CONFIRM_TIME_MS,
            _ => HOLD_TO_CONFIRM_TIME_MS,
        };
        Self::UserPrompt {
            prompt,
            animation: AnimationState::new(hold_duration),
            confirm_emitted: false,
        }
    }
}

#[derive(Debug)]
pub enum EnteringBackupStage {
    //HACK So the creator of the workflow doesn't have to construct the screen
    Init,
    ShareIndex { screen: EnterShareIndexScreen },
    Share { screen: EnterShareScreen },
}

impl Default for Workflow {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen {
        session_hash: SessionHash,
        key_name: String,
        t_of_n: (u16, u16),
    },
    Signing(SignPrompt),
    NewName {
        old_name: Option<String>,
        new_name: String,
    },
    DisplayBackupRequest {
        key_name: String,
        key_id: KeyId,
    },
    ConfirmFirmwareUpgrade {
        firmware_digest: Sha256Digest,
        size: u32,
    },
    ConfirmLoadBackup(SecretShare),
    WipeDevice,
}

#[derive(Clone, Debug)]
pub enum SignPrompt {
    Bitcoin {
        fee: bitcoin::Amount,
        foreign_recipients: alloc::vec::Vec<(bitcoin::Address, bitcoin::Amount)>,
    },
    Plain(String),
    Nostr(String),
}

#[derive(Clone, Copy, Debug)]
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
    KeyGenConfirm,
    SigningConfirm,
    NameConfirm(String),
    EnteredShareBackup(SecretShare),
    EnteredShareBackupConfirm(SecretShare),
    BackupRequestConfirm,
    UpgradeConfirm,
    WipeDataConfirm,
}
