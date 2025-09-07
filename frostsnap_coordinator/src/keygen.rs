use std::collections::BTreeSet;

use crate::{Completion, Sink, UiProtocol};
use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{BeginKeygen, CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, FrostCoordinator},
    AccessStructureRef, DeviceId, KeygenId, SessionHash,
};
use tracing::{event, Level};

pub struct KeyGen {
    sink: Box<dyn Sink<KeyGenState>>,
    state: KeyGenState,
    keygen_messages: Vec<CoordinatorSendMessage>,
    send_cancel_to_all: bool,
}

impl KeyGen {
    pub fn new(
        keygen_sink: impl Sink<KeyGenState> + 'static,
        coordinator: &mut FrostCoordinator,
        currently_connected: BTreeSet<DeviceId>,
        begin_keygen: BeginKeygen,
        rng: &mut impl rand_core::RngCore,
    ) -> Self {
        let mut self_ = Self {
            sink: Box::new(keygen_sink),
            state: KeyGenState {
                devices: begin_keygen.devices().into_iter().collect(),
                threshold: begin_keygen.threshold.into(),
                keygen_id: begin_keygen.keygen_id,
                ..Default::default()
            },
            keygen_messages: vec![],
            send_cancel_to_all: false,
        };

        if !currently_connected.is_superset(&begin_keygen.devices()) {
            self_.abort("A selected device was disconnected".into(), false);
        }

        match coordinator.begin_keygen(begin_keygen, rng) {
            Ok(messages) => {
                for message in messages {
                    self_.keygen_messages.push(
                        message
                            .try_into()
                            .expect("will only send messages to device"),
                    );
                }
            }
            Err(e) => self_.abort(format!("couldn't start keygen: {e}"), false),
        }

        self_
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }

    fn abort(&mut self, reason: String, send_cancel_to_all: bool) {
        self.state.aborted = Some(reason);
        self.send_cancel_to_all = send_cancel_to_all;
        self.emit_state();
    }

    pub fn keygen_finalized(&mut self, as_ref: AccessStructureRef) {
        self.state.finished = Some(as_ref);
        self.emit_state()
    }
}

impl UiProtocol for KeyGen {
    fn cancel(&mut self) {
        self.abort("Key generation canceled".into(), true);
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.state.finished.is_some() {
            Some(Completion::Success)
        } else if self.state.aborted.is_some() {
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        if let CoordinatorToUserMessage::KeyGen { keygen_id, inner } = message {
            if keygen_id == self.state.keygen_id {
                match inner {
                    CoordinatorToUserKeyGenMessage::ReceivedShares { from } => {
                        self.state.got_shares.push(from);
                        if self.state.got_shares.len() == self.state.devices.len() {
                            self.state.all_shares = true;
                        }
                    }
                    CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
                        self.state.session_hash = Some(session_hash);
                    }
                    CoordinatorToUserKeyGenMessage::KeyGenAck {
                        from,
                        all_acks_received,
                    } => {
                        self.state.session_acks.push(from);
                        assert_eq!(
                            all_acks_received,
                            self.state.session_acks.len() == self.state.devices.len(),
                        );
                        self.state.all_acks = all_acks_received;
                    }
                }
            }
            self.emit_state();
            true
        } else {
            false
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        core::mem::take(&mut self.keygen_messages)
    }

    fn disconnected(&mut self, id: frostsnap_core::DeviceId) {
        if self.state.devices.contains(&id) {
            event!(
                Level::ERROR,
                id = id.to_string(),
                "Device disconnected during keygen"
            );
            self.abort(
                "Key generation failed because a device was disconnected".into(),
                true,
            );
            self.emit_state();
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct KeyGenState {
    pub threshold: usize,
    pub devices: Vec<DeviceId>, // not a set for frb compat
    pub got_shares: Vec<DeviceId>,
    pub all_shares: bool,
    pub session_acks: Vec<DeviceId>,
    pub all_acks: bool,
    pub session_hash: Option<SessionHash>,
    pub finished: Option<AccessStructureRef>,
    pub aborted: Option<String>,
    pub keygen_id: KeygenId,
}
