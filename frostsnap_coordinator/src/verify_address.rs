use std::{borrow::BorrowMut, collections::BTreeSet};

use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    coordinator::VerifyAddress, message::CoordinatorToDeviceMessage, schnorr_fun::fun::Point,
    DeviceId,
};
use tracing::{event, Level};

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};

#[derive(Clone, Debug, Default)]
pub struct VerifyAddressProtocolState {
    pub target_devices: Vec<DeviceId>,  // not a set for frb compat
    pub sent_to_devices: Vec<DeviceId>, // not a set for frb compat
}

pub struct VerifyAddressProtocol {
    state: VerifyAddressProtocolState,
    rootkey: Point,
    derivation_index: u32,
    is_complete: Option<Completion>,
    need_to_send_to: BTreeSet<DeviceId>,
    sink: Box<dyn Sink<VerifyAddressProtocolState>>,
}

impl VerifyAddressProtocol {
    pub fn new(
        verify_address_message: VerifyAddress,
        sink: impl Sink<VerifyAddressProtocolState> + 'static,
    ) -> Self {
        Self {
            state: VerifyAddressProtocolState {
                target_devices: verify_address_message.target_devices.into_iter().collect(),
                sent_to_devices: Default::default(),
            },
            rootkey: verify_address_message.rootkey,
            derivation_index: verify_address_message.derivation_index,
            is_complete: None,
            need_to_send_to: Default::default(),
            sink: Box::new(sink),
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }
}

impl UiProtocol for VerifyAddressProtocol {
    fn cancel(&mut self) {
        self.is_complete = Some(Completion::Abort {
            send_cancel_to_all_devices: true,
        })
    }

    fn is_complete(&self) -> Option<Completion> {
        self.is_complete.clone()
    }

    fn connected(&mut self, id: frostsnap_core::DeviceId) {
        if self.state.target_devices.contains(&id) {
            self.need_to_send_to.insert(id);
            self.emit_state()
        }
    }

    fn disconnected(&mut self, device_id: frostsnap_core::DeviceId) {
        if self.need_to_send_to.remove(&device_id) {
            self.emit_state()
        };
    }

    fn process_to_user_message(
        &mut self,
        message: frostsnap_core::message::CoordinatorToUserMessage,
    ) {
        event!(
            Level::ERROR,
            "Unexpected message sent during verifying address {:?}",
            message
        );
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        if self.is_complete.is_some() {
            return (vec![], vec![]);
        }

        self.state
            .sent_to_devices
            .extend(self.need_to_send_to.iter().cloned());

        let verify_address_message = CoordinatorSendMessage {
            target_destinations: Destination::Particular(core::mem::take(
                &mut self.need_to_send_to,
            )),
            message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::VerifyAddress {
                rootkey: self.rootkey,
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
