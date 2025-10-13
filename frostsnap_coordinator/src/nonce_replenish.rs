use crate::{Completion, Sink, UiProtocol};

use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    coordinator::{CoordinatorToUserMessage, NonceReplenishRequest},
    message::signing::OpenNonceStreams,
    DeviceId,
};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

pub struct NonceReplenishProtocol {
    state: NonceReplenishState,
    pending_messages: HashMap<DeviceId, VecDeque<OpenNonceStreams>>,
    awaiting_response: HashSet<DeviceId>,
    completed_streams: u32,
    sink: Box<dyn Sink<NonceReplenishState>>,
}

impl NonceReplenishProtocol {
    pub fn new(
        devices: BTreeSet<DeviceId>,
        nonce_request: NonceReplenishRequest,
        sink: impl Sink<NonceReplenishState> + 'static,
    ) -> Self {
        let mut pending_messages = HashMap::new();
        let mut total_streams = 0;

        // Process NonceReplenishRequest into split OpenNonceStream messages
        for (device_id, open_nonce_stream) in nonce_request.into_open_nonce_streams() {
            // split them so we get more fine grained progress
            let split_messages = open_nonce_stream.split();
            total_streams += split_messages.len() as u32;
            pending_messages.insert(device_id, VecDeque::from(split_messages));
        }

        Self {
            state: NonceReplenishState {
                devices: devices.into_iter().collect(),
                completed_streams: 0,
                total_streams,
                abort: false,
            },
            pending_messages,
            awaiting_response: HashSet::new(),
            completed_streams: 0,
            sink: Box::new(sink),
        }
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
        if self.state.is_finished() {
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
            if self.awaiting_response.remove(&device_id) {
                self.completed_streams += 1;
                self.state.completed_streams = self.completed_streams;

                self.emit_state();
            }
            true
        } else {
            false
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        let mut messages = vec![];

        for (device_id, queue) in &mut self.pending_messages {
            if !self.awaiting_response.contains(device_id) {
                if let Some(open_nonce_stream) = queue.pop_front() {
                    let msg = CoordinatorSendMessage {
                        target_destinations: Destination::from([*device_id]),
                        message_body: CoordinatorSendBody::Core(open_nonce_stream.into()),
                    };
                    messages.push(msg);
                    self.awaiting_response.insert(*device_id);
                }
            }
        }

        messages
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
    pub devices: HashSet<DeviceId>,
    pub completed_streams: u32,
    pub total_streams: u32,
    pub abort: bool,
}

impl NonceReplenishState {
    pub fn is_finished(&self) -> bool {
        self.completed_streams >= self.total_streams
    }
}
