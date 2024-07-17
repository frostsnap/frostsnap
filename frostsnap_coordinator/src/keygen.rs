use std::collections::BTreeSet;

use crate::{Completion, Sink, UiProtocol, UiToStorageMessage};
use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    message::{CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage},
    DeviceId, KeyId,
};
use tracing::{event, Level};

pub struct KeyGen {
    sink: Box<dyn Sink<KeyGenState>>,
    state: KeyGenState,
}

impl KeyGen {
    pub fn new(
        keygen_sink: impl Sink<KeyGenState> + 'static,
        devices: BTreeSet<DeviceId>,
        threshold: usize,
    ) -> Self {
        Self {
            sink: Box::new(keygen_sink),
            state: KeyGenState {
                devices: devices.into_iter().collect(),
                threshold,
                ..Default::default()
            },
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }
}

impl UiProtocol for KeyGen {
    fn cancel(&mut self) {
        self.state.aborted = Some("Key generation canceled".into());
        self.emit_state();
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.state.finished.is_some() {
            Some(Completion::Success)
        } else if self.state.aborted.is_some() {
            Some(Completion::Abort)
        } else {
            None
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        if let CoordinatorToUserMessage::KeyGen(message) = message {
            match message {
                CoordinatorToUserKeyGenMessage::ReceivedShares { from } => {
                    self.state.got_shares.push(from);
                }
                CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
                    self.state.session_hash = Some(session_hash);
                }
                CoordinatorToUserKeyGenMessage::KeyGenAck { from } => {
                    self.state.session_acks.push(from);
                }
                CoordinatorToUserKeyGenMessage::FinishedKey { key_id } => {
                    self.state.finished = Some(key_id);
                }
            }
            self.emit_state();
        } else {
            event!(Level::ERROR, "Non keygen message sent during keygen");
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        (vec![], vec![])
    }

    fn connected(&mut self, _id: frostsnap_core::DeviceId) {
        // generally a bad idea to connect devices during keygen but nothing needs to be done per se.
    }

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.state.devices.contains(&id) {
            event!(
                Level::ERROR,
                id = id.to_string(),
                "Device disconnected during keygen"
            );
            self.state.aborted =
                Some("Key generation failed because a device was disconnected".into());
            self.emit_state();
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct KeyGenState {
    pub threshold: usize,
    pub devices: Vec<DeviceId>, // not a set for frb compat
    pub got_shares: Vec<DeviceId>,
    pub session_acks: Vec<DeviceId>,
    pub session_hash: Option<[u8; 32]>,
    pub finished: Option<KeyId>,
    pub aborted: Option<String>,
}
