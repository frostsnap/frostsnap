use crate::api;
use flutter_rust_bridge::StreamSink;
use frostsnap_coordinator::{
    frostsnap_comms::CoordinatorSendMessage,
    frostsnap_core::{message::CoordinatorToUserSigningMessage, DeviceId},
    SigningDispatcher,
};

pub struct SigningSession {
    stream: StreamSink<api::SigningState>,
    dispatcher: SigningDispatcher,
}

impl core::fmt::Debug for SigningSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningSession")
            .field("stream", &"..")
            .field("dispatcher", &self.dispatcher)
            .finish()
    }
}

impl SigningSession {
    pub fn new(stream: StreamSink<api::SigningState>, dispatcher: SigningDispatcher) -> Self {
        Self { stream, dispatcher }
    }
    pub fn process_to_user_message(&mut self, message: CoordinatorToUserSigningMessage) {
        self.dispatcher.process_to_user_message(message);
        if self.dispatcher.signing_state_changed() {
            self.stream.add(self.signing_state());
        }

        if self.is_complete() {
            self.stream.close();
        }
    }

    pub fn resend_sign_request(&mut self) -> Option<CoordinatorSendMessage> {
        self.dispatcher.resend_sign_request()
    }

    pub fn connected(&mut self, device_id: DeviceId) {
        self.dispatcher.connected(device_id);
    }

    pub fn disconnected(&mut self, device_id: DeviceId) {
        self.dispatcher.disconnected(device_id)
    }

    pub fn is_complete(&self) -> bool {
        self.dispatcher.is_complete()
    }

    pub fn signing_state(&self) -> api::SigningState {
        api::SigningState {
            got_shares: self.dispatcher.got_signatures.iter().cloned().collect(),
            needed_from: self.dispatcher.targets.iter().cloned().collect(),
            finished_signatures: self
                .dispatcher
                .finished_signatures
                .clone()
                .unwrap_or_default(),
        }
    }

    pub fn cancel(&mut self) {
        self.stream.close();
    }
}
