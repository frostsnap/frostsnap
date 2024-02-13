use alloc::string::String;
use frostsnap_core::{KeyId, SessionHash};

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, state: crate::ConnectionState);

    fn set_device_name(&mut self, name: String);

    fn get_device_label(&self) -> Option<&str>;

    fn set_workflow(&mut self, workflow: Workflow);
    fn take_workflow(&mut self) -> Workflow;

    fn poll(&mut self) -> Option<UiEvent>;

    fn cancel(&mut self) {
        let workflow = self.take_workflow();
        let new_workflow = match workflow {
            Workflow::UserPrompt(Prompt::NewName { old_name, new_name }) => {
                Workflow::NamingDevice { old_name, new_name }
            }
            Workflow::NamingDevice { .. }
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
    /// Waitinf for the announce ack
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
}

impl Default for Workflow {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen(SessionHash),
    Signing(String),
    NewName {
        old_name: Option<String>,
        new_name: String,
    },
    DisplayBackupRequest(KeyId),
    DisplayBackup(String),
}

#[derive(Clone, Copy, Debug)]
pub enum BusyTask {
    KeyGen,
    Signing,
    VerifyingShare,
    Loading,
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    KeyGenConfirm,
    SigningConfirm,
    NameConfirm(String),
    BackupRequestConfirm(KeyId),
    BackupConfirm(String),
}
