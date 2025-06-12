use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{
        ActiveSignSession, CoordinatorSend, CoordinatorToUserMessage,
        CoordinatorToUserSigningMessage, RequestDeviceSign,
    },
    message::EncodedSignature,
    DeviceId, KeyId, SignSessionId,
};
use std::collections::BTreeSet;
use tracing::{event, Level};

use crate::{Completion, DeviceMode, UiProtocol};

/// Keeps track of when
pub struct SigningDispatcher {
    pub key_id: KeyId,
    pub session_id: SignSessionId,
    pub finished_signatures: Option<Vec<EncodedSignature>>,
    pub targets: BTreeSet<DeviceId>,
    pub got_signatures: BTreeSet<DeviceId>,
    pub sink: Box<dyn crate::Sink<SigningState>>,
    pub aborted: Option<String>,
    pub connected_but_need_request: BTreeSet<DeviceId>,
    pub outbox_to_devices: Vec<CoordinatorSendMessage>,
}

impl SigningDispatcher {
    pub fn new(
        targets: BTreeSet<DeviceId>,
        key_id: KeyId,
        session_id: SignSessionId,
        sink: impl crate::Sink<SigningState>,
    ) -> Self {
        Self {
            targets,
            key_id,
            session_id,
            got_signatures: Default::default(),
            finished_signatures: Default::default(),
            sink: Box::new(sink),
            aborted: None,
            connected_but_need_request: Default::default(),
            outbox_to_devices: Default::default(),
        }
    }

    pub fn restore_signing_session(
        active_sign_session: &ActiveSignSession,
        sink: impl crate::Sink<SigningState>,
    ) -> Self {
        Self {
            key_id: active_sign_session.key_id,
            session_id: active_sign_session.session_id(),
            got_signatures: active_sign_session.received_from().collect(),
            targets: active_sign_session.init.nonces.keys().cloned().collect(),
            finished_signatures: None,
            sink: Box::new(sink),
            aborted: None,
            connected_but_need_request: Default::default(),
            outbox_to_devices: Default::default(),
        }
    }

    pub fn set_signature_received(&mut self, from: DeviceId) {
        self.got_signatures.insert(from);
    }

    pub fn emit_state(&mut self) {
        let state = SigningState {
            session_id: self.session_id,
            got_shares: self.got_signatures.iter().cloned().collect(),
            needed_from: self.targets.iter().cloned().collect(),
            finished_signatures: self.finished_signatures.clone().unwrap_or_default(),
            aborted: self.aborted.clone(),
            connected_but_need_request: self.connected_but_need_request.iter().cloned().collect(),
        };
        self.sink.send(state);
    }

    pub fn send_sign_request(&mut self, sign_req: RequestDeviceSign) {
        if self.connected_but_need_request.remove(&sign_req.device_id) {
            self.outbox_to_devices.push(
                CoordinatorSend::from(sign_req)
                    .try_into()
                    .expect("sign_req goes to devices"),
            );
            self.emit_state();
        }
    }
}

impl UiProtocol for SigningDispatcher {
    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        if let CoordinatorToUserMessage::Signing(message) = message {
            match message {
                CoordinatorToUserSigningMessage::GotShare { from, session_id } => {
                    if session_id != self.session_id {
                        return false;
                    }
                    if self.got_signatures.insert(from) {
                        self.emit_state()
                    }
                }
                CoordinatorToUserSigningMessage::Signed {
                    signatures,
                    session_id,
                } => {
                    if session_id != self.session_id {
                        return false;
                    }
                    self.finished_signatures = Some(signatures);
                    event!(Level::INFO, "received signatures from all devices");
                    self.emit_state();
                }
            }
            true
        } else {
            false
        }
    }

    fn disconnected(&mut self, device_id: DeviceId) {
        self.connected_but_need_request.remove(&device_id);
        self.emit_state();
    }

    fn connected(&mut self, device_id: DeviceId, state: DeviceMode) {
        if !self.got_signatures.contains(&device_id)
            && self.targets.contains(&device_id)
            && state == DeviceMode::Ready
        {
            self.connected_but_need_request.insert(device_id);
            self.emit_state();
        }
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.finished_signatures.is_some() {
            Some(Completion::Success)
        } else if self.aborted.is_some() {
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        core::mem::take(&mut self.outbox_to_devices)
    }

    fn cancel(&mut self) {
        self.aborted = Some("Signing canceled".into());
        self.emit_state()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug)]
pub struct SigningState {
    pub session_id: SignSessionId,
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    // for some reason FRB woudln't allow Option here to empty vec implies not being finished
    pub finished_signatures: Vec<EncodedSignature>,
    pub aborted: Option<String>,
    pub connected_but_need_request: Vec<DeviceId>,
}
