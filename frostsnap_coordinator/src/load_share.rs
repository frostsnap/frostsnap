use std::borrow::BorrowMut;

use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    coordinator::FrostCoordinator,
    message::{CoordinatorSend, CoordinatorToUserMessage},
    DeviceId, KeyId,
};
use tracing::{event, Level};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

pub struct LoadShareProtocol {
    state: LoadShareState,
    device_id: DeviceId,
    load_share_messages: Vec<CoordinatorSendMessage>,
    sink: Box<dyn Sink<LoadShareState>>,
}

#[derive(Clone, Debug)]
pub struct LoadShareState {
    pub outcome: Option<String>,
    pub abort: bool,
}

impl LoadShareProtocol {
    pub fn new(
        coordinator: &mut FrostCoordinator,
        device_id: DeviceId,
        key_id: KeyId,
        sink: impl Sink<LoadShareState> + 'static,
    ) -> Self {
        let mut self_ = Self {
            state: LoadShareState {
                outcome: None,
                abort: false,
            },
            device_id,
            sink: Box::new(sink),
            load_share_messages: vec![],
        };

        match coordinator.restore_share(device_id, key_id) {
            Ok(messages) => {
                for message in messages {
                    match message {
                        CoordinatorSend::ToDevice {
                            message,
                            destinations,
                        } => {
                            let load_share_message = CoordinatorSendMessage {
                                target_destinations: Destination::Particular(destinations),
                                message_body: CoordinatorSendBody::Core(message),
                            };
                            self_.load_share_messages.push(load_share_message);
                        }
                        CoordinatorSend::ToUser(_) => todo!("handle these if they ever exist"),
                        CoordinatorSend::SigningSessionStore(_) => unreachable!("not signing"),
                    }
                }
            }
            Err(e) => self_.abort(format!("couldn't start restoring share: {e}")),
        }
        self_
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }

    fn abort(&mut self, reason: String) {
        self.state.abort = true;
        self.state.outcome = Some(reason);
        self.emit_state();
    }
}

impl UiProtocol for LoadShareProtocol {
    fn cancel(&mut self) {
        self.abort("loading share cancelled".to_string())
    }

    fn is_complete(&self) -> Option<Completion> {
        if !self.state.abort && self.state.outcome.is_some() {
            Some(Completion::Success)
        } else if self.state.abort {
            Some(Completion::Abort {
                send_cancel_to_all_devices: false,
            })
        } else {
            None
        }
    }

    fn connected(&mut self, _id: frostsnap_core::DeviceId) {}

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.device_id == id {
            event!(
                Level::ERROR,
                id = id.to_string(),
                "Device disconnected during loading share"
            );
            self.abort("Loading share failed because a device was disconnected".into());
            self.emit_state();
        }
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        if let CoordinatorToUserMessage::EnteredBackup { device_id, outcome } = message {
            if self.device_id == device_id {
                let outcome = match outcome {
                    frostsnap_core::message::EnteredBackupOutcome::DoesntBelongToKey => {
                        "Share backup does not belong to this key.".to_string()
                    }
                    frostsnap_core::message::EnteredBackupOutcome::ValidAtIndex => {
                        "Share backup is valid and belongs to this key.".to_string()
                    }
                };
                self.state.outcome = Some(outcome);
                self.emit_state();
            }
        } else {
            event!(
                Level::ERROR,
                "Non load share message sent during loading of share"
            );
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        (core::mem::take(&mut self.load_share_messages), vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.borrow_mut()
    }
}
