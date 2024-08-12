use alloc::string::{String, ToString};
use frostsnap_comms::FirmwareDigest;
use frostsnap_core::{KeyId, SessionHash};

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, state: crate::DownstreamConnectionState);
    fn set_upstream_connection_state(&mut self, state: crate::UpstreamConnection);

    fn set_device_name(&mut self, name: String);

    fn get_device_label(&self) -> Option<&str>;

    fn set_workflow(&mut self, workflow: Workflow);

    /// Use this when you want to redraw the UI right now
    fn set_busy_task(&mut self, task: BusyTask) {
        self.set_workflow(Workflow::BusyDoing(task));
        assert!(self.poll().is_none(), "busy tasks cannot have ui events");
    }

    fn take_workflow(&mut self) -> Workflow;

    fn poll(&mut self) -> Option<UiEvent>;

    fn debug<S: ToString>(&mut self, debug: S) {
        self.set_workflow(Workflow::Debug(debug.to_string()));
    }

    fn cancel(&mut self) {
        let workflow = self.take_workflow();
        let new_workflow = match workflow {
            Workflow::UserPrompt(Prompt::NewName { old_name, new_name }) => {
                Workflow::NamingDevice { old_name, new_name }
            }
            Workflow::NamingDevice { .. }
            | Workflow::DisplayBackup { .. }
            | Workflow::UserPrompt(_)
            | Workflow::BusyDoing(_)
            | Workflow::WaitingFor(_) => Workflow::WaitingFor(WaitingFor::CoordinatorInstruction {
                completed_task: None,
            }),
            Workflow::None | Workflow::Debug(_) => workflow,
        };
        self.set_workflow(new_workflow);
    }
}

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

#[derive(Clone, Debug)]
pub enum Workflow {
    None,
    WaitingFor(WaitingFor),
    BusyDoing(BusyTask),
    UserPrompt(Prompt),
    Debug(String),
    NamingDevice {
        old_name: Option<String>,
        new_name: String,
    },
    DisplayBackup {
        backup: String,
    },
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
        key_id: KeyId,
    },
    Signing(SignPrompt),
    NewName {
        old_name: Option<String>,
        new_name: String,
    },
    DisplayBackupRequest((String, KeyId)),
    ConfirmFirmwareUpgrade {
        firmware_digest: FirmwareDigest,
        size: u32,
    },
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
    FirmwareUpgrade(FirmwareUpgradeStatus),
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
        key_name: String,
        key_id: KeyId,
    },
    SigningConfirm,
    NameConfirm(String),
    BackupRequestConfirm(KeyId),
    UpgradeConfirm {
        size: u32,
        firmware_digest: FirmwareDigest,
    },
}
