use std::borrow::BorrowMut;

use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{AccessStructureRef, FrostCoordinator},
    message::CoordinatorToUserMessage,
    DeviceId, SymmetricKey,
};
use tracing::{event, Level};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

pub struct CheckShareProtocol {
    state: CheckShareState,
    device_id: DeviceId,
    check_share_messages: Vec<CoordinatorSendMessage>,
    sink: Box<dyn Sink<CheckShareState>>,
}

#[derive(Clone, Debug)]
pub struct CheckShareState {
    pub outcome: Option<bool>,
    pub abort: Option<String>,
}

impl CheckShareProtocol {
    pub fn new(
        coordinator: &mut FrostCoordinator,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        sink: impl Sink<CheckShareState> + 'static,
        encryption_key: SymmetricKey,
    ) -> Self {
        let mut self_ = Self {
            state: CheckShareState {
                outcome: None,
                abort: None,
            },
            device_id,
            sink: Box::new(sink),
            check_share_messages: vec![],
        };

        match coordinator.check_share(access_structure_ref, device_id, encryption_key) {
            Ok(messages) => {
                for message in messages {
                    self_.check_share_messages.push(
                        message
                            .try_into()
                            .expect("will only send messages to devices"),
                    );
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
        self.state.abort = Some(reason);
        self.emit_state();
    }
}

impl UiProtocol for CheckShareProtocol {
    fn cancel(&mut self) {
        self.abort("loading share cancelled".to_string())
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.state.abort.is_none() && self.state.outcome.is_some() {
            Some(Completion::Success)
        } else if self.state.abort.is_some() {
            Some(Completion::Abort {
                // NOTE: It would be better if we only send the cancel to a particular device but it
                // doesn't really matter
                send_cancel_to_all_devices: true,
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
            self.abort("Checking share failed because a device was disconnected".into());
        }
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        if let CoordinatorToUserMessage::EnteredBackup { device_id, valid } = message {
            if self.device_id == device_id {
                self.state.outcome = Some(valid);
                self.emit_state();
            }
        } else {
            event!(
                Level::ERROR,
                "Non check share message sent during CheckShareProtocol"
            );
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        (core::mem::take(&mut self.check_share_messages), vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.borrow_mut()
    }
}
