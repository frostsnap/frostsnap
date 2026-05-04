//! FFI surface for the remote (org) keygen lobby. Thin wrapper around
//! `frostsnap_nostr::keygen::LobbyClient` that exposes a Dart-subscribable
//! state stream and async methods for the full lobby lifecycle:
//! presence → register (mark ready) → start keygen. Threshold no longer
//! has its own negotiation round-trip: it rides inline on `StartKeygen`
//! and joiners signal acceptance implicitly by broadcasting their first
//! DKG round-1 output on the resulting subchannel.

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
    ResolvedKeygen, SelectedCoordinator,
};
use frostsnap_nostr::{EventId, Keys, PublicKey};
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
}

impl From<CoreParticipantStatus> for FfiParticipantStatus {
    fn from(s: CoreParticipantStatus) -> Self {
        match s {
            CoreParticipantStatus::Joining => Self::Joining,
            CoreParticipantStatus::Ready => Self::Ready,
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
    /// `None` until the participant publishes `Register`. Dart uses
    /// this as the handle it passes back through `start_keygen` — the
    /// host picks a subset of participants and their register ids land
    /// as e-tags on the StartKeygen event.
    pub register_event_id: Option<EventId>,
}

/// Surfaced to the joiner once `StartKeygen` arrives and its invite
/// has been decrypted — i.e. this participant is in the host's
/// selected set. Dart reads this to render a host-proposes-N-of-M
/// accept/decline modal. `None` on parties not in the selected set.
///
/// `acked.length == participants.length` is the canonical "all
/// participants have acked" check — Dart computes that as a getter
/// rather than carrying a redundant bool that could drift out of sync
/// with the two lists.
#[derive(Clone, Debug)]
pub struct FfiPendingKeygen {
    pub threshold: u16,
    pub initiator: PublicKey,
    /// Participants in the order given by the StartKeygen event's
    /// e-tags (same ordering the DKG will use).
    pub participants: Vec<FfiLobbyParticipant>,
    /// E-tag target for `ack_keygen` and (post-StartKeygen) `leave`.
    /// Comes from `ResolvedKeygen.keygen_event_id`.
    pub start_keygen_event_id: EventId,
    /// Pubkeys that have published `AckKeygen` (or are the initiator,
    /// who is implicitly acked by publishing `StartKeygen`). Subset of
    /// `participants` — non-selected acks are filtered out at the
    /// receiver, and `LobbyState.acked` is only populated for selected
    /// pubkeys.
    pub acked: Vec<PublicKey>,
}

#[derive(Clone, Debug)]
pub struct FfiLobbyState {
    pub initiator: Option<PublicKey>,
    pub key_name: Option<String>,
    pub purpose: Option<KeyPurpose>,
    pub participants: Vec<FfiLobbyParticipant>,
    /// True when every participant has published `Register`.
    pub all_ready: bool,
    /// `Some` on this party's side once the host has published
    /// `StartKeygen` and this party is in the selected set.
    pub pending_keygen: Option<FfiPendingKeygen>,
    /// `true` once the initiator has published `CancelLobby`. Latched —
    /// remains true for the life of the handle.
    pub cancelled: bool,
}

impl FfiLobbyState {
    fn from_lobby(state: &LobbyState) -> Self {
        let participants: Vec<FfiLobbyParticipant> = state
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
                register_event_id: p.register_event_id,
            })
            .collect();

        let pending_keygen = state.keygen.as_ref().map(|resolved| {
            FfiPendingKeygen::from_resolved(resolved, &participants, state.initiator, &state.acked)
        });

        Self {
            initiator: state.initiator,
            key_name: state.key_name.clone(),
            purpose: state.purpose,
            participants,
            all_ready: state.all_ready(),
            pending_keygen,
            cancelled: false,
        }
    }

    fn empty() -> Self {
        Self {
            initiator: None,
            key_name: None,
            purpose: None,
            participants: vec![],
            all_ready: false,
            pending_keygen: None,
            cancelled: false,
        }
    }
}

impl FfiPendingKeygen {
    /// True if the given pubkey is in the selected participant set —
    /// i.e. they've been invited to this keygen. Dart calls this with
    /// the local pubkey to render the right UI when `pending_keygen`
    /// is `Some`: included → accept screen; not included → "round
    /// started without you" banner on the lobby.
    #[frb(sync)]
    pub fn includes(&self, pubkey: &PublicKey) -> bool {
        self.participants.iter().any(|p| p.pubkey == *pubkey)
    }

    fn from_resolved(
        resolved: &ResolvedKeygen,
        all_participants: &[FfiLobbyParticipant],
        initiator: Option<PublicKey>,
        acked_set: &std::collections::BTreeSet<PublicKey>,
    ) -> Self {
        // `lobby.process_register` is now invoked for every e-tagged
        // Register event during StartKeygen processing, so every
        // selected pubkey is guaranteed to be in `all_participants`.
        // If the lookup fails here, something has gone wrong with
        // state-keeping rather than just timing — surface it loudly
        // instead of silently dropping the row.
        let ordered: Vec<FfiLobbyParticipant> = resolved
            .participants
            .iter()
            .map(|(pk, _)| {
                all_participants
                    .iter()
                    .find(|p| p.pubkey == *pk)
                    .cloned()
                    .unwrap_or_else(|| {
                        panic!(
                            "selected participant {pk} missing from lobby state — \
                             process_register must run for every e-tagged Register before \
                             FfiPendingKeygen is built",
                        )
                    })
            })
            .collect();
        let acked: Vec<PublicKey> = resolved
            .participants
            .iter()
            .map(|(pk, _)| *pk)
            .filter(|pk| acked_set.contains(pk))
            .collect();
        Self {
            threshold: resolved.threshold,
            // Invariant: by the time `lobby.keygen` is `Some`, the
            // StartKeygen arm has run and `lobby.initiator` was set
            // by `CreationEventReceived` strictly earlier (we reject
            // StartKeygen from a non-initiator).
            initiator: initiator
                .expect("lobby.initiator must be set whenever lobby.keygen is set"),
            participants: ordered,
            start_keygen_event_id: resolved.keygen_event_id,
            acked,
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
            LobbyEvent::AllAcked => {
                // The latest `LobbyChanged` already carried
                // `pending_keygen.all_acked = true`; consumers read it
                // off the snapshot. Wiring the actual DKG-start pivot
                // is the follow-up slice.
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
/// for presence → mark ready → start keygen.
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
    ///
    /// This is the one lobby action that hard-requires a successful
    /// relay publish: a Register with no peers would leave the local
    /// UI showing Ready while others still see Joining. Returns after
    /// (a) ≥1 relay has OK'd the event and (b) the lobby state has
    /// locally reflected the change (Sink fired).
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
    /// it's surfaced via `FfiPendingKeygen.start_keygen_event_id`.
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
        selected: Vec<FfiSelectedParticipant>,
    ) -> Result<()> {
        let coordinators: Vec<SelectedCoordinator> = selected
            .into_iter()
            .map(|s| SelectedCoordinator {
                register_event_id: s.register_event_id,
                pubkey: s.pubkey,
            })
            .collect();
        let outcome = self
            .handle
            .start_keygen(&self.keys, &coordinators, threshold)
            .await?;
        if !outcome.any_relay_success() {
            return Err(anyhow!(
                "no relay accepted StartKeygen: {:?}",
                outcome.relay_failed
            ));
        }
        Ok(())
    }
}

/// One (pubkey, register_event_id) pair per selected participant.
/// Dart snapshots its own `FfiLobbyState.participants` to build this
/// list (filter to Ready, and in the future honour an exclusion UI).
#[frb(non_opaque)]
#[derive(Clone, Debug)]
pub struct FfiSelectedParticipant {
    pub pubkey: PublicKey,
    pub register_event_id: EventId,
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
