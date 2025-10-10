use crate::{Completion, Sink, UiProtocol};

use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{CoordinatorToUserMessage, NonceReplenishRequest},
    DeviceId,
};
use std::collections::{BTreeSet, HashSet};

pub struct NonceReplenishProtocol {
    state: NonceReplenishState,
    messages: Vec<CoordinatorSendMessage>,
    sink: Box<dyn Sink<NonceReplenishState>>,
}

impl NonceReplenishProtocol {
    pub fn new(
        devices: BTreeSet<DeviceId>,
        nonce_request: NonceReplenishRequest,
        sink: impl Sink<NonceReplenishState> + 'static,
    ) -> Self {
        let devices_with_messages: BTreeSet<DeviceId> = nonce_request
            .replenish_requests
            .iter()
            .filter(|(_, streams)| !streams.is_empty())
            .map(|(device_id, _)| *device_id)
            .collect();

        // devices that don't need messages are considered complete
        let received_from: HashSet<DeviceId> = devices
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
        if self.state.received_from == self.state.devices {
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
            if self.state.received_from.insert(device_id) {
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
    pub received_from: HashSet<DeviceId>,
    pub devices: HashSet<DeviceId>,
    pub abort: bool,
}

impl NonceReplenishState {
    pub fn is_finished(&self) -> bool {
        self.received_from == self.devices
    }
}
