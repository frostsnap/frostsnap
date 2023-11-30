use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    message::{
        CoordinatorSend, CoordinatorToDeviceMessage, DeviceToCoordinatorMessage, SignRequest,
    },
    DeviceId,
};
use std::collections::HashSet;

pub struct SigningDispatcher {
    request: SignRequest,
    targets: HashSet<DeviceId>,
    got_signatures: HashSet<DeviceId>,
    need_to_send_to: HashSet<DeviceId>,
}

impl SigningDispatcher {
    /// Takes in the messages from `start_sign` and extracts the signing request to handle separately.
    ///
    /// We need to do this because we want to only send out the message to the devices that are connected.
    pub fn new(start_sign_messages: &mut Vec<CoordinatorSend>) -> Self {
        let (i, request) = start_sign_messages
            .iter()
            .enumerate()
            .find_map(|(i, m)| match m {
                CoordinatorSend::ToDevice(CoordinatorToDeviceMessage::RequestSign(request)) => {
                    Some((i, request.clone()))
                }
                _ => None,
            })
            .expect("must have a sign request");

        start_sign_messages.remove(i);
        let targets = request.devices().collect::<HashSet<_>>();

        Self {
            request,
            targets,
            got_signatures: Default::default(),
            need_to_send_to: Default::default(),
        }
    }
    pub fn process(&mut self, from: DeviceId, message: &DeviceToCoordinatorMessage) {
        if !self.targets.contains(&from) {
            return;
        }
        if let DeviceToCoordinatorMessage::SignatureShare { .. } = message {
            self.got_signatures.insert(from);
        }
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
        self.got_signatures.is_superset(&self.targets)
    }

    pub fn emit_messages(&mut self) -> Option<CoordinatorSendMessage> {
        if !self.need_to_send_to.is_empty() {
            return Some(CoordinatorSendMessage {
                target_destinations: Destination::from(self.need_to_send_to.drain()),
                message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::RequestSign(
                    self.request.clone(),
                )),
            });
        }
        None
    }
}
