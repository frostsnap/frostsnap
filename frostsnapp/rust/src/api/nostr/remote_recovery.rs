//! FFI surface for the remote recovery lobby. Thin wrapper around
//! `frostsnap_nostr::recovery::RecoveryLobbyClient` that exposes a
//! Dart-subscribable state stream and async methods for the full
//! recovery-lobby lifecycle: presence → post_share → finish.

use crate::api::broadcast::BehaviorBroadcast;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::Sink;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::{AccessStructureRef, DeviceId, SymmetricKey};
use frostsnap_core::schnorr_fun::frost::ShareImage;
use frostsnap_macros::broadcast_handle;
use frostsnap_nostr::channel_runner::NostrProfile;
use frostsnap_nostr::keygen::DeviceKind;
pub use frostsnap_nostr::recovery::{
    FinishedRecovery, ObservedShare, RecoveryParticipantInfo, RecoveredKey, RecoveryChannelMetadata,
    RecoveryLobbyClient, RecoveryLobbyEvent, RecoveryLobbyHandle, RecoveryLobbyState, SharePost,
};
use frostsnap_core::coordinator::restoration::RecoveringAccessStructure;
use frostsnap_nostr::{Client, EventId, PublicKey};

/// Rust-internal bundle stashed alongside the FRB-safe
/// FinishedRecovery — carries the RecoveringAccessStructure and the
/// leader's channel metadata (key_name, purpose) so
/// `persist_recovered` can call
/// `Coordinator::finalize_remote_recovery` without a second wire
/// round-trip.
#[frb(ignore)]
pub(crate) struct FinishedSlotInner {
    pub finished: FinishedRecovery,
    pub ras: RecoveringAccessStructure,
    pub key_name: String,
    pub purpose: KeyPurpose,
}
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

// ============================================================================
// FRB mirrors
// ============================================================================

// `DeviceKind` is already mirrored via api/nostr/remote_keygen.rs — reuse.

#[frb(mirror(SharePost), non_opaque)]
pub struct _SharePost {
    pub device_id: DeviceId,
    pub device_name: String,
    pub device_kind: DeviceKind,
    pub share_image: ShareImage,
    pub needs_consolidation: bool,
}

#[frb(mirror(ObservedShare), non_opaque)]
pub struct _ObservedShare {
    pub event_id: EventId,
    pub author: PublicKey,
    pub post: SharePost,
}

#[frb(mirror(RecoveryParticipantInfo), non_opaque)]
pub struct _RecoveryParticipantInfo {
    pub pubkey: PublicKey,
    pub joined_at_secs: u64,
    pub profile: Option<NostrProfile>,
    pub posted_shares: Vec<EventId>,
    pub left: bool,
}

#[frb(mirror(RecoveryChannelMetadata), non_opaque)]
pub struct _RecoveryChannelMetadata {
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub threshold_hint: Option<u16>,
}

#[frb(mirror(RecoveredKey), non_opaque)]
pub struct _RecoveredKey {
    pub access_structure_ref: AccessStructureRef,
    pub winning_share_refs: Vec<EventId>,
}

/// `SharedKey` intentionally stays inside Rust — it never crosses
/// the FRB boundary. Dart consumes the finished-recovery outcome as
/// `(access_structure_ref, share_refs)` via this mirror; the
/// `SharedKey` itself lives inside the runtime handle and is
/// consumed by `persist_recovered`.
#[frb(mirror(FinishedRecovery), non_opaque)]
pub struct _FinishedRecovery {
    pub access_structure_ref: AccessStructureRef,
    pub share_refs: Vec<EventId>,
}

#[frb(mirror(RecoveryLobbyState), non_opaque)]
pub struct _RecoveryLobbyState {
    pub metadata: RecoveryChannelMetadata,
    pub participants: HashMap<PublicKey, RecoveryParticipantInfo>,
    pub shares: Vec<ObservedShare>,
    pub current_recovery: Option<RecoveredKey>,
    pub finished: Option<FinishedRecovery>,
    pub cancelled: bool,
}

// ============================================================================
// Sink: RecoveryLobbyEvent → shared bridge state
// ============================================================================

/// Shared state populated by the sink and read by the entry point +
/// the FRB handle. See plan §"Emission gate" — the broadcast can't
/// be seeded until the first `StateChanged` lands (metadata comes
/// from the ChannelCreation event), so it's held in
/// `Arc<Mutex<Option<BehaviorBroadcast<_>>>>` and lazily populated.
#[frb(ignore)]
pub(crate) struct RecoveryBridge {
    pub broadcast: Arc<Mutex<Option<BehaviorBroadcast<RecoveryLobbyState>>>>,
    pub finished_slot:
        Arc<Mutex<Option<FinishedSlotInner>>>,
    pub verification_failed: Arc<Mutex<bool>>,
    pub cancelled_pre_state: Arc<Mutex<bool>>,
    pub ready: Arc<Notify>,
    pub state_changed: Arc<Notify>,
}

impl RecoveryBridge {
    pub fn new() -> Self {
        Self {
            broadcast: Arc::new(Mutex::new(None)),
            finished_slot: Arc::new(Mutex::new(None)),
            verification_failed: Arc::new(Mutex::new(false)),
            cancelled_pre_state: Arc::new(Mutex::new(false)),
            ready: Arc::new(Notify::new()),
            state_changed: Arc::new(Notify::new()),
        }
    }

    pub fn sink(&self) -> RecoveryBridgeSink {
        RecoveryBridgeSink {
            broadcast: self.broadcast.clone(),
            finished_slot: self.finished_slot.clone(),
            verification_failed: self.verification_failed.clone(),
            cancelled_pre_state: self.cancelled_pre_state.clone(),
            ready: self.ready.clone(),
            state_changed: self.state_changed.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RecoveryBridgeSink {
    broadcast: Arc<Mutex<Option<BehaviorBroadcast<RecoveryLobbyState>>>>,
    finished_slot:
        Arc<Mutex<Option<FinishedSlotInner>>>,
    verification_failed: Arc<Mutex<bool>>,
    cancelled_pre_state: Arc<Mutex<bool>>,
    ready: Arc<Notify>,
    state_changed: Arc<Notify>,
}

impl Sink<RecoveryLobbyEvent> for RecoveryBridgeSink {
    fn send(&self, event: RecoveryLobbyEvent) {
        match event {
            RecoveryLobbyEvent::StateChanged(state) => {
                let mut slot = self.broadcast.lock().unwrap();
                match slot.as_ref() {
                    Some(bcast) => {
                        bcast.add(&state);
                    }
                    None => {
                        *slot = Some(BehaviorBroadcast::seeded(state));
                        drop(slot);
                        self.ready.notify_waiters();
                    }
                }
                self.state_changed.notify_waiters();
            }
            RecoveryLobbyEvent::RecoveryAvailable(_) => {
                self.state_changed.notify_waiters();
            }
            RecoveryLobbyEvent::Finished(finished, ras, key_name, purpose) => {
                *self.finished_slot.lock().unwrap() = Some(FinishedSlotInner {
                    finished,
                    ras,
                    key_name,
                    purpose,
                });
                self.state_changed.notify_waiters();
            }
            RecoveryLobbyEvent::FinishVerificationFailed => {
                *self.verification_failed.lock().unwrap() = true;
                self.state_changed.notify_waiters();
            }
            RecoveryLobbyEvent::Cancelled => {
                let bcast_present = self.broadcast.lock().unwrap().is_some();
                if !bcast_present {
                    *self.cancelled_pre_state.lock().unwrap() = true;
                    self.ready.notify_waiters();
                }
                self.state_changed.notify_waiters();
            }
        }
    }
}

// ============================================================================
// RemoteRecoveryLobbyHandle
// ============================================================================

broadcast_handle! { pub struct RecoveryLobbyStateBcast(pub BehaviorBroadcast<RecoveryLobbyState>); }

#[frb(opaque)]
pub struct RemoteRecoveryLobbyHandle {
    inner: RecoveryLobbyHandle,
    invite_link: String,
    state_broadcast: BehaviorBroadcast<RecoveryLobbyState>,
    #[allow(dead_code)]
    client: Client,
    finished_slot:
        Arc<Mutex<Option<FinishedSlotInner>>>,
    verification_failed: Arc<Mutex<bool>>,
    state_changed: Arc<Notify>,
}

impl RemoteRecoveryLobbyHandle {
    #[frb(ignore)]
    pub(crate) fn from_bridge(
        inner: RecoveryLobbyHandle,
        invite_link: String,
        client: Client,
        broadcast: BehaviorBroadcast<RecoveryLobbyState>,
        bridge: &RecoveryBridge,
    ) -> Self {
        Self {
            inner,
            invite_link,
            state_broadcast: broadcast,
            client,
            finished_slot: bridge.finished_slot.clone(),
            verification_failed: bridge.verification_failed.clone(),
            state_changed: bridge.state_changed.clone(),
        }
    }

    #[frb(sync)]
    pub fn invite_link(&self) -> String {
        self.invite_link.clone()
    }

    #[frb(sync)]
    pub fn my_pubkey(&self) -> PublicKey {
        let keys = self.inner.runner_handle().signing_keys();
        keys.public_key().into()
    }

    #[frb(sync)]
    pub fn sub_state(&self) -> RecoveryLobbyStateBcast {
        RecoveryLobbyStateBcast::new(self.state_broadcast.clone())
    }

    pub async fn announce_presence(&self) -> Result<()> {
        let outcome = self.inner.announce_presence().await?;
        require_relay_success(&outcome, "announce_presence")
    }

    pub async fn post_share(&self, post: SharePost) -> Result<EventId> {
        let outcome = self.inner.post_share(post).await?;
        require_relay_success(&outcome, "post_share")?;
        Ok(outcome.inner_event_id)
    }

    pub async fn finish(&self, share_refs: Vec<EventId>) -> Result<()> {
        let outcome = self.inner.finish(share_refs).await?;
        require_relay_success(&outcome, "finish")
    }

    pub async fn leave(&self) -> Result<()> {
        let outcome = self.inner.leave().await?;
        require_relay_success(&outcome, "leave")
    }

    pub async fn cancel(&self) -> Result<()> {
        let outcome = self.inner.cancel_lobby().await?;
        require_relay_success(&outcome, "cancel")
    }

    /// Await `Finished` (or `FinishVerificationFailed` → Err).
    /// Idempotent — resolves immediately if already finished.
    pub async fn await_finished(&self) -> Result<FinishedRecovery> {
        loop {
            let notified = self.state_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if *self.verification_failed.lock().unwrap() {
                return Err(anyhow!("finish verification failed"));
            }
            if let Some(slot) = self.finished_slot.lock().unwrap().as_ref() {
                return Ok(slot.finished.clone());
            }
            notified.await;
        }
    }

    /// Persist the recovered access structure into the coordinator.
    /// Calls `Coordinator::finalize_remote_recovery` with the
    /// finished-slot's stashed `RecoveringAccessStructure`, deriving
    /// `my_local_devices` from the DeviceIds carried by this
    /// participant's own SharePosts (which are guaranteed to be
    /// local — the participant published them).
    pub async fn persist_recovered(
        &self,
        coord: &crate::api::coordinator::Coordinator,
        encryption_key: SymmetricKey,
    ) -> Result<AccessStructureRef> {
        let slot = self
            .finished_slot
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| FinishedSlotInner {
                finished: s.finished.clone(),
                ras: s.ras.clone(),
                key_name: s.key_name.clone(),
                purpose: s.purpose,
            })
            .ok_or_else(|| anyhow!("recovery not finished yet"))?;

        let me: PublicKey = self.inner.runner_handle().signing_keys().public_key().into();
        let my_local_devices = self
            .state_broadcast
            .latest()
            .as_ref()
            .map(|state| frostsnap_nostr::recovery::my_local_devices(state, me))
            .unwrap_or_default();

        let mut rng = rand::thread_rng();
        coord
            .call_finalize_remote_recovery(
                &slot.ras,
                slot.key_name.clone(),
                slot.purpose,
                &my_local_devices,
                encryption_key,
                &mut rng,
            )
            .map_err(|e| anyhow!("finalize_remote_recovery: {e}"))
    }
}

fn require_relay_success(
    outcome: &frostsnap_nostr::channel_runner::SendOutcome,
    label: &str,
) -> Result<()> {
    if outcome.any_relay_success() {
        Ok(())
    } else {
        Err(anyhow!(
            "{label}: no relay accepted publish: {:?}",
            outcome.relay_failed
        ))
    }
}

// ============================================================================
// Entry-point support: build the bridge + handle
// ============================================================================

#[frb(ignore)]
pub(crate) struct RecoveryLobbyBridge {
    pub bridge: RecoveryBridge,
    pub sink: RecoveryBridgeSink,
}

impl RecoveryLobbyBridge {
    pub fn new() -> Self {
        let bridge = RecoveryBridge::new();
        let sink = bridge.sink();
        Self { bridge, sink }
    }
}
