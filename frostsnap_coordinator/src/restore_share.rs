use std::borrow::BorrowMut;

use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{message::CoordinatorToUserMessage, DeviceId};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

pub struct RestoreShareProtocol {
    state: RestoreShareState,
    device_id: DeviceId,
    sink: Box<dyn Sink<RestoreShareState>>,
}

#[derive(Clone, Debug)]
pub struct RestoreShareState {
    pub outcome: Option<String>,
    pub abort: bool,
}

impl RestoreShareProtocol {
    pub fn new(device_id: DeviceId, sink: impl Sink<RestoreShareState> + 'static) -> Self {
        Self {
            state: RestoreShareState {
                outcome: None,
                abort: false,
            },
            device_id,
            sink: Box::new(sink),
        }
    }
}

impl UiProtocol for RestoreShareProtocol {
    fn cancel(&mut self) {
        self.sink.send(RestoreShareState {
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
            self.state = RestoreShareState {
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
        if let CoordinatorToUserMessage::EnteredShareBackup {
            device_id,
            share_index: _share_index,
            outcome,
        } = message
        {
            if self.device_id == device_id {
                self.sink.send(RestoreShareState {
                    outcome: Some(match outcome {
                        frostsnap_core::message::EnteredShareBackupOutcome::DoesntBelongToKey => {
                            "DoesntBelongToKey".to_string()
                        }
                        frostsnap_core::message::EnteredShareBackupOutcome::ValidAtIndex => {
                            "ValidAtIndex".to_string()
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
