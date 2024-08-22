use std::borrow::BorrowMut;

use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{message::CoordinatorToUserMessage, DeviceId};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

pub struct LoadShareProtocol {
    state: LoadShareState,
    device_id: DeviceId,
    sink: Box<dyn Sink<LoadShareState>>,
}

#[derive(Clone, Debug)]
pub struct LoadShareState {
    pub outcome: Option<String>,
    pub abort: bool,
}

impl LoadShareProtocol {
    pub fn new(device_id: DeviceId, sink: impl Sink<LoadShareState> + 'static) -> Self {
        Self {
            state: LoadShareState {
                outcome: None,
                abort: false,
            },
            device_id,
            sink: Box::new(sink),
        }
    }
}

impl UiProtocol for LoadShareProtocol {
    fn cancel(&mut self) {
        self.sink.send(LoadShareState {
            outcome: None,
            abort: true,
        })
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.state.outcome.is_some() {
            Some(Completion::Success)
        } else {
            None
        }
    }

    fn connected(&mut self, _id: frostsnap_core::DeviceId) {}

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.device_id == id {
            self.state = LoadShareState {
                outcome: None,
                abort: true,
            };
        }
        self.sink.send(self.state.clone())
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        if let CoordinatorToUserMessage::EnteredBackup { device_id, outcome } = message {
            if self.device_id == device_id {
                self.sink.send(LoadShareState {
                    outcome: Some(match outcome {
                        frostsnap_core::message::EnteredBackupOutcome::DoesntBelongToKey => {
                            "Share backup does not belong to this key.".to_string()
                        }
                        frostsnap_core::message::EnteredBackupOutcome::ValidAtIndex => {
                            "Share backup is valid and belongs to this key.".to_string()
                        }
                    }),
                    abort: false,
                });
            }
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        let to_devices = vec![];
        let to_storage = vec![];
        (to_devices, to_storage)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.borrow_mut()
    }
}
