use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{CoordinatorSend, CoordinatorToUserMessage, CoordinatorToUserSigningMessage},
    message::signing::SpContactSessionId,
    DeviceId,
};
use std::collections::BTreeSet;

use crate::{Completion, DeviceMode, UiProtocol};

/// Progress of adding a contact, before the wallet signs it.
#[derive(Clone, Debug)]
pub struct SpContactState {
    pub session_id: SpContactSessionId,
    pub confirmed: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    pub connected_but_need_request: Vec<DeviceId>,
    pub aborted: Option<String>,
}

/// Sends `RequestSpContactEcdh` to the selected devices and tracks who has confirmed.
///
/// A disconnect drops the device from `connected_but_need_request`, and from `confirmed` since its
/// approval is gone. A later connect puts it back and the caller re-asks with
/// `request_sp_contact_ecdh`.
pub struct SpContactDispatcher {
    session_id: SpContactSessionId,
    targets: BTreeSet<DeviceId>,
    confirmed: BTreeSet<DeviceId>,
    sink: Box<dyn crate::Sink<SpContactState>>,
    aborted: Option<String>,
    /// Set by [`Self::finish`]. Completes as success so the stack does not `send_cancel_all`.
    finished: bool,
    connected_but_need_request: BTreeSet<DeviceId>,
    outbox_to_devices: Vec<CoordinatorSendMessage>,
}

impl SpContactDispatcher {
    pub fn new(
        targets: BTreeSet<DeviceId>,
        session_id: SpContactSessionId,
        sink: impl crate::Sink<SpContactState>,
    ) -> Self {
        Self {
            targets,
            session_id,
            confirmed: Default::default(),
            sink: Box::new(sink),
            aborted: None,
            finished: false,
            connected_but_need_request: Default::default(),
            outbox_to_devices: Default::default(),
        }
    }

    pub fn session_id(&self) -> SpContactSessionId {
        self.session_id
    }

    pub fn emit_state(&mut self) {
        self.sink.send(SpContactState {
            session_id: self.session_id,
            confirmed: self.confirmed.iter().copied().collect(),
            needed_from: self.targets.iter().copied().collect(),
            connected_but_need_request: self.connected_but_need_request.iter().copied().collect(),
            aborted: self.aborted.clone(),
        });
    }

    /// Delivers a built `RequestSpContactEcdh` if that device is plugged in and still waiting. A request
    /// that arrives before the device is connected is dropped; the caller asks again when
    /// `connected_but_need_request` includes it. Asking again is safe: the device keeps nothing from
    /// an earlier request beyond the user's approval.
    pub fn send_request(&mut self, send: CoordinatorSend) {
        let device_id = match &send {
            CoordinatorSend::ToDevice { destinations, .. } => destinations.iter().next().copied(),
            CoordinatorSend::ToUser(_) => None,
        };
        let Some(device_id) = device_id else {
            return;
        };
        if self.connected_but_need_request.remove(&device_id) {
            self.outbox_to_devices.push(
                send.try_into()
                    .expect("contact ECDH request goes to a device"),
            );
            self.emit_state();
        }
    }

    /// Signing is open. Leave the stack without a stack-wide cancel, which would also clear the
    /// approvals of the devices about to be asked to sign. Devices that never answered may still
    /// be showing the prompt, so only they are cancelled.
    pub fn finish(&mut self) {
        for device_id in self.targets.difference(&self.confirmed) {
            self.outbox_to_devices.push(CoordinatorSendMessage::to(
                *device_id,
                frostsnap_comms::CoordinatorSendBody::Cancel,
            ));
        }
        self.finished = true;
        self.emit_state();
    }
}

impl UiProtocol for SpContactDispatcher {
    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        let CoordinatorToUserMessage::Signing(
            CoordinatorToUserSigningMessage::GotSpContactEcdhShare { session_id, from },
        ) = message
        else {
            return false;
        };
        if session_id != self.session_id {
            return false;
        }
        if self.confirmed.insert(from) {
            self.connected_but_need_request.remove(&from);
            self.emit_state();
        }
        true
    }

    // A device loses its approval when it disconnects. The caller tells the coordinator to forget
    // its share with `forget_sp_contact_shares_from`, and the device is asked again on reconnect.
    fn disconnected(&mut self, device_id: DeviceId) {
        let need_request = self.connected_but_need_request.remove(&device_id);
        let confirmed = self.confirmed.remove(&device_id);
        if need_request || confirmed {
            self.emit_state();
        }
    }

    fn connected(&mut self, device_id: DeviceId, state: DeviceMode) {
        if !self.confirmed.contains(&device_id)
            && self.targets.contains(&device_id)
            && state == DeviceMode::Ready
        {
            self.connected_but_need_request.insert(device_id);
            self.emit_state();
        }
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.finished {
            return Some(Completion::Success);
        }
        self.aborted.as_ref().map(|_| Completion::Abort {
            send_cancel_to_all_devices: true,
        })
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        core::mem::take(&mut self.outbox_to_devices)
    }

    fn cancel(&mut self) {
        self.aborted = Some("Adding the contact was canceled".into());
        self.emit_state();
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frostsnap_comms::{CoordinatorSendBody, Destination};

    #[test]
    fn a_confirmed_device_that_reconnects_is_asked_again() {
        let device = DeviceId([1; 33]);
        let session_id = SpContactSessionId([0; 32]);
        let mut dispatcher = SpContactDispatcher::new([device].into(), session_id, ());
        dispatcher.connected(device, DeviceMode::Ready);
        dispatcher.process_to_user_message(CoordinatorToUserMessage::Signing(
            CoordinatorToUserSigningMessage::GotSpContactEcdhShare {
                session_id,
                from: device,
            },
        ));

        dispatcher.disconnected(device);
        dispatcher.connected(device, DeviceMode::Ready);

        assert!(!dispatcher.confirmed.contains(&device));
        assert!(dispatcher.connected_but_need_request.contains(&device));
    }

    #[test]
    fn finished_cancels_only_unconfirmed_devices() {
        let confirmed = DeviceId([1; 33]);
        let unconfirmed = DeviceId([2; 33]);
        let session_id = SpContactSessionId([0; 32]);
        let mut dispatcher =
            SpContactDispatcher::new([confirmed, unconfirmed].into(), session_id, ());
        assert!(
            dispatcher.process_to_user_message(CoordinatorToUserMessage::Signing(
                CoordinatorToUserSigningMessage::GotSpContactEcdhShare {
                    session_id,
                    from: confirmed,
                },
            ))
        );

        dispatcher.finish();
        let out = dispatcher.poll();

        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].message_body, CoordinatorSendBody::Cancel));
        assert!(matches!(
            &out[0].target_destinations,
            Destination::Particular(ids) if ids == &BTreeSet::from([unconfirmed])
        ));
        assert!(matches!(
            dispatcher.is_complete(),
            Some(Completion::Success)
        ));
    }
}
