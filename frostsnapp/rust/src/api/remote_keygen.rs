//! FFI surface for the remote (org) keygen lobby. Thin wrapper around
//! `frostsnap_nostr::keygen::LobbyClient` that exposes a Dart-subscribable
//! state stream and async methods for the full lobby lifecycle:
//! presence → register (mark ready) → set/accept threshold → start keygen
//! (still a TODO stub — the handoff into core keygen is a later slice).

use crate::api::broadcast::{BehaviorBroadcast, BehaviorBroadcastSubscription, StartError};
use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::Sink;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::DeviceId;
pub use frostsnap_nostr::keygen::{DeviceKind, DeviceRegistration};
use frostsnap_nostr::keygen::{
    LobbyEvent, LobbyHandle, LobbyState, ParticipantStatus as CoreParticipantStatus,
};
use frostsnap_nostr::{Keys, PublicKey};
use std::sync::{Arc, Mutex};

// ============================================================================
// FRB-mirrored types from frostsnap_nostr
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

// ============================================================================
// FFI lobby state (flat, Dart-friendly snapshot of LobbyState)
// ============================================================================

#[derive(Clone, Debug)]
pub enum FfiParticipantStatus {
    Joining,
    Ready,
    Accepted,
}

impl From<CoreParticipantStatus> for FfiParticipantStatus {
    fn from(s: CoreParticipantStatus) -> Self {
        match s {
            CoreParticipantStatus::Joining => Self::Joining,
            CoreParticipantStatus::Ready => Self::Ready,
            CoreParticipantStatus::Accepted => Self::Accepted,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FfiLobbyDevice {
    pub device_id: DeviceId,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct FfiLobbyParticipant {
    pub pubkey: PublicKey,
    pub status: FfiParticipantStatus,
    /// Empty while `status == Joining`. Populated once `Register` lands.
    pub devices: Vec<FfiLobbyDevice>,
    pub is_initiator: bool,
}

#[derive(Clone, Debug)]
pub struct FfiLobbyState {
    pub initiator: Option<PublicKey>,
    pub key_name: Option<String>,
    pub purpose: Option<KeyPurpose>,
    pub participants: Vec<FfiLobbyParticipant>,
    /// Currently-proposed threshold (cleared on re-propose).
    pub threshold: Option<u16>,
    /// True when every participant has at least `Ready` status.
    pub all_ready: bool,
    /// True when every participant has `Accepted` the current threshold.
    pub all_accepted: bool,
    /// `true` once the initiator has published `CancelLobby`. Latched —
    /// remains true for the life of the handle.
    pub cancelled: bool,
}

impl FfiLobbyState {
    fn from_lobby(state: &LobbyState) -> Self {
        let participants = state
            .participants
            .values()
            .map(|p| FfiLobbyParticipant {
                pubkey: p.pubkey,
                status: p.status.into(),
                devices: p
                    .commitment
                    .as_ref()
                    .map(|c| {
                        c.devices
                            .iter()
                            .map(|d| FfiLobbyDevice {
                                device_id: d.device_id,
                                name: d.name.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                is_initiator: state.initiator.as_ref() == Some(&p.pubkey),
            })
            .collect();

        Self {
            initiator: state.initiator,
            key_name: state.key_name.clone(),
            purpose: state.purpose,
            participants,
            threshold: state.threshold,
            all_ready: state.all_ready(),
            all_accepted: state.all_accepted(),
            cancelled: false,
        }
    }

    fn empty() -> Self {
        Self {
            initiator: None,
            key_name: None,
            purpose: None,
            participants: vec![],
            threshold: None,
            all_ready: false,
            all_accepted: false,
            cancelled: false,
        }
    }
}

// ============================================================================
// Sink that bridges LobbyEvent → BehaviorBroadcast<FfiLobbyState>
// ============================================================================

/// Holds a clone of the broadcast + a `cancelled` latch that survives
/// across subsequent `LobbyChanged` events (state from the nostr layer
/// never flips `cancelled` back on its own).
#[derive(Clone)]
struct LobbyBridgeSink {
    broadcast: BehaviorBroadcast<FfiLobbyState>,
    cancelled: Arc<Mutex<bool>>,
}

impl Sink<LobbyEvent> for LobbyBridgeSink {
    fn send(&self, event: LobbyEvent) {
        match event {
            LobbyEvent::LobbyChanged(state) => {
                let mut snapshot = FfiLobbyState::from_lobby(&state);
                if *self.cancelled.lock().unwrap() {
                    snapshot.cancelled = true;
                }
                self.broadcast.add(&snapshot);
            }
            LobbyEvent::KeygenResolved { .. } => {
                // The DKG subchannel is out of scope for this slice.
            }
            LobbyEvent::Cancelled => {
                *self.cancelled.lock().unwrap() = true;
                let mut snapshot = self.broadcast.latest().unwrap_or_else(FfiLobbyState::empty);
                snapshot.cancelled = true;
                self.broadcast.add(&snapshot);
            }
        }
    }
}

// ============================================================================
// RemoteLobbyHandle
// ============================================================================

/// Opaque handle returned by `NostrClient::{create,join}_remote_lobby`.
/// Drives the lobby round: state subscription plus the async methods
/// for presence → ready → threshold → accept → start_keygen (stub).
#[frb(opaque)]
pub struct RemoteLobbyHandle {
    handle: LobbyHandle,
    keys: Keys,
    invite_link: String,
    state_broadcast: BehaviorBroadcast<FfiLobbyState>,
}

impl RemoteLobbyHandle {
    pub(crate) fn new(
        handle: LobbyHandle,
        keys: Keys,
        invite_link: String,
        state_broadcast: BehaviorBroadcast<FfiLobbyState>,
    ) -> Self {
        Self {
            handle,
            keys,
            invite_link,
            state_broadcast,
        }
    }

    /// Build the bridging sink plus the broadcast it feeds. The caller
    /// (`NostrClient::{create,join}_remote_lobby`) passes the sink into
    /// `LobbyClient::run` and hands the broadcast back to `new`.
    pub(crate) fn build_bridge() -> (
        BehaviorBroadcast<FfiLobbyState>,
        impl Sink<LobbyEvent> + Clone,
    ) {
        let broadcast = BehaviorBroadcast::<FfiLobbyState>::default();
        let sink = LobbyBridgeSink {
            broadcast: broadcast.clone(),
            cancelled: Arc::new(Mutex::new(false)),
        };
        (broadcast, sink)
    }

    #[frb(sync)]
    pub fn invite_link(&self) -> String {
        self.invite_link.clone()
    }

    #[frb(sync)]
    pub fn my_pubkey(&self) -> PublicKey {
        self.keys.public_key()
    }

    /// Subscribe to `FfiLobbyState` updates. Fresh subscribers receive
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
    /// Re-callable — each call supersedes the prior commitment and
    /// invalidates the sender's acceptance of the current threshold.
    pub async fn mark_ready(&self, devices: Vec<DeviceRegistration>) -> Result<()> {
        self.handle.register_devices(&self.keys, devices).await?;
        Ok(())
    }

    /// Host-only. Publish the wallet name + purpose once, immediately
    /// after creating the lobby. Re-sending is a receiver-side no-op.
    pub async fn set_key_name(&self, key_name: String, purpose: KeyPurpose) -> Result<()> {
        self.handle
            .set_key_name(&self.keys, key_name, purpose)
            .await?;
        Ok(())
    }

    /// Host-only. Propose a threshold. Re-sending clears all prior
    /// acceptances — participants have to re-accept.
    pub async fn set_threshold(&self, threshold: u16) -> Result<()> {
        self.handle.set_threshold(&self.keys, threshold).await?;
        Ok(())
    }

    /// Participant accepts the currently-proposed threshold. Callable
    /// only when `FfiLobbyState.threshold` matches.
    pub async fn accept_threshold(&self, threshold: u16) -> Result<()> {
        self.handle.accept_threshold(&self.keys, threshold).await?;
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

    /// TODO: bridge into the core `RemoteKeygen` state machine. Out of
    /// scope for this slice — the lobby is up, the keygen isn't.
    pub async fn start_keygen(&self) -> Result<()> {
        Err(anyhow!(
            "remote keygen is not yet implemented — lobby ends here"
        ))
    }
}

// ============================================================================
// FRB subscription wrapper (generics don't cross FFI; concrete wrapper needed)
// ============================================================================

pub struct LobbyStateBroadcastSubscription(pub(crate) BehaviorBroadcastSubscription<FfiLobbyState>);

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
    pub fn start(&self, sink: StreamSink<FfiLobbyState>) -> std::result::Result<(), StartError> {
        self.0._start(sink)
    }

    #[frb(sync)]
    pub fn stop(&self) -> bool {
        self.0._stop()
    }
}
