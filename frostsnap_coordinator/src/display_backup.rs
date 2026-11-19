use crate::{Completion, DeviceMode, Sink, UiProtocol};
use frostsnap_comms::{CommsMisc, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{CoordinatorToUserMessage, FrostCoordinator},
    AccessStructureRef, DeviceId, SymmetricKey,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayBackupState {
    pub confirmed: bool,
    pub close_dialog: bool,
    /// The device refused: the share was restored from a verifiable card, whose words it never
    /// shows again.
    pub refused: bool,
}

pub struct DisplayBackupProtocol {
    device_id: DeviceId,
    abort: bool,
    refused: bool,
    messages: Vec<CoordinatorSendMessage>,
    should_send: bool,
    sink: Box<dyn Sink<DisplayBackupState> + Send>,
}

impl DisplayBackupProtocol {
    pub fn new(
        coord: &mut FrostCoordinator,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        sink: impl Sink<DisplayBackupState> + 'static,
    ) -> anyhow::Result<Self> {
        let messages = coord
            .request_device_display_backup(device_id, access_structure_ref, encryption_key)?
            .into_iter()
            .map(|message| {
                message
                    .try_into()
                    .expect("will only send messages to device")
            })
            .collect();

        Ok(Self {
            device_id,
            sink: Box::new(sink),
            abort: false,
            refused: false,
            messages,
            should_send: true,
        })
    }

    fn abort(&mut self) {
        self.abort = true;
        self.sink.send(DisplayBackupState {
            confirmed: false,
            close_dialog: true,
            refused: false,
        });
    }
}

impl UiProtocol for DisplayBackupProtocol {
    fn cancel(&mut self) {
        self.abort();
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.refused {
            // The device already went back to its home screen.
            Some(Completion::Abort {
                send_cancel_to_all_devices: false,
            })
        } else if self.abort {
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn connected(&mut self, id: frostsnap_core::DeviceId, _state: DeviceMode) {
        if id == self.device_id {
            self.should_send = true;
        }
    }

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.device_id == id {
            self.abort()
        }
    }

    fn process_comms_message(&mut self, from: DeviceId, message: CommsMisc) -> bool {
        if self.device_id != from {
            return false;
        }
        match message {
            CommsMisc::BackupRecorded => {
                self.sink.send(DisplayBackupState {
                    confirmed: true,
                    close_dialog: true,
                    refused: false,
                });
                true
            }
            CommsMisc::DisplayBackupConfrimed => {
                tracing::warn!("Received deprecated DisplayBackupConfrimed message. Device firmware should be updated.");
                self.sink.send(DisplayBackupState {
                    confirmed: true,
                    close_dialog: false,
                    refused: false,
                });
                true
            }
            _ => false,
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        match message {
            CoordinatorToUserMessage::CardRefused { device_id, .. }
                if device_id == self.device_id =>
            {
                self.refused = true;
                self.sink.send(DisplayBackupState {
                    confirmed: false,
                    close_dialog: true,
                    refused: true,
                });
                true
            }
            _ => false,
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        if !self.should_send {
            return vec![];
        }

        self.should_send = false;
        self.messages.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{ui_protocol::UiStack, Sink};
    use frostsnap_core::message::card::CardRefusal;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeSink(Arc<Mutex<Vec<DisplayBackupState>>>);

    impl Sink<DisplayBackupState> for FakeSink {
        fn send(&self, state: DisplayBackupState) {
            self.0.lock().unwrap().push(state);
        }
    }

    /// The device refuses the 25-word display of a card-restored share: the dialog closes and
    /// says why, and nothing is broadcast to the devices.
    #[test]
    fn a_card_refusal_ends_it() {
        let device_id = DeviceId([2; 33]);
        let sink = FakeSink::default();
        let mut stack = UiStack::default();
        stack.push(DisplayBackupProtocol {
            device_id,
            abort: false,
            refused: false,
            messages: vec![],
            should_send: false,
            sink: Box::new(sink.clone()),
        });
        let refusal = |device_id| CoordinatorToUserMessage::CardRefused {
            device_id,
            reason: CardRefusal::WrongState,
        };
        stack.process_to_user_message(refusal(DeviceId([3; 33])));
        assert!(sink.0.lock().unwrap().is_empty(), "another device's");
        stack.process_to_user_message(refusal(device_id));
        assert_eq!(
            sink.0.lock().unwrap().last(),
            Some(&DisplayBackupState {
                confirmed: false,
                close_dialog: true,
                refused: true,
            })
        );
        assert!(!stack.clean_finished(), "finished, with no cancel to send");
        assert!(stack.get_mut::<DisplayBackupProtocol>().is_none());
    }
}
