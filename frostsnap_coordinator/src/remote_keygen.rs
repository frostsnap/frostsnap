use std::collections::BTreeSet;

use crate::keygen::KeyGenState;
use crate::{Completion, Sink, UiProtocol};
use frostsnap_comms::CoordinatorSendMessage;
use frostsnap_core::{
    coordinator::{CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage},
    AccessStructureRef, DeviceId, KeygenId,
};
use tracing::{event, Level};

/// UI protocol for a remote keygen ceremony.
///
/// Sibling of [`crate::keygen::KeyGen`]. Differences:
/// - Constructor does **not** call `coordinator.begin_keygen` — that's done by
///   the caller (`Coordinator::start_remote_keygen`) via `begin_remote_keygen`.
/// - `poll()` returns no buffered USB messages; ToDevice sends are pushed
///   inline to the USB sender by the caller's bidi loop.
/// - `cancel()` only flips `state.aborted` for the Dart-facing stream; it
///   does **not** broadcast a USB cancel-all (most devices belong to other
///   participants). USB cancels for local devices are issued by the bidi
///   loop's central cleanup step.
/// - `disconnected(id)` for a *local* device fires the supplied
///   `on_local_disconnect` callback — the bidi loop is responsible for
///   waking up and running the central cleanup. Calling `abort()` here
///   directly would race with the loop's cleanup-side `cancel()` call.
pub struct RemoteKeyGen {
    sink: Box<dyn Sink<KeyGenState>>,
    state: KeyGenState,
    local_devices: BTreeSet<DeviceId>,
    on_local_disconnect: Box<dyn Fn() + Send + Sync>,
}

impl RemoteKeyGen {
    pub fn new(
        sink: impl Sink<KeyGenState> + 'static,
        keygen_id: KeygenId,
        threshold: u16,
        all_devices: Vec<DeviceId>,
        local_devices: BTreeSet<DeviceId>,
        on_local_disconnect: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            sink: Box::new(sink),
            state: KeyGenState {
                threshold: threshold.into(),
                devices: all_devices,
                keygen_id,
                ..Default::default()
            },
            local_devices,
            on_local_disconnect: Box::new(on_local_disconnect),
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }

    fn abort(&mut self, reason: String) {
        if self.state.finished.is_some() || self.state.aborted.is_some() {
            return;
        }
        self.state.aborted = Some(reason);
        self.emit_state();
    }

    pub fn keygen_finalized(&mut self, as_ref: AccessStructureRef) {
        self.state.finished = Some(as_ref);
        self.emit_state()
    }

    pub fn is_finalized(&self) -> bool {
        self.state.finished.is_some()
    }

    pub fn keygen_id(&self) -> KeygenId {
        self.state.keygen_id
    }

    pub fn local_devices(&self) -> &BTreeSet<DeviceId> {
        &self.local_devices
    }
}

impl UiProtocol for RemoteKeyGen {
    fn cancel(&mut self) {
        self.abort("Remote keygen cancelled".into());
    }

    fn is_complete(&self) -> Option<Completion> {
        if self.state.finished.is_some() {
            Some(Completion::Success)
        } else if self.state.aborted.is_some() {
            // Don't broadcast cancel to all USB devices — most belong to other
            // participants. The bidi loop's cleanup issues USB cancels only
            // to our `local_devices`.
            Some(Completion::Abort {
                send_cancel_to_all_devices: false,
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
        // Nothing buffered — broadcasts go through the bidi channel; ToDevice
        // sends are emitted inline by the run loop's drain helper.
        vec![]
    }

    fn disconnected(&mut self, id: DeviceId) {
        if self.local_devices.contains(&id) {
            event!(
                Level::ERROR,
                id = id.to_string(),
                "Local device disconnected during remote keygen"
            );
            (self.on_local_disconnect)();
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
