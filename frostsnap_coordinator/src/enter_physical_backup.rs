use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{CoordinatorToUserMessage, PhysicalBackupPhase},
    message::CoordinatorToDeviceMessage,
    DeviceId, Gist, RestorationId,
};
use std::collections::BTreeSet;
use tracing::{event, Level};

use crate::{Sink, UiProtocol};

pub struct EnterPhysicalBackup {
    sink: Box<dyn Sink<EnterPhysicalBackupState>>,
    blank_connected: BTreeSet<DeviceId>,
    chosen_device: Option<DeviceId>,
    restoration_id: RestorationId,
    sent_req: bool,
    finished: Option<PhysicalBackupPhase>,
    abort: bool,
}

impl EnterPhysicalBackup {
    pub fn new(sink: impl Sink<EnterPhysicalBackupState>, restoration_id: RestorationId) -> Self {
        Self {
            sink: Box::new(sink),
            restoration_id,
            blank_connected: Default::default(),
            chosen_device: None,
            sent_req: false,
            finished: None,
            abort: false,
        }
    }

    pub fn choose_device_to_enter_backup(&mut self, device_id: DeviceId) {
        if self.blank_connected.contains(&device_id) {
            self.chosen_device = Some(device_id);
        }
        self.emit_state();
    }

    pub fn emit_state(&self) {
        self.sink.send(EnterPhysicalBackupState {
            restoration_id: self.restoration_id,
            blank_connected: self.blank_connected.iter().copied().collect(),
            chosen: self.chosen_device,
            finished: self.finished.is_some(),
        })
    }
}

impl UiProtocol for EnterPhysicalBackup {
    fn cancel(&mut self) {
        self.abort = true;
    }

    fn is_complete(&self) -> Option<crate::Completion> {
        if self.finished.is_some() {
            Some(crate::Completion::Success)
        } else if self.abort {
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        let mut changed = false;
        if self.chosen_device == Some(id) {
            self.chosen_device = None;
            self.sent_req = false;
            changed = true;
        }
        changed |= self.blank_connected.remove(&id);
        if changed {
            self.emit_state();
        }
    }

    fn poll(&mut self) -> Vec<frostsnap_comms::CoordinatorSendMessage> {
        if !self.sent_req {
            if let Some(chosen) = self.chosen_device {
                self.sent_req = true;
                return vec![CoordinatorSendMessage::to(
                    chosen,
                    CoordinatorSendBody::Core(CoordinatorToDeviceMessage::LoadPhysicalBackup {
                        restoration_id: self.restoration_id,
                    }),
                )];
            }
        }
        vec![]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn connected(&mut self, id: DeviceId, is_blank: bool) {
        if is_blank && self.blank_connected.insert(id) {
            self.emit_state();
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        match message {
            CoordinatorToUserMessage::PromptRecoverPhysicalBackup(physical_backup_phase) => {
                self.finished = Some(*physical_backup_phase);
                self.emit_state();
            }
            message => event!(
                Level::WARN,
                gist = message.gist(),
                "unexpected message during entering backup"
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnterPhysicalBackupState {
    pub restoration_id: RestorationId,
    pub blank_connected: Vec<DeviceId>,
    pub chosen: Option<DeviceId>,
    pub finished: bool,
}
