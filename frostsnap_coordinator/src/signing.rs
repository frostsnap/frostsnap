use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage, Destination};
use frostsnap_core::{
    message::{
        CoordinatorSend, CoordinatorToDeviceMessage, CoordinatorToUserMessage,
        CoordinatorToUserSigningMessage, EncodedSignature, SignRequest,
    },
    DeviceId,
};
use std::collections::BTreeSet;
use tracing::{event, Level};

use crate::{Completion, UiProtocol, UiToStorageMessage};

/// Keeps track of when
pub struct SigningDispatcher {
    need_to_send_to: BTreeSet<DeviceId>,
    // FIXME: make accessors
    pub request: SignRequest,
    pub finished_signatures: Option<Vec<EncodedSignature>>,
    pub targets: BTreeSet<DeviceId>,
    pub got_signatures: BTreeSet<DeviceId>,
    pub sink: Box<dyn crate::Sink<SigningState>>,
    pub aborted: Option<String>,
}

impl SigningDispatcher {
    /// Takes in the messages from `start_sign` and extracts the signing request to handle separately.
    ///
    /// We need to do this because we want to only send out the message to the devices that are connected.
    pub fn from_filter_out_start_sign(
        start_sign_messages: &mut Vec<CoordinatorSend>,
        sink: impl crate::Sink<SigningState> + 'static,
    ) -> Self {
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
        Self::new_from_request(request, targets, sink)
    }

    pub fn new_from_request(
        request: SignRequest,
        targets: BTreeSet<DeviceId>,
        sink: impl crate::Sink<SigningState> + 'static,
    ) -> Self {
        Self {
            request,
            targets,
            got_signatures: Default::default(),
            need_to_send_to: Default::default(),
            finished_signatures: Default::default(),
            sink: Box::new(sink),
            aborted: None,
        }
    }

    pub fn set_signature_received(&mut self, from: DeviceId) {
        self.got_signatures.insert(from);
    }

    pub fn emit_state(&mut self) {
        let state = SigningState {
            got_shares: self.got_signatures.iter().cloned().collect(),
            needed_from: self.targets.iter().cloned().collect(),
            finished_signatures: self.finished_signatures.clone().unwrap_or_default(),
            aborted: self.aborted.clone(),
        };

        self.sink.send(state);
    }
}

impl UiProtocol for SigningDispatcher {
    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) {
        if let CoordinatorToUserMessage::Signing(message) = message {
            match message {
                CoordinatorToUserSigningMessage::GotShare { from } => {
                    if self.got_signatures.insert(from) {
                        self.emit_state()
                    }
                }
                CoordinatorToUserSigningMessage::Signed { signatures } => {
                    self.finished_signatures = Some(signatures);
                    event!(Level::INFO, "received signatures from all devices");
                    self.emit_state();
                    self.sink.close();
                }
            }
        }
    }

    fn disconnected(&mut self, device_id: DeviceId) {
        self.need_to_send_to.remove(&device_id);
    }

    fn connected(&mut self, device_id: DeviceId) {
        if !self.got_signatures.contains(&device_id) && self.targets.contains(&device_id) {
            self.need_to_send_to.insert(device_id);
        }
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.finished_signatures.is_some() {
            Some(Completion::Success)
        } else if self.aborted.is_some() {
            Some(Completion::Abort)
        } else {
            None
        }
    }

    fn poll(&mut self) -> (Vec<CoordinatorSendMessage>, Vec<UiToStorageMessage>) {
        let mut to_devices = vec![];
        let mut to_storage = vec![];
        if !self.need_to_send_to.is_empty() {
            event!(Level::INFO, "Sending sign request");
            to_devices.push(CoordinatorSendMessage {
                target_destinations: Destination::from(core::mem::take(&mut self.need_to_send_to)),
                message_body: CoordinatorSendBody::Core(CoordinatorToDeviceMessage::RequestSign(
                    self.request.clone(),
                )),
            });
        }
        if self.is_complete().is_some() {
            to_storage.push(UiToStorageMessage::ClearSigningSession);
        }
        (to_devices, to_storage)
    }

    fn cancel(&mut self) {
        self.aborted = Some("Signing canceled".into());
        self.emit_state()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug)]
pub struct SigningState {
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    // for some reason FRB woudln't allow Option here to empty vec implies not being finished
    pub finished_signatures: Vec<EncodedSignature>,
    pub aborted: Option<String>,
}
