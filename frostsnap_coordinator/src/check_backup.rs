use crate::{Completion, DeviceMode, FirmwareVersion, Sink, UiProtocol};
use frostsnap_comms::{CommsMisc, CoordinatorSendMessage};
use frostsnap_core::schnorr_fun::frost::{ShareImage, ShareIndex};
use frostsnap_core::{
    coordinator::{restoration, CoordinatorToUserMessage, FrostCoordinator},
    AccessStructureRef, DeviceId, EnterPhysicalId, SymmetricKey,
};

#[derive(Debug, Clone)]
pub struct CheckBackupState {
    pub backup_manually_entered_valid: Option<bool>,
    /// The device refused: the share was restored from a verifiable card, whose words it never
    /// checks as 25.
    pub refused: bool,
}

enum CheckBackupKind {
    Modern {
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
    Legacy {
        enter_physical_id: EnterPhysicalId,
        expected_share_image: ShareImage,
    },
}

pub struct CheckBackupProtocol {
    device_id: DeviceId,
    abort: bool,
    finished: bool,
    refused: bool,
    confirmed: bool,
    messages: Vec<CoordinatorSendMessage>,
    should_send: bool,
    kind: CheckBackupKind,
    sink: Box<dyn Sink<CheckBackupState> + Send>,
}

impl CheckBackupProtocol {
    pub fn new(
        coord: &mut FrostCoordinator,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
        encryption_key: SymmetricKey,
        firmware: FirmwareVersion,
        sink: impl Sink<CheckBackupState> + 'static,
    ) -> anyhow::Result<Self> {
        let (messages, kind) = if firmware.features().check_backup {
            let messages = coord
                .request_device_check_backup(device_id, access_structure_ref, encryption_key)?
                .into_iter()
                .map(|message| {
                    message
                        .try_into()
                        .expect("will only send messages to device")
                })
                .collect();
            (
                messages,
                CheckBackupKind::Modern {
                    access_structure_ref,
                    share_index,
                },
            )
        } else {
            let expected_share_image = coord
                .expected_share_image(access_structure_ref, share_index, encryption_key)
                .ok_or_else(|| anyhow::anyhow!("couldn't decrypt root key"))?;
            let enter_physical_id = EnterPhysicalId::new(&mut rand::thread_rng());
            let messages = coord
                .tell_device_to_load_physical_backup(enter_physical_id, device_id)
                .into_iter()
                .map(|message| {
                    message
                        .try_into()
                        .expect("will only send messages to device")
                })
                .collect();
            (
                messages,
                CheckBackupKind::Legacy {
                    enter_physical_id,
                    expected_share_image,
                },
            )
        };

        Ok(Self {
            device_id,
            sink: Box::new(sink),
            abort: false,
            finished: false,
            refused: false,
            confirmed: false,
            messages,
            should_send: true,
            kind,
        })
    }

    fn abort(&mut self) {
        self.abort = true;
    }
}

impl UiProtocol for CheckBackupProtocol {
    fn cancel(&mut self) {
        self.abort();
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.finished {
            Some(Completion::Success)
        } else if self.refused {
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

    fn connected(&mut self, id: DeviceId, _state: DeviceMode) {
        if id == self.device_id {
            self.should_send = true;
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        if self.device_id == id {
            self.abort()
        }
    }

    fn process_comms_message(&mut self, from: DeviceId, message: CommsMisc) -> bool {
        if self.device_id != from {
            return false;
        }
        match (&self.kind, message) {
            (
                CheckBackupKind::Modern {
                    access_structure_ref: expected_access_structure_ref,
                    share_index: expected_share_index,
                },
                CommsMisc::BackupChecked {
                    access_structure_ref,
                    share_index,
                },
            ) if access_structure_ref == *expected_access_structure_ref
                && share_index == *expected_share_index =>
            {
                self.finished = true;
                self.confirmed = true;
                self.sink.send(CheckBackupState {
                    backup_manually_entered_valid: None,
                    refused: false,
                });
                true
            }
            _ => false,
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        match (&self.kind, message) {
            (
                CheckBackupKind::Legacy {
                    enter_physical_id,
                    expected_share_image,
                },
                CoordinatorToUserMessage::Restoration(
                    restoration::ToUserRestoration::PhysicalBackupEntered(phase),
                ),
            ) if phase.from == self.device_id
                && phase.backup.enter_physical_id == *enter_physical_id =>
            {
                let confirmed = phase.backup.share_image == *expected_share_image;
                self.finished = true;
                self.confirmed = confirmed;
                self.sink.send(CheckBackupState {
                    backup_manually_entered_valid: Some(confirmed),
                    refused: false,
                });
                true
            }
            (_, CoordinatorToUserMessage::CardRefused { device_id, .. })
                if device_id == self.device_id =>
            {
                self.refused = true;
                self.sink.send(CheckBackupState {
                    backup_manually_entered_valid: None,
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
    use frostsnap_core::schnorr_fun::fun::{marker::*, Scalar};
    use frostsnap_core::{AccessStructureId, KeyId};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeSink(Arc<Mutex<Vec<CheckBackupState>>>);

    impl Sink<CheckBackupState> for FakeSink {
        fn send(&self, state: CheckBackupState) {
            self.0.lock().unwrap().push(state);
        }
    }

    /// The device refuses the 25-word check of a card-restored share: the check ends and says
    /// why, and nothing is broadcast to the devices.
    #[test]
    fn a_card_refusal_ends_it() {
        let device_id = DeviceId([2; 33]);
        let sink = FakeSink::default();
        let mut stack = UiStack::default();
        stack.push(CheckBackupProtocol {
            device_id,
            abort: false,
            finished: false,
            refused: false,
            confirmed: false,
            messages: vec![],
            should_send: false,
            kind: CheckBackupKind::Modern {
                access_structure_ref: AccessStructureRef {
                    key_id: KeyId([1; 32]),
                    access_structure_id: AccessStructureId([3; 32]),
                },
                share_index: Scalar::<Secret, NonZero>::one().public(),
            },
            sink: Box::new(sink.clone()),
        });
        stack.process_to_user_message(CoordinatorToUserMessage::CardRefused {
            device_id,
            reason: CardRefusal::WrongState,
        });
        let last = sink.0.lock().unwrap().last().cloned().expect("a state");
        assert!(last.refused);
        assert_eq!(last.backup_manually_entered_valid, None);
        assert!(!stack.clean_finished(), "finished, with no cancel to send");
        assert!(stack.get_mut::<CheckBackupProtocol>().is_none());
    }
}
