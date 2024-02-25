use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    message::{
        CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserSigningMessage,
        EncodedSignature, SignRequest,
    },
    DeviceId,
};
use std::collections::BTreeSet;

/// Keeps track of when
#[derive(Debug)]
pub struct SigningDispatcher {
    need_to_send_to: BTreeSet<DeviceId>,
    signing_state_changed: bool,
    // FIXME: make accessors
    pub request: SignRequest,
    pub finished_signatures: Option<Vec<EncodedSignature>>,
    pub targets: BTreeSet<DeviceId>,
    pub got_signatures: BTreeSet<DeviceId>,
}

impl SigningDispatcher {
    /// Takes in the messages from `start_sign` and extracts the signing request to handle separately.
    ///
    /// We need to do this because we want to only send out the message to the devices that are connected.
    pub fn from_filter_out_start_sign(start_sign_messages: &mut Vec<CoordinatorSend>) -> Self {
        let (i, request, targets) = start_sign_messages
            .iter()
            .enumerate()
            .find_map(|(i, m)| match m {
                CoordinatorSend::ToDevice {
                    message: CoordinatorToDeviceMessage::RequestSign(request),
                    destinations,
                } => Some((i, request.clone(), destinations.clone())),
                _ => None,
            })
            .expect("must have a sign request");

        let _ /*already cloned*/ = start_sign_messages.remove(i);
        Self::new_from_request(request, targets)
    }

    pub fn new_from_request(request: SignRequest, targets: BTreeSet<DeviceId>) -> Self {
        Self {
            request,
            targets,
            got_signatures: Default::default(),
            need_to_send_to: Default::default(),
            finished_signatures: Default::default(),
            signing_state_changed: false,
        }
    }

    pub fn process_to_user_message(&mut self, message: CoordinatorToUserSigningMessage) {
        match message {
            CoordinatorToUserSigningMessage::GotShare { from } => {
                self.signing_state_changed ^= self.got_signatures.insert(from);
            }
            CoordinatorToUserSigningMessage::Signed { signatures } => {
                self.finished_signatures = Some(signatures);
                self.signing_state_changed = true;
            }
        }
    }

    pub fn set_signature_received(&mut self, from: DeviceId) {
        self.got_signatures.insert(from);
    }

    pub fn disconnected(&mut self, device_id: DeviceId) {
        self.need_to_send_to.remove(&device_id);
    }

    pub fn connected(&mut self, device_id: DeviceId) {
        if !self.got_signatures.contains(&device_id) && self.targets.contains(&device_id) {
            self.need_to_send_to.insert(device_id);
        }
    }

    pub fn is_complete(&self) -> bool {
        self.finished_signatures.is_some()
    }

    pub fn resend_sign_request(&mut self) -> Option<CoordinatorSendMessage> {
        if !self.need_to_send_to.is_empty() {
            return Some(CoordinatorSendMessage {
                target_destinations: Destination::from(core::mem::take(&mut self.need_to_send_to)),
                message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::RequestSign(
                    self.request.clone(),
                )),
            });
        }
        None
    }

    pub fn signing_state_changed(&mut self) -> bool {
        let res = self.signing_state_changed;
        self.signing_state_changed = false;
        res
    }
}
