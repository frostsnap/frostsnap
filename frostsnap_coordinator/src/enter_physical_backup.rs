use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{restoration, CoordinatorToUserMessage},
    message::{CoordinatorRestoration, CoordinatorToDeviceMessage},
    DeviceId, EnterPhysicalId,
};

use crate::{DeviceMode, Sink, UiProtocol};

pub struct EnterPhysicalBackup {
    sink: Box<dyn Sink<EnterPhysicalBackupState>>,
    enter_physical_id: EnterPhysicalId,
    chosen_device: DeviceId,
    sent_req: bool,
    connected: bool,
    entered: Option<restoration::PhysicalBackupPhase>,
    saved: bool,
    abort: Option<String>,
}

impl EnterPhysicalBackup {
    pub fn new(sink: impl Sink<EnterPhysicalBackupState>, chosen_device: DeviceId) -> Self {
        let enter_physical_id = EnterPhysicalId::new(&mut rand::thread_rng());
        Self {
            sink: Box::new(sink),
            enter_physical_id,
            chosen_device,
            sent_req: false,
            connected: false,
            entered: None,
            saved: false,
            abort: None,
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(EnterPhysicalBackupState {
            device_id: self.chosen_device,
            entered: self.entered,
            saved: self.saved,
            abort: self.abort.clone(),
        })
    }
}

impl UiProtocol for EnterPhysicalBackup {
    fn cancel(&mut self) {
        self.abort = Some("entering backup canceled".into());
    }

    fn is_complete(&self) -> Option<crate::Completion> {
        if self.saved {
            Some(crate::Completion::Success)
        } else if self.abort.is_some() {
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        if id == self.chosen_device {
            self.connected = false;
            self.abort = Some("device was unplugged".into());
        }
        self.emit_state();
    }

    fn poll(&mut self) -> Vec<frostsnap_comms::CoordinatorSendMessage> {
        if !self.sent_req && self.connected {
            self.sent_req = true;
            return vec![CoordinatorSendMessage::to(
                self.chosen_device,
                CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                    CoordinatorRestoration::EnterPhysicalBackup {
                        enter_physical_id: self.enter_physical_id,
                    },
                )),
            )];
        }

        vec![]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn connected(&mut self, id: DeviceId, _state: DeviceMode) {
        if id == self.chosen_device {
            self.connected = true;
        }
        self.emit_state();
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        match message {
            CoordinatorToUserMessage::Restoration(
                restoration::ToUserRestoration::PhysicalBackupEntered(physical_backup_phase),
            ) if physical_backup_phase.from == self.chosen_device
                && physical_backup_phase.backup.enter_physical_id == self.enter_physical_id =>
            {
                self.entered = Some(*physical_backup_phase);
                self.emit_state();
                true
            }
            CoordinatorToUserMessage::Restoration(
                restoration::ToUserRestoration::PhysicalBackupSaved { device_id, .. },
            ) if device_id == self.chosen_device => {
                self.saved = true;
                self.emit_state();
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnterPhysicalBackupState {
    pub device_id: DeviceId,
    pub entered: Option<restoration::PhysicalBackupPhase>,
    pub saved: bool,
    pub abort: Option<String>,
}
