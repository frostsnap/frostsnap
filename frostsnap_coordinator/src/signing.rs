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
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use tracing::{event, Level};

use crate::{Completion, DeviceMode, UiProtocol};

/// Command messages sent from a `SigningSessionHandle` to its
/// `SigningDispatcher`. Drained inside `UiProtocol::poll` on the
/// coordinator loop thread; no mutex required.
pub enum DispatcherCommand {
    /// Queue a built `RequestDeviceSign` for USB delivery. Only actually
    /// placed on the USB outbox if the target device is currently in
    /// `connected_but_need_request` (i.e. plugged in + awaiting a request).
    /// Boxed because it's ~384 bytes — avoids bloating the other variants.
    SendSignRequest(Box<RequestDeviceSign>),
    /// Cancel this specific session. Scoped replacement for the blunt
    /// `cancel_all` on the whole `ui_stack`.
    Cancel,
}

/// Streams `SigningState` for either a local or a remote signing session.
///
/// In **local mode**, the dispatcher waits for a
/// [`CoordinatorToUserSigningMessage::Signed`] message carrying the combined
/// signatures before it declares completion — the coordinator owns all the
/// shares and produces the final signature itself.
///
/// In **remote mode**, no such `Signed` message ever arrives (shares for the
/// other devices live on other coordinators). The dispatcher instead
/// completes as soon as every local target device has returned a share via
/// `GotShare`. `finished_signatures` stays `None` in this mode — downstream
/// callers combine signatures out-of-band (see
/// `frostsnap_core::coordinator::remote_signing::combine_signatures`).
pub struct SigningDispatcher {
    pub key_id: KeyId,
    pub session_id: SignSessionId,
    pub finished_signatures: Option<Vec<EncodedSignature>>,
    pub targets: BTreeSet<DeviceId>,
    pub got_signatures: BTreeSet<DeviceId>,
    /// Set at construction time. Typically a `BehaviorBroadcast<SigningState>`
    /// so downstream Dart subscribers get the current snapshot on subscribe
    /// and multiple subscribers fan out cleanly, but any `Sink` will do —
    /// the dispatcher doesn't care what's on the other side.
    pub sink: Box<dyn crate::Sink<SigningState>>,
    /// Abort reason. `Some("<non-empty>")` is a user-visible error
    /// (rendered in the UI); `Some("")` is "silent abort" (session torn
    /// down, no error shown — used when the handle is dropped during
    /// normal page unmount); `None` means not aborted.
    pub aborted: Option<String>,
    pub connected_but_need_request: BTreeSet<DeviceId>,
    pub outbox_to_devices: Vec<CoordinatorSendMessage>,
    /// When true, completion fires as soon as `got_signatures ⊇ targets`
    /// without waiting for a `Signed` message. Set by `new_remote` /
    /// `restore_remote_signing_session`.
    complete_on_all_shares: bool,
    /// Drained at the start of every `poll()` tick. Commands land via the
    /// paired `Sender` held by the `SigningSessionHandle`.
    command_rx: Receiver<DispatcherCommand>,
}

impl SigningDispatcher {
    pub fn new(
        targets: BTreeSet<DeviceId>,
        key_id: KeyId,
        session_id: SignSessionId,
        sink: Box<dyn crate::Sink<SigningState>>,
    ) -> (Self, Sender<DispatcherCommand>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Self {
                targets,
                key_id,
                session_id,
                got_signatures: Default::default(),
                finished_signatures: Default::default(),
                sink,
                aborted: None,
                connected_but_need_request: Default::default(),
                outbox_to_devices: Default::default(),
                complete_on_all_shares: false,
                command_rx: rx,
            },
            tx,
        )
    }

    /// Dispatcher for a remote signing session. Completes on
    /// `got_signatures ⊇ targets` since no `Signed` message will ever arrive.
    pub fn new_remote(
        targets: BTreeSet<DeviceId>,
        key_id: KeyId,
        session_id: SignSessionId,
        got_signatures: BTreeSet<DeviceId>,
        sink: Box<dyn crate::Sink<SigningState>>,
    ) -> (Self, Sender<DispatcherCommand>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Self {
                targets,
                key_id,
                session_id,
                got_signatures,
                finished_signatures: Default::default(),
                sink,
                aborted: None,
                connected_but_need_request: Default::default(),
                outbox_to_devices: Default::default(),
                complete_on_all_shares: true,
                command_rx: rx,
            },
            tx,
        )
    }

    pub fn restore_signing_session(
        active_sign_session: &ActiveSignSession,
        sink: Box<dyn crate::Sink<SigningState>>,
    ) -> (Self, Sender<DispatcherCommand>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (
            Self {
                key_id: active_sign_session.key_id,
                session_id: active_sign_session.session_id(),
                got_signatures: active_sign_session.received_from().collect(),
                targets: active_sign_session
                    .init
                    .local_nonces
                    .keys()
                    .cloned()
                    .collect(),
                finished_signatures: None,
                sink,
                aborted: None,
                connected_but_need_request: Default::default(),
                outbox_to_devices: Default::default(),
                complete_on_all_shares: false,
                command_rx: rx,
            },
            tx,
        )
    }

    pub fn set_signature_received(&mut self, from: DeviceId) {
        self.got_signatures.insert(from);
    }

    pub fn emit_state(&mut self) {
        let state = SigningState {
            session_id: self.session_id,
            got_shares: self.got_signatures.iter().cloned().collect(),
            needed_from: self.targets.iter().cloned().collect(),
            finished_signatures: self.finished_signatures.clone(),
            aborted: self.aborted.clone(),
            connected_but_need_request: self.connected_but_need_request.iter().cloned().collect(),
        };
        self.sink.send(state);
    }

    fn send_sign_request(&mut self, sign_req: RequestDeviceSign) {
        if self.connected_but_need_request.remove(&sign_req.device_id) {
            self.outbox_to_devices.push(
                CoordinatorSend::from(sign_req)
                    .try_into()
                    .expect("sign_req goes to devices"),
            );
            self.emit_state();
        }
    }

    fn drain_commands(&mut self) {
        loop {
            match self.command_rx.try_recv() {
                Ok(DispatcherCommand::SendSignRequest(req)) => self.send_sign_request(*req),
                Ok(DispatcherCommand::Cancel) | Err(TryRecvError::Disconnected) => {
                    // Explicit handle cancel or handle dropped — both are
                    // "silent teardown" from the UI's perspective. Mark the
                    // abort with an empty string so the dispatcher gets
                    // popped via `is_complete` without Dart rendering an
                    // error message. (Non-terminal sessions only; if we're
                    // already done, don't overwrite a Success outcome.)
                    if !self.is_terminal() {
                        self.aborted = Some(String::new());
                        self.emit_state();
                    }
                    return;
                }
                Err(TryRecvError::Empty) => return,
            }
        }
    }

    fn is_terminal(&self) -> bool {
        self.aborted.is_some()
            || self.finished_signatures.is_some()
            || (self.complete_on_all_shares
                && !self.targets.is_empty()
                && self.targets.is_subset(&self.got_signatures))
    }
}

impl UiProtocol for SigningDispatcher {
    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        if let CoordinatorToUserMessage::Signing(message) = message {
            match message {
                CoordinatorToUserSigningMessage::GotShare {
                    from, session_id, ..
                } => {
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
        // Success takes precedence over abort: if shares landed and then a
        // cancel raced in, the session is done — no point in sending
        // cancel bytes out.
        if self.finished_signatures.is_some() {
            return Some(Completion::Success);
        }
        if self.complete_on_all_shares
            && !self.targets.is_empty()
            && self.targets.is_subset(&self.got_signatures)
        {
            return Some(Completion::Success);
        }
        if self.aborted.is_some() {
            return Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            });
        }
        None
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        self.drain_commands();
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
    pub finished_signatures: Option<Vec<EncodedSignature>>,
    pub aborted: Option<String>,
    pub connected_but_need_request: Vec<DeviceId>,
}
