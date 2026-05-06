//! FFI surface for the remote (org) keygen lobby. Thin wrapper around
//! `frostsnap_nostr::keygen::LobbyClient` that exposes a Dart-subscribable
//! state stream and async methods for the full lobby lifecycle:
//! presence → register (mark ready) → start keygen → ack keygen.

use crate::api::broadcast::{BehaviorBroadcast, BehaviorBroadcastSubscription, StartError};
use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::Sink;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::DeviceId;
use frostsnap_nostr::channel::ChannelKeys;
pub use frostsnap_nostr::keygen::{
    DeviceKind, DeviceRegistration, LobbyState, ParticipantInfo, ParticipantStatus, ResolvedKeygen,
    SelectedCoordinator, SelectedParticipant,
};
use frostsnap_nostr::keygen::{LobbyEvent, LobbyHandle};
use frostsnap_nostr::{Client, EventId, Keys, PublicKey};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

// ============================================================================
// FRB-mirrored types from frostsnap_nostr::keygen
// ============================================================================

#[frb(mirror(DeviceKind), non_opaque)]
pub enum _DeviceKind {
    Frostsnap,
    AppKey,
}

#[frb(mirror(DeviceRegistration), non_opaque)]
pub struct _DeviceRegistration {
    pub device_id: DeviceId,
    pub name: String,
    pub kind: DeviceKind,
}

/// Mirrors `frostsnap_nostr::keygen::ParticipantStatus`.
#[frb(mirror(ParticipantStatus), non_opaque)]
pub enum _ParticipantStatus {
    Joining,
    Ready,
}

/// Mirrors `frostsnap_nostr::keygen::ParticipantInfo`. The `is_initiator`
/// projection that used to live on the FFI wrapper is now derived
/// Dart-side via `participant.pubkey.equals(state.initiator)`.
#[frb(mirror(ParticipantInfo), non_opaque)]
pub struct _ParticipantInfo {
    pub pubkey: PublicKey,
    pub status: ParticipantStatus,
    pub devices: Vec<DeviceRegistration>,
    pub register_event_id: Option<EventId>,
}

/// Mirrors `frostsnap_nostr::keygen::ResolvedKeygen`. Carries the
/// public information about an in-flight keygen — selected
/// participants (in StartKeygen e-tag order), threshold, key name,
/// purpose, and the running ack set. Methods exposed via
/// `#[frb(external)]` below.
#[frb(mirror(SelectedParticipant), non_opaque)]
pub struct _SelectedParticipant {
    pub pubkey: PublicKey,
    pub devices: Vec<DeviceRegistration>,
}

/// Mirrors `frostsnap_nostr::keygen::ResolvedKeygen` as an **opaque**
/// handle. We deliberately don't list fields here — non-opaque value
/// mirrors of structs containing opaque fields (e.g. `purpose:
/// KeyPurpose`) trigger the by-value-encode disposal trap when any
/// method serialises `&self`. Opaque mirroring sidesteps that: `&self`
/// encodes as a handle clone, and the inherent helper methods on the
/// source struct (`includes`, `all_acked`) are exposed via
/// `#[frb(external)]`. Field accessors that Dart needs are
/// synthesised via the `ResolvedKeygenExt` trait below.
#[frb(mirror(ResolvedKeygen), opaque)]
pub struct _ResolvedKeygen {}

#[frb(external)]
impl ResolvedKeygen {
    #[frb(sync)]
    pub fn includes(&self, _pubkey: &PublicKey) -> bool {}

    #[frb(sync)]
    pub fn all_acked(&self) -> bool {}
}

pub trait ResolvedKeygenExt {
    #[frb(sync, getter)]
    fn keygen_event_id(&self) -> EventId;

    #[frb(sync, getter)]
    fn threshold(&self) -> u16;

    #[frb(sync, getter)]
    fn participants(&self) -> Vec<SelectedParticipant>;

    #[frb(sync, getter)]
    fn acked(&self) -> Vec<PublicKey>;
}

impl ResolvedKeygenExt for ResolvedKeygen {
    #[frb(sync, getter)]
    fn keygen_event_id(&self) -> EventId {
        self.keygen_event_id
    }

    #[frb(sync, getter)]
    fn threshold(&self) -> u16 {
        self.threshold
    }

    #[frb(sync, getter)]
    fn participants(&self) -> Vec<SelectedParticipant> {
        self.participants.clone()
    }

    #[frb(sync, getter)]
    fn acked(&self) -> Vec<PublicKey> {
        self.acked.clone()
    }
}

/// Mirrors `frostsnap_nostr::keygen::LobbyState`. The `cancelled`
/// latch is part of the state itself (flipped by `process_event`).
/// Same reasoning as `ResolvedKeygen` above for the Dart-emitted
/// helpers.
#[frb(
    mirror(LobbyState),
    non_opaque,
    dart_code = "
  bool allReady() =>
      participants.isNotEmpty &&
      participants.values.every((p) => p.status == ParticipantStatus.ready);

  int totalDeviceCount() => participants.values
      .fold(0, (sum, p) => sum + p.devices.length);
"
)]
pub struct _LobbyState {
    pub initiator: Option<PublicKey>,
    pub key_name: Option<String>,
    pub purpose: Option<KeyPurpose>,
    pub participants: std::collections::HashMap<PublicKey, ParticipantInfo>,
    pub keygen: Option<ResolvedKeygen>,
    pub cancelled: bool,
}

#[frb(mirror(SelectedCoordinator), non_opaque)]
pub struct _SelectedCoordinator {
    pub register_event_id: EventId,
    pub pubkey: PublicKey,
}

// ============================================================================
// Sink: LobbyEvent → BehaviorBroadcast<LobbyState>
// ============================================================================

/// Encrypted-subchannel keys + the resolved keygen, captured from
/// `LobbyEvent::KeygenResolved` so `await_keygen_ready` can hand them off
/// without exposing `ChannelKeys` to Dart.
#[frb(ignore)]
#[derive(Clone)]
pub(crate) struct SessionInit {
    pub resolved: ResolvedKeygen,
    pub channel_keys: ChannelKeys,
}

#[derive(Clone)]
struct LobbyBridgeSink {
    broadcast: BehaviorBroadcast<LobbyState>,
    session_init: Arc<Mutex<Option<SessionInit>>>,
    state_changed: Arc<Notify>,
}

impl Sink<LobbyEvent> for LobbyBridgeSink {
    fn send(&self, event: LobbyEvent) {
        match event {
            LobbyEvent::LobbyChanged(state) => {
                self.broadcast.add(&state);
                self.state_changed.notify_waiters();
            }
            LobbyEvent::KeygenResolved {
                resolved,
                channel_keys,
            } => {
                *self.session_init.lock().unwrap() = Some(SessionInit {
                    resolved,
                    channel_keys,
                });
                self.state_changed.notify_waiters();
            }
            LobbyEvent::AllAcked => {
                // The latest `LobbyChanged` already carried the
                // fully-acked state; the notification below wakes any
                // `await_keygen_ready` future so it can re-check.
                self.state_changed.notify_waiters();
            }
            LobbyEvent::Cancelled => {
                // The publishing path (`process_event`) already set
                // `LobbyState.cancelled = true` and emitted the
                // accompanying `LobbyChanged`. Nothing extra needed
                // here — but if no `LobbyChanged` ever fired (e.g.
                // `CancelLobby` arriving with no participant change),
                // synthesise a final snapshot so Dart sees the latch.
                if let Some(mut snapshot) = self.broadcast.latest() {
                    if !snapshot.cancelled {
                        snapshot.cancelled = true;
                        self.broadcast.add(&snapshot);
                    }
                }
                self.state_changed.notify_waiters();
            }
        }
    }
}

// ============================================================================
// KeygenStartArgs — handoff from lobby to `Coordinator::run_remote_keygen`
// ============================================================================

/// Bundle of everything `Coordinator::run_remote_keygen` needs to drive the
/// ceremony: the resolved keygen, the encrypted-subchannel keys, this
/// participant's nostr `Keys`, and the nostr `Client` used to subscribe to
/// the protocol relay. Opaque to Dart — secrets stay inside Rust.
#[frb(opaque)]
pub struct KeygenStartArgs {
    pub(crate) keys: Keys,
    pub(crate) resolved: ResolvedKeygen,
    pub(crate) channel_keys: ChannelKeys,
    pub(crate) client: Client,
}

// ============================================================================
// RemoteLobbyHandle
// ============================================================================

/// Opaque handle returned by `NostrClient::{create,join}_remote_lobby`.
/// Drives the lobby round: state subscription plus the async methods
/// for presence → mark ready → start keygen → ack keygen.
#[frb(opaque)]
pub struct RemoteLobbyHandle {
    handle: LobbyHandle,
    keys: Keys,
    invite_link: String,
    state_broadcast: BehaviorBroadcast<LobbyState>,
    client: Client,
    session_init: Arc<Mutex<Option<SessionInit>>>,
    state_changed: Arc<Notify>,
}

/// Internal bundle returned by [`RemoteLobbyHandle::build_bridge`] alongside
/// the bridge sink, so the caller can construct the handle with matching
/// `Arc`s.
#[frb(ignore)]
pub(crate) struct LobbyBridge {
    pub broadcast: BehaviorBroadcast<LobbyState>,
    pub session_init: Arc<Mutex<Option<SessionInit>>>,
    pub state_changed: Arc<Notify>,
}

impl RemoteLobbyHandle {
    pub(crate) fn new(
        handle: LobbyHandle,
        keys: Keys,
        invite_link: String,
        client: Client,
        bridge: LobbyBridge,
    ) -> Self {
        Self {
            handle,
            keys,
            invite_link,
            state_broadcast: bridge.broadcast,
            client,
            session_init: bridge.session_init,
            state_changed: bridge.state_changed,
        }
    }

    /// Build the bridging sink plus the shared state it feeds. The caller
    /// (`NostrClient::{create,join}_remote_lobby`) passes the sink into
    /// `LobbyClient::run` and hands the bridge back to `new`.
    pub(crate) fn build_bridge() -> (LobbyBridge, impl Sink<LobbyEvent> + Clone) {
        let broadcast = BehaviorBroadcast::<LobbyState>::default();
        let session_init: Arc<Mutex<Option<SessionInit>>> = Default::default();
        let state_changed: Arc<Notify> = Arc::new(Notify::new());
        let sink = LobbyBridgeSink {
            broadcast: broadcast.clone(),
            session_init: session_init.clone(),
            state_changed: state_changed.clone(),
        };
        (
            LobbyBridge {
                broadcast,
                session_init,
                state_changed,
            },
            sink,
        )
    }

    #[frb(sync)]
    pub fn invite_link(&self) -> String {
        self.invite_link.clone()
    }

    #[frb(sync)]
    pub fn my_pubkey(&self) -> PublicKey {
        self.keys.public_key().into()
    }

    /// Subscribe to `LobbyState` updates. Fresh subscribers receive
    /// the cached current snapshot immediately on `start()`.
    #[frb(sync)]
    pub fn sub_state(&self) -> LobbyStateBroadcastSubscription {
        LobbyStateBroadcastSubscription(self.state_broadcast.subscribe())
    }

    /// Publish an immediate presence ping. `LobbyClient::run` already
    /// fires one on start and then every heartbeat tick, so this is
    /// mostly for manual resync.
    pub async fn announce_presence(&self) -> Result<()> {
        self.handle.announce_presence(&self.keys).await?;
        Ok(())
    }

    /// Commit a device set ("Continue with N devices" in the mockup).
    /// Re-callable — each call supersedes the prior commitment.
    pub async fn mark_ready(&self, devices: Vec<DeviceRegistration>) -> Result<()> {
        let outcome = self.handle.register_devices(&self.keys, devices).await?;
        if !outcome.any_relay_success() {
            return Err(anyhow!(
                "no relay accepted the registration: {:?}",
                outcome.relay_failed
            ));
        }
        Ok(())
    }

    /// Publish `Leave` so other participants remove us. Idempotent.
    pub async fn leave(&self) -> Result<()> {
        self.handle.leave(&self.keys).await?;
        Ok(())
    }

    /// Initiator-only. Aborts the lobby for everyone.
    pub async fn cancel(&self) -> Result<()> {
        self.handle.cancel_lobby(&self.keys).await?;
        Ok(())
    }

    /// Selected participants only. Publishes `AckKeygen` referencing
    /// the given `StartKeygen` event id. Dart owns the event id —
    /// it's surfaced via `LobbyState.keygen.keygen_event_id`.
    pub async fn ack_keygen(&self, start_keygen_event_id: EventId) -> Result<()> {
        let outcome = self
            .handle
            .ack_keygen(&self.keys, start_keygen_event_id)
            .await?;
        if !outcome.any_relay_success() {
            return Err(anyhow!(
                "no relay accepted AckKeygen: {:?}",
                outcome.relay_failed
            ));
        }
        Ok(())
    }

    /// Host-only. Publishes `StartKeygen` with the given threshold +
    /// selected participants. Dart is authoritative on participant
    /// selection and passes the set in here. Key name + purpose are
    /// already known to every party via the channel-creation event,
    /// so they don't need to cross the wire (or the FFI) again.
    pub async fn start_keygen(
        &self,
        threshold: u16,
        selected: Vec<SelectedCoordinator>,
    ) -> Result<()> {
        let outcome = self
            .handle
            .start_keygen(&self.keys, &selected, threshold)
            .await?;
        if !outcome.any_relay_success() {
            return Err(anyhow!(
                "no relay accepted StartKeygen: {:?}",
                outcome.relay_failed
            ));
        }
        Ok(())
    }

    /// Resolves once the lobby observes `AllAcked` with our pubkey in `acked`
    /// (i.e. the ceremony is ready to start for us). Returns the bundle Dart
    /// hands off to `Coordinator::run_remote_keygen`. After this returns, the
    /// lobby's job is done — Dart can drop the handle.
    pub async fn await_keygen_ready(&self) -> Result<KeygenStartArgs> {
        let my_pubkey: PublicKey = self.keys.public_key().into();
        loop {
            // Arm the notification before reading state to avoid the TOCTOU
            // race where a state change lands between our check and our wait.
            let notified = self.state_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if let Some(state) = self.state_broadcast.latest() {
                if state.cancelled {
                    return Err(anyhow!("lobby was cancelled"));
                }
                if let Some(resolved) = state.keygen.as_ref() {
                    if resolved.all_acked() && resolved.acked.contains(&my_pubkey) {
                        let init = self.session_init.lock().unwrap().clone().ok_or_else(|| {
                            anyhow!("AllAcked observed without KeygenResolved session_init")
                        })?;
                        return Ok(KeygenStartArgs {
                            keys: self.keys.clone(),
                            resolved: init.resolved,
                            channel_keys: init.channel_keys,
                            client: self.client.clone(),
                        });
                    }
                }
            }
            notified.await;
        }
    }
}

// ============================================================================
// FRB subscription wrapper (generics don't cross FFI; concrete wrapper needed)
// ============================================================================

pub struct LobbyStateBroadcastSubscription(pub(crate) BehaviorBroadcastSubscription<LobbyState>);

impl LobbyStateBroadcastSubscription {
    #[frb(sync)]
    pub fn id(&self) -> u32 {
        self.0._id()
    }

    #[frb(sync)]
    pub fn is_running(&self) -> bool {
        self.0._is_running()
    }

    #[frb(sync)]
    pub fn start(&self, sink: StreamSink<LobbyState>) -> std::result::Result<(), StartError> {
        self.0._start(sink)
    }

    #[frb(sync)]
    pub fn stop(&self) -> bool {
        self.0._stop()
    }
}
