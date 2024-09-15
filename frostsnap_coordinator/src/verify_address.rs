use std::{borrow::BorrowMut, collections::BTreeSet};

use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    message::{
        CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage,
        CoordinatorToUserVerifyingAddressMessage,
    },
    DeviceId, KeyId,
};
use tracing::{event, Level};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

#[derive(Clone, Debug, Default)]
pub struct VerifyAddressProtocolState {
    pub finished: bool,
    pub target_devices: Vec<DeviceId>, // not a set for frb compat
    pub acks: Vec<DeviceId>,           // not a set for frb compat
    pub aborted: Option<String>,
}

pub struct VerifyAddressProtocol {
    state: VerifyAddressProtocolState,
    key_id: KeyId,
    derivation_index: u32,
    need_to_send_to: BTreeSet<DeviceId>,
    sink: Box<dyn Sink<VerifyAddressProtocolState>>,
}

impl VerifyAddressProtocol {
    pub fn new(
        verify_address_messages: &mut Vec<CoordinatorSend>,
        sink: impl Sink<VerifyAddressProtocolState> + 'static,
    ) -> Self {
        let (i, key_id, derivation_index, targets) = verify_address_messages
            .iter()
            .enumerate()
            .find_map(|(i, m)| match m {
                CoordinatorSend::ToDevice {
                    message:
                        CoordinatorToDeviceMessage::VerifyAddress {
                            key_id,
                            derivation_index,
                        },
                    destinations,
                } => Some((i, *key_id, *derivation_index, destinations.clone())),
                _ => None,
            })
            .expect("must have a sign request");

        let _ /*recreating message when target is connected*/ = verify_address_messages.remove(i);

        Self {
            state: VerifyAddressProtocolState {
                finished: false,
                target_devices: targets.clone().into_iter().collect(),
                acks: Default::default(),
                aborted: None,
            },
            key_id,
            derivation_index,
            need_to_send_to: Default::default(),
            sink: Box::new(sink),
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }

    fn abort(&mut self, reason: String) {
        self.state.aborted = Some(reason);
        self.emit_state();
    }
}

impl UiProtocol for VerifyAddressProtocol {
    fn cancel(&mut self) {
        self.abort("cancelled".to_string());
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.state.finished {
            Some(Completion::Success)
        } else if let Some(reason) = &self.state.aborted {
            event!(Level::WARN, "verifying address aborted: {reason}");
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn connected(&mut self, id: frostsnap_core::DeviceId) {
        if !self.state.acks.contains(&id) && self.state.target_devices.contains(&id) {
            self.need_to_send_to.insert(id);
        }
        self.emit_state()
    }

    fn disconnected(&mut self, device_id: frostsnap_core::DeviceId) {
        self.need_to_send_to.remove(&device_id);
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        if let CoordinatorToUserMessage::VerifyAddress(verify_address_message) = message {
            match verify_address_message {
                CoordinatorToUserVerifyingAddressMessage::DeviceAck { from } => {
                    if self.state.target_devices.contains(&from) {
                        self.state.acks.push(from);
                    }
                }
                CoordinatorToUserVerifyingAddressMessage::Confirmed => {
                    if self.state.acks.is_empty() {
                        event!(
                            Level::WARN,
                            "Internal coordinator confirmed verifying address before we saw the acks"
                        );
                    }
                    self.state.finished = true;
                }
            }
            self.emit_state()
        } else {
            event!(
                Level::ERROR,
                "Non verify address message sent during verifying address"
            );
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        if self.is_complete().is_some() {
            return (vec![], vec![]);
        }

        let verify_address_message = CoordinatorSendMessage {
            target_destinations: Destination::Particular(core::mem::take(
                &mut self.need_to_send_to,
            )),
            message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::VerifyAddress {
                key_id: self.key_id,
                derivation_index: self.derivation_index,
            }),
        };

        (vec![verify_address_message], vec![])
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.borrow_mut()
    }
}
