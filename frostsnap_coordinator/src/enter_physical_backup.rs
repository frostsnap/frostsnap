use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{restoration, CoordinatorToUserMessage},
    message::{CoordinatorRestoration, CoordinatorToDeviceMessage},
    DeviceId, Gist, RestorationId,
};
use tracing::{event, Level};

use crate::{DeviceMode, Sink, UiProtocol};

pub struct EnterPhysicalBackup {
    sink: Box<dyn Sink<EnterPhysicalBackupState>>,
    chosen_device: DeviceId,
    restoration_id: RestorationId,
    sent_req: bool,
    connected: bool,
    entered: Option<restoration::PhysicalBackupPhase>,
    saved: Option<bool>,
    sent_save: bool,
    abort: Option<String>,
}

impl EnterPhysicalBackup {
    pub fn new(
        sink: impl Sink<EnterPhysicalBackupState>,
        restoration_id: RestorationId,
        chosen_device: DeviceId,
        should_save: bool,
    ) -> Self {
        Self {
            sink: Box::new(sink),
            restoration_id,
            chosen_device,
            sent_req: false,
            connected: false,
            entered: None,
            saved: if should_save { Some(false) } else { None },
            sent_save: false,
            abort: None,
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(EnterPhysicalBackupState {
            restoration_id: self.restoration_id,
            device_id: self.chosen_device,
            entered: self.entered.clone(),
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
        if self.entered.is_some() && self.saved != Some(false) {
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
                    CoordinatorRestoration::Load {
                        restoration_id: self.restoration_id,
                    },
                )),
            )];
        }

        if !self.sent_save && self.connected && self.saved == Some(false) && self.entered.is_some()
        {
            self.sent_save = true;

            return vec![CoordinatorSendMessage::to(
                self.chosen_device,
                CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                    CoordinatorRestoration::Save {
                        restoration_id: self.restoration_id,
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

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        match message {
            CoordinatorToUserMessage::Restoration(
                restoration::ToUserRestoration::PhysicalBackupEntered(physical_backup_phase),
            ) if physical_backup_phase.from == self.chosen_device
                && physical_backup_phase.backup.restoration_id == self.restoration_id =>
            {
                self.entered = Some(*physical_backup_phase);
                self.emit_state();
            }
            CoordinatorToUserMessage::Restoration(
                restoration::ToUserRestoration::PhysicalBackupSaved {
                    device_id,
                    restoration_id,
                    ..
                },
            ) if device_id == self.chosen_device && restoration_id == self.restoration_id => {
                self.saved = Some(true);
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
    pub device_id: DeviceId,
    pub entered: Option<restoration::PhysicalBackupPhase>,
    /// null if the user is entering the backup not to save but to check it
    pub saved: Option<bool>,
    pub abort: Option<String>,
}
