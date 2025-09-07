use std::{borrow::BorrowMut, collections::BTreeSet};

use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    coordinator::VerifyAddress, message::CoordinatorToDeviceMessage, DeviceId, MasterAppkey,
};

use crate::{Completion, DeviceMode, Sink, UiProtocol};

#[derive(Clone, Debug, Default)]
pub struct VerifyAddressProtocolState {
    pub target_devices: Vec<DeviceId>, // not a set for frb compat
}

pub struct VerifyAddressProtocol {
    state: VerifyAddressProtocolState,
    master_appkey: MasterAppkey,
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
            },
            master_appkey: verify_address_message.master_appkey,
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

    fn connected(&mut self, id: frostsnap_core::DeviceId, state: DeviceMode) {
        if self.state.target_devices.contains(&id) && state == DeviceMode::Ready {
            self.need_to_send_to.insert(id);
            self.emit_state()
        }
    }

    fn disconnected(&mut self, device_id: frostsnap_core::DeviceId) {
        if self.need_to_send_to.remove(&device_id) {
            self.emit_state()
        };
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        let mut messages = vec![];
        if !self.need_to_send_to.is_empty() {
            messages.push(CoordinatorSendMessage {
                target_destinations: Destination::Particular(core::mem::take(
                    &mut self.need_to_send_to,
                )),
                message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::ScreenVerify(
                    frostsnap_core::message::screen_verify::ScreenVerify::VerifyAddress {
                        master_appkey: self.master_appkey,
                        derivation_index: self.derivation_index,
                    },
                )),
            });
        }

        messages
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.borrow_mut()
    }
}
