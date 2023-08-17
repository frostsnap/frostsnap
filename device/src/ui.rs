use alloc::collections::VecDeque;
use alloc::string::String;

pub trait UserInteraction {
    fn set_downstream_connection_state(&mut self, connected: bool);

    fn set_device_label(&mut self, label: String);

    fn get_device_label(&self) -> Option<&str>;

    fn set_workflow(&mut self, workflow: Workflow);

    fn poll(&mut self) -> Option<UiEvent>;

    /// try not to use this
    fn misc_print(&mut self, string: &str);

    fn display_error(&mut self, message: &str);
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
pub struct DebugLog {
    log: VecDeque<String>,
    max_items: usize,
}

impl DebugLog {
    pub fn new(max_items: usize) -> Self {
        Self {
            log: VecDeque::new(),
            max_items,
        }
    }
    pub fn add(&mut self, str: String) {
        if self.log.len() >= self.max_items {
            self.log.pop_front();
        }
        self.log.push_back(str);
    }

    pub fn to_string(&self) -> String {
        let mut print_msg = String::new();
        for item in &self.log {
            print_msg.push_str(&item.chars().take(20).collect::<String>());
            print_msg.push_str("|");
        }
        print_msg
    }
}

#[derive(Clone, Debug)]
pub enum Workflow {
    None,
    WaitingFor(WaitingFor),
    BusyDoing(BusyTask),
    UserPrompt(Prompt),
    OnScreenDebug(DebugLog),
}

impl Default for Workflow {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub enum Prompt {
    KeyGen(String),
    Signing(String),
}

#[derive(Clone, Copy, Debug)]
pub enum BusyTask {
    KeyGen,
    Signing,
    VerifyingShare,
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    KeyGenConfirm(bool),
    SigningConfirm(bool),
}
