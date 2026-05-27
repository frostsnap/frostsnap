//! `Coordinator::start_remote_keygen` and the `RemoteKeygenSessionHandle`
//! that drives a remote keygen ceremony.
//!
//! Mirrors `SigningSessionHandle`: the handle owns the cancel signal + the
//! state broadcast + a command channel. The actual ceremony runs in a
//! `tokio::spawn`'d task that owns clones of the FfiCoordinator sub-Arcs it
//! needs. **All teardown — finalize-success or any abort cause — converges
//! on a single cleanup block at the end of the run loop.**
//!
//! Cancel triggers in priority order:
//! 1. `handle.cancel()` from Dart fires the cancel token.
//! 2. `RemoteKeyGen::disconnected(local_device_id)` fires the same token via
//!    the `on_local_disconnect` callback set up at construction.
//! 3. Dropping the handle fires the token via its `Drop` impl.
//! 4. `handle.confirm_match(...)` finalizes via the command channel; the
//!    loop exits its select branch normally with `finalized = true` so the
//!    cleanup skips the abort path.

use crate::api::broadcast::BehaviorBroadcast;
use crate::coordinator::RemoteBroadcastRegistration;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::frostsnap_comms::{
    CoordinatorSendBody, CoordinatorSendMessage, Destination,
};
pub use frostsnap_coordinator::keygen::KeyGenState;
use frostsnap_coordinator::remote_keygen::RemoteKeyGen;
use frostsnap_coordinator::{Sink, UiProtocol};
use frostsnap_core::coordinator::remote_keygen::{RemoteKeygenMessage, RemoteKeygenPayload};
use frostsnap_core::coordinator::{BroadcastPayload, CoordinatorSend};
use frostsnap_core::schnorr_fun::fun::{KeyPair, Scalar};
use frostsnap_core::{AccessStructureRef, DeviceId, KeygenId, SymmetricKey};
use frostsnap_macros::broadcast_handle;
use frostsnap_nostr::keygen::{ProtocolClient, SelectedParticipant};
use frostsnap_nostr::{ChannelParticipant, Keys};
use std::collections::BTreeSet;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{event, Level};

use super::remote_keygen::KeygenStartArgs;
use crate::api::coordinator::Coordinator;
use crate::coordinator::FfiCoordinator;

// ============================================================================
// Wrappers
// ============================================================================

/// Sink adapter: pushes inbound nostr `RemoteKeygenMessage`s into an mpsc.
#[derive(Clone)]
struct MpscKeygenSink(mpsc::UnboundedSender<RemoteKeygenMessage>);

impl Sink<RemoteKeygenMessage> for MpscKeygenSink {
    fn send(&self, msg: RemoteKeygenMessage) {
        let _ = self.0.send(msg);
    }
}

#[frb(non_opaque)]
pub struct RemoteKeygenResult {
    pub access_structure_ref: AccessStructureRef,
    pub participants: Vec<ChannelParticipant>,
}

// ============================================================================
// Session handle
// ============================================================================

broadcast_handle! { pub struct KeyGenStateBcast(pub BehaviorBroadcast<KeyGenState>); }

/// Opaque handle to an in-flight remote keygen session.
///
/// Returned by [`Coordinator::start_remote_keygen`]. Owns the cancel signal,
/// the state broadcast (so multiple Dart subscribers can fan out), and the
/// command channel into the spawned ceremony task.
///
/// All operations that affect a specific session — subscribing to state,
/// confirming the security-code match, cancelling — go through this handle.
/// Dropping it fires the cancel token, which makes the spawned task run its
/// central cleanup and exit cleanly.
#[frb(opaque)]
pub struct RemoteKeygenSessionHandle {
    keygen_id: KeygenId,
    state_broadcast: BehaviorBroadcast<KeyGenState>,
    command_tx: mpsc::UnboundedSender<SessionCommand>,
    cancel: CancellationToken,
}

enum SessionCommand {
    ConfirmMatch {
        encryption_key: SymmetricKey,
        reply: oneshot::Sender<Result<RemoteKeygenResult>>,
    },
}

impl RemoteKeygenSessionHandle {
    #[frb(sync)]
    pub fn keygen_id(&self) -> KeygenId {
        self.keygen_id
    }

    /// Subscribe to `KeyGenState` updates. Fan-out: multiple subscribers
    /// supported. Each fresh `.watch().listen()` immediately emits the
    /// cached current state before streaming further updates.
    #[frb(sync)]
    pub fn sub_state(&self) -> KeyGenStateBcast {
        KeyGenStateBcast::new(self.state_broadcast.clone())
    }

    /// Called when the user confirms the security code matches across all
    /// devices. Drives the run loop's `ConfirmMatch` command branch:
    /// finalizes core state, sends `Keygen::Finalize` to local devices via
    /// USB, marks the UiProtocol finalized so `state.finished` propagates to
    /// Dart, then breaks the run loop. Cleanup runs but skips the abort path.
    pub async fn confirm_match(&self, encryption_key: SymmetricKey) -> Result<RemoteKeygenResult> {
        let (reply, rx) = oneshot::channel();
        self.command_tx
            .send(SessionCommand::ConfirmMatch {
                encryption_key,
                reply,
            })
            .map_err(|_| anyhow!("remote keygen session has shut down"))?;
        rx.await
            .map_err(|_| anyhow!("remote keygen session shut down before reply"))?
    }

    /// Cancel this session. Fires the cancel token; the spawned task wakes
    /// up on its cancel branch and runs the central cleanup (clear core
    /// state, USB-cancel local devices, abort the UiProtocol, clear the
    /// outbound slot). Local-only — no protocol message published.
    #[frb(sync)]
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for RemoteKeygenSessionHandle {
    fn drop(&mut self) {
        // If Dart drops the handle without calling cancel/confirm, the
        // spawned task still needs to know to clean up. The token is
        // idempotent — ok to fire here even if cancel/confirm already did.
        self.cancel.cancel();
    }
}

// ============================================================================
// RemoteKeygenSession — cloned FfiCoordinator + session-local data
// ============================================================================

struct RemoteKeygenSession {
    coord: FfiCoordinator,
    keygen_id: KeygenId,
    keys: Keys,
    local_devices: BTreeSet<DeviceId>,
    participants: Vec<SelectedParticipant>,
}

impl RemoteKeygenSession {
    fn drain_outgoing(
        &self,
        outbound_tx: &mpsc::UnboundedSender<(DeviceId, RemoteKeygenPayload)>,
        outgoing: Vec<CoordinatorSend>,
    ) {
        for m in outgoing {
            match m {
                CoordinatorSend::ToDevice {
                    message,
                    destinations,
                } => {
                    self.coord.usb_sender.send(CoordinatorSendMessage {
                        target_destinations: Destination::from(destinations),
                        message_body: CoordinatorSendBody::Core(message),
                    });
                }
                CoordinatorSend::ToUser(m) => {
                    self.coord.ui_stack.lock().unwrap().process_to_user_message(m);
                }
                CoordinatorSend::Broadcast {
                    from,
                    payload: BroadcastPayload::RemoteKeygen(payload),
                    ..
                } => {
                    let _ = outbound_tx.send((from, payload));
                }
            }
        }
    }

    /// Run the finalize via FfiCoordinator helper + build RemoteKeygenResult.
    fn finalize(&self, encryption_key: SymmetricKey) -> Result<RemoteKeygenResult> {
        let (access_structure_ref, device_to_share_index) = self
            .coord
            .finalize_remote_keygen_with_side_effects(self.keygen_id, encryption_key)?;

        let participants = self
            .participants
            .iter()
            .map(|p| {
                let share_indices = p
                    .devices
                    .iter()
                    .map(|d| {
                        let si = device_to_share_index[&d.device_id];
                        u32::try_from(si).expect("share index fits u32")
                    })
                    .collect();
                ChannelParticipant {
                    pubkey: p.pubkey,
                    share_indices,
                }
            })
            .collect();

        Ok(RemoteKeygenResult {
            access_structure_ref,
            participants,
        })
    }

    /// Central cleanup — runs whether we exited via cancel, disconnect,
    /// finalize, or handle-drop. The `finalized` flag distinguishes the
    /// successful-finalize path (no abort needed) from any abort path.
    fn cleanup(&self, finalized: bool) {
        if !finalized {
            self.coord
                .coordinator
                .lock()
                .unwrap()
                .MUTATE_NO_PERSIST()
                .cancel_remote_keygen(self.keygen_id);

            for d in &self.local_devices {
                self.coord.usb_sender.send_cancel(*d);
            }

            if let Some(kg) = self.coord.ui_stack.lock().unwrap().get_mut::<RemoteKeyGen>() {
                kg.cancel();
            }
        }

        let mut slot = self.coord.active_remote_broadcast.lock().unwrap();
        if matches!(&*slot, Some(reg) if reg.keygen_id == self.keygen_id) {
            *slot = None;
        }
    }
}

// ============================================================================
// Coordinator entry point
// ============================================================================

impl Coordinator {
    /// Start a remote keygen ceremony. Awaits `ProtocolClient::run` to bring
    /// up the protocol channel, then `tokio::spawn`s the bidi run loop and
    /// returns a handle Dart can use to subscribe to state, confirm the
    /// match, or cancel.
    pub async fn start_remote_keygen(
        &self,
        args: KeygenStartArgs,
    ) -> Result<RemoteKeygenSessionHandle> {
        let KeygenStartArgs {
            keys,
            resolved,
            channel_keys,
            client,
        } = args;
        let keygen_id = KeygenId(resolved.keygen_event_id.to_bytes());

        // This participant's local device set, looked up from the resolved
        // keygen by our nostr pubkey.
        let my_pubkey = keys.public_key().into();
        let local_devices: BTreeSet<DeviceId> = resolved
            .participants
            .iter()
            .find(|p| p.pubkey == my_pubkey)
            .ok_or_else(|| anyhow!("self not in resolved.participants"))?
            .devices
            .iter()
            .map(|d| d.device_id)
            .collect();

        // Two mpscs internal to the bidi loop.
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel::<SessionCommand>();
        let cancel = CancellationToken::new();

        // Register the outbound slot so the sync USB loop's Broadcasts
        // (produced when a local USB device responds to a Keygen::* message)
        // are routed into the same outbound mpsc.
        *self.0.active_remote_broadcast.lock().unwrap() = Some(RemoteBroadcastRegistration {
            keygen_id,
            tx: outbound_tx.clone(),
            cancel: cancel.clone(),
        });

        // Push the RemoteKeyGen UiProtocol. The on_local_disconnect closure
        // fires our cancel token — the bidi loop's central cleanup then
        // takes over (idempotent against any other cancel trigger).
        // RemoteKeyGen::emit_state() is called immediately below; the
        // seeded placeholder is overwritten before the broadcast is
        // observed by any subscriber.
        let state_broadcast = BehaviorBroadcast::seeded(KeyGenState::default());
        let on_local_disconnect = {
            let cancel = cancel.clone();
            move || cancel.cancel()
        };
        let ui_proto = RemoteKeyGen::new(
            ForwardingSink::new(state_broadcast.clone()),
            keygen_id,
            resolved.threshold,
            resolved.devices_in_order(),
            local_devices.clone(),
            on_local_disconnect,
        );
        ui_proto.emit_state();
        self.0.start_protocol(ui_proto);

        // Spin up the nostr-side bidi task. Its internal loop feeds
        // `inbound_tx`; we use `protocol_handle.send_keygen_payload` for
        // outbound.
        let protocol_handle = ProtocolClient::run(
            client,
            channel_keys,
            resolved.keygen_event_id,
            resolved.allowed_senders(),
            MpscKeygenSink(inbound_tx),
        )
        .await?;

        // Derive this participant's keygen keypair from their nostr secret.
        // Matches `keygen_live.rs:101-105` so coordinator_ids() (also
        // derived from nostr pubkeys) lines up with the keypair's device id
        // across all participants.
        let scalar = Scalar::from_bytes(keys.secret_key().secret_bytes())
            .ok_or_else(|| anyhow!("nostr secret key not a valid scalar"))?
            .non_zero()
            .ok_or_else(|| anyhow!("nostr secret key is zero"))?;
        let keypair = KeyPair::new_xonly(scalar).into();
        let coordinator_ids = resolved.coordinator_ids();

        // Kick off the ceremony.
        let initial: Vec<CoordinatorSend> = {
            let mut coord = self.0.coordinator.lock().unwrap();
            coord
                .MUTATE_NO_PERSIST()
                .begin_remote_keygen(
                    resolved.to_begin_keygen(),
                    &coordinator_ids,
                    &local_devices,
                    keypair,
                    &mut rand::thread_rng(),
                )?
                .into_iter()
                .collect()
        };

        let ctx = RemoteKeygenSession {
            coord: self.0.clone(),
            keygen_id,
            keys: keys.clone(),
            local_devices,
            participants: resolved.participants.clone(),
        };
        ctx.drain_outgoing(&outbound_tx, initial);

        let cancel_for_task = cancel.clone();
        tokio::spawn(run_session(
            ctx,
            protocol_handle,
            inbound_rx,
            outbound_rx,
            outbound_tx,
            command_rx,
            cancel_for_task,
        ));

        Ok(RemoteKeygenSessionHandle {
            keygen_id,
            state_broadcast,
            command_tx,
            cancel,
        })
    }
}

async fn run_session(
    ctx: RemoteKeygenSession,
    protocol_handle: frostsnap_nostr::keygen::ProtocolHandle,
    mut inbound_rx: mpsc::UnboundedReceiver<RemoteKeygenMessage>,
    mut outbound_rx: mpsc::UnboundedReceiver<(DeviceId, RemoteKeygenPayload)>,
    outbound_tx: mpsc::UnboundedSender<(DeviceId, RemoteKeygenPayload)>,
    mut command_rx: mpsc::UnboundedReceiver<SessionCommand>,
    cancel: CancellationToken,
) {
    let mut finalized = false;

    loop {
        tokio::select! {
            // Cancel takes priority — if both an inbound message and a
            // cancel signal are pending, exit instead of processing.
            biased;

            _ = cancel.cancelled() => break,

            cmd = command_rx.recv() => match cmd {
                Some(SessionCommand::ConfirmMatch { encryption_key, reply }) => {
                    let result = ctx.finalize(encryption_key);
                    finalized = result.is_ok();
                    let _ = reply.send(result);
                    break;
                }
                None => break, // handle dropped
            },

            Some(msg) = inbound_rx.recv() => {
                let outgoing: Vec<CoordinatorSend> = {
                    let mut coord = ctx.coord.coordinator.lock().unwrap();
                    match coord
                        .MUTATE_NO_PERSIST()
                        .apply_keygen_message(ctx.keygen_id, msg)
                    {
                        Ok(v) => v,
                        Err(e) => {
                            event!(Level::ERROR, error = %e, "apply_keygen_message failed");
                            Vec::new()
                        }
                    }
                };
                ctx.drain_outgoing(&outbound_tx, outgoing);
            }

            Some((from, payload)) = outbound_rx.recv() => {
                if let Err(e) = protocol_handle
                    .send_keygen_payload(&ctx.keys, from, payload)
                    .await
                {
                    event!(Level::ERROR, error = %e, "send_keygen_payload failed");
                }
            }
        }
    }

    ctx.cleanup(finalized);
}

// ============================================================================
// ForwardingSink — Sink<KeyGenState> wrapper around BehaviorBroadcast
// ============================================================================
//
// `BehaviorBroadcast` is FFI-aware (its subscribers are `StreamSink`s); the
// `Sink<T>` trait `RemoteKeyGen` expects is the simpler in-process trait from
// `frostsnap_coordinator`. This thin wrapper bridges them.

#[derive(Clone)]
struct ForwardingSink {
    inner: BehaviorBroadcast<KeyGenState>,
}

impl ForwardingSink {
    fn new(inner: BehaviorBroadcast<KeyGenState>) -> Self {
        Self { inner }
    }
}

impl Sink<KeyGenState> for ForwardingSink {
    fn send(&self, value: KeyGenState) {
        self.inner.add(&value);
    }
}
