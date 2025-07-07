use crate::{Completion, Sink, UiProtocol};

use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{CoordinatorToUserMessage, FrostCoordinator},
    DeviceId,
};
use std::collections::BTreeSet;

pub struct NonceReplenishProtocol {
    state: NonceReplenishState,
    messages: Vec<CoordinatorSendMessage>,
    sink: Box<dyn Sink<NonceReplenishState>>,
}

impl NonceReplenishProtocol {
    pub fn new(
        coordinator: &mut FrostCoordinator,
        devices: BTreeSet<DeviceId>,
        desired_nonce_streams: usize,
        rng: &mut impl rand_core::RngCore,
        sink: impl Sink<NonceReplenishState> + 'static,
    ) -> Self {
        let nonce_request =
            coordinator.maybe_request_nonce_replenishment(&devices, desired_nonce_streams, rng);

        let devices_with_messages: BTreeSet<DeviceId> = nonce_request
            .replenish_requests
            .iter()
            .filter(|(_, streams)| !streams.is_empty())
            .map(|(device_id, _)| *device_id)
            .collect();

        // devices that don't need messages as already "received from"
        let received_from: Vec<DeviceId> = devices
            .difference(&devices_with_messages)
            .copied()
            .collect();

        let mut self_ = Self {
            state: NonceReplenishState {
                devices: devices.into_iter().collect(),
                received_from,
                abort: false,
            },
            messages: Default::default(),
            sink: Box::new(sink),
        };

        // Convert NonceReplenishRequest to messages
        for message in nonce_request {
            self_.messages.push(
                message
                    .try_into()
                    .expect("will only send messages to device"),
            );
        }

        self_
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }
}

impl UiProtocol for NonceReplenishProtocol {
    fn cancel(&mut self) {
        self.state.abort = true;
        self.emit_state();
    }

    fn is_complete(&self) -> Option<Completion> {
        if BTreeSet::from_iter(self.state.received_from.iter())
            == BTreeSet::from_iter(self.state.devices.iter())
        {
            Some(Completion::Success)
        } else if self.state.abort {
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        if self.state.devices.contains(&id) {
            self.state.abort = true;
            self.emit_state();
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        if let CoordinatorToUserMessage::ReplenishedNonces { device_id } = message {
            if !self.state.received_from.contains(&device_id) {
                self.state.received_from.push(device_id);
                self.emit_state()
            }
            true
        } else {
            false
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        core::mem::take(&mut self.messages)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug)]
pub struct NonceReplenishState {
    pub received_from: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub abort: bool,
}
