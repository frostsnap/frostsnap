use crate::channel::{ChannelKeys, ChannelSecret};
use crate::channel_runner::{
    decode_bincode, ChannelInfraEvent, ChannelMessageDraft, ChannelRunner, ChannelRunnerEvent,
    ChannelRunnerHandle, GroupMember, SendOutcome, BINCODE_CONFIG,
};
use crate::keygen::DeviceKind;
use crate::signing::ChannelParticipant;
use crate::{EventId, PublicKey};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use frostsnap_coordinator::Sink;
use frostsnap_core::{
    coordinator::restoration::{RecoverShare, RecoveringAccessStructure},
    device::KeyPurpose,
    message::HeldShare2,
    schnorr_fun::frost::{Fingerprint, ShareImage},
    AccessStructureRef, DeviceId,
};
use nostr_sdk::{Client, Event, EventBuilder, Kind};
use std::collections::HashMap;

/// NIP-based custom kind for recovery-lobby wire messages.
/// (Keygen uses 9002; recovery follows in the same range.)
pub const KIND_FROSTSNAP_RECOVERY_LOBBY: Kind = Kind::Custom(9003);

// =============================================================================
// Wire types
// =============================================================================

/// Payloads carried on the recovery-lobby channel.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub enum RecoveryLobbyMessage {
    /// Explicit "I am here" — parallels keygen's `Presence`.
    Presence,
    /// Contribute a share to the pool.
    Share(SharePost),
    /// Leader-only. Locks in the winning share subset. `share_refs`
    /// point at the `EventId`s of prior `Share` events.
    Finish { share_refs: Vec<EventId> },
    /// Idempotent departure — parallels keygen's `Leave`.
    Leave,
    /// Leader-only. Aborts the lobby.
    CancelLobby,
}

/// Flat wire payload for one contributed share. Deliberately NOT the
/// `frostsnap_core::coordinator::restoration::RecoverShare` type —
/// keeps the wire schema stable while core evolves the internal
/// representation. Same trick keygen uses with `DeviceRegistration`.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SharePost {
    pub device_id: DeviceId,
    pub device_name: String,
    pub device_kind: DeviceKind,
    pub share_image: ShareImage,
    pub needs_consolidation: bool,
}

impl SharePost {
    /// Lift the wire payload into a `RecoverShare` for `fuzzy_recovery`.
    /// The `HeldShare2::{access_structure_ref, threshold, key_name,
    /// purpose}` fields are set to `None` — the wire schema doesn't
    /// carry them, and we don't want the transport to be a path for
    /// forged "trusted" metadata.
    pub fn to_recover_share(&self) -> RecoverShare {
        RecoverShare {
            held_by: self.device_id,
            held_share: HeldShare2 {
                access_structure_ref: None,
                share_image: self.share_image,
                threshold: None,
                key_name: None,
                purpose: None,
                needs_consolidation: self.needs_consolidation,
            },
        }
    }
}

/// Leader-authored, bincode-encoded and base64-wrapped into the
/// NIP-28 `ChannelCreation` event's `content` field. Same pattern as
/// keygen's `LobbyChannelMetadata`. Immutable for the life of the
/// channel.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct RecoveryChannelMetadata {
    pub key_name: String,
    pub purpose: KeyPurpose,
    pub threshold_hint: Option<u16>,
}

impl RecoveryChannelMetadata {
    pub fn encode_content(&self) -> Result<String> {
        let bytes = bincode::encode_to_vec(self, BINCODE_CONFIG)?;
        Ok(BASE64.encode(bytes))
    }

    pub fn decode_content(content: &str) -> Result<Self> {
        let bytes = BASE64.decode(content)?;
        let (val, _) = bincode::decode_from_slice(&bytes, BINCODE_CONFIG)?;
        Ok(val)
    }
}

// =============================================================================
// Fold state
// =============================================================================

/// One `SharePost` as observed in the fold — payload plus its
/// nostr envelope for downstream references.
#[derive(Clone, Debug)]
pub struct ObservedShare {
    pub event_id: EventId,
    pub author: PublicKey,
    pub post: SharePost,
}

#[derive(Clone, Debug)]
pub struct RecoveryParticipantInfo {
    pub pubkey: PublicKey,
    pub joined_at_secs: u64,
    pub posted_shares: Vec<EventId>,
    pub left: bool,
}

#[derive(Clone, Debug)]
pub struct RecoveryLobbyState {
    pub metadata: RecoveryChannelMetadata,
    /// The channel creator (authorized to publish Finish/CancelLobby).
    /// Mirrors keygen's `LobbyState.initiator` — UIs badge this row.
    pub leader: PublicKey,
    pub participants: HashMap<PublicKey, RecoveryParticipantInfo>,
    pub shares: Vec<ObservedShare>,
    pub current_recovery: Option<RecoveredKey>,
    pub finished: Option<FinishedRecovery>,
    pub cancelled: bool,
}

/// The consumer-facing snapshot: the lobby's app fold plus the
/// channel's member block (runner-owned; see
/// [`ChannelInfraEvent::Members`]). One stream, one payload — names
/// and avatars come from `members`, protocol facts from `state`.
/// The wrapper stashes the latest member block unconditionally (no
/// metadata gate on that slot), so profile events arriving before
/// the creation event ride out with the first snapshot instead of
/// being dropped.
impl RecoveryLobbyState {
    /// The recovered wallet's pubkey -> share-index assignment,
    /// derived from the leader-verified winning subset only (the
    /// assignment must describe the recovered access structure, not
    /// every posted share). Feeds the coordination channel's
    /// creation metadata when recovery has to (re)create it.
    pub fn channel_participants(&self) -> Vec<ChannelParticipant> {
        let Some(finished) = &self.finished else {
            return Vec::new();
        };
        let mut by_author: std::collections::BTreeMap<PublicKey, std::collections::BTreeSet<u32>> =
            Default::default();
        for share_ref in &finished.share_refs {
            if let Some(obs) = self.shares.iter().find(|o| o.event_id == *share_ref) {
                let index =
                    u32::try_from(obs.post.share_image.index).expect("share index fits u32");
                by_author.entry(obs.author).or_default().insert(index);
            }
        }
        by_author
            .into_iter()
            .map(|(pubkey, indices)| ChannelParticipant {
                pubkey,
                share_indices: indices.into_iter().collect(),
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct RecoveryLobbySnapshot {
    pub state: RecoveryLobbyState,
    pub members: Vec<GroupMember>,
}

/// Fuzzy-recovery result — the current "if everyone posted their
/// share, this is what we'd get" snapshot. Leader UIs use this to
/// know when it's safe to publish `Finish`.
#[derive(Clone, Debug)]
pub struct RecoveredKey {
    pub access_structure_ref: AccessStructureRef,
    pub winning_share_refs: Vec<EventId>,
}

/// Latched post-`Finish` recovery outcome, as seen by consumers.
/// The `SharedKey` is NOT part of this — it never crosses the FRB
/// boundary, and lives beside this value in the runtime handle's
/// `finished_slot`, consumed by `persist_recovered`.
#[derive(Clone, Debug)]
pub struct FinishedRecovery {
    pub access_structure_ref: AccessStructureRef,
    pub share_refs: Vec<EventId>,
}

// =============================================================================
// Sink event stream
// =============================================================================

#[derive(Clone, Debug)]
pub enum RecoveryLobbyEvent {
    StateChanged(RecoveryLobbySnapshot),
    RecoveryAvailable(RecoveredKey),
    /// The `RecoveringAccessStructure` + metadata ride here so the
    /// FRB layer can stash them in its finished_slot alongside the
    /// FRB-safe `FinishedRecovery`. They never cross the FRB boundary.
    Finished(
        FinishedRecovery,
        RecoveringAccessStructure,
        String,
        KeyPurpose,
    ),
    FinishVerificationFailed,
    Cancelled,
}

// =============================================================================
// Client
// =============================================================================

pub struct RecoveryLobbyClient {
    channel_keys: ChannelKeys,
    channel_secret: ChannelSecret,
    metadata: Option<RecoveryChannelMetadata>,
    /// FROST fingerprint used to verify reconstructed shared keys.
    /// Production callers stick with the default; tests use a
    /// cheaper fingerprint to avoid grinding cost.
    fingerprint: Fingerprint,
}

impl RecoveryLobbyClient {
    pub fn new(channel_secret: ChannelSecret) -> Self {
        let channel_keys = ChannelKeys::from_channel_secret(&channel_secret);
        Self {
            channel_keys,
            channel_secret,
            metadata: None,
            fingerprint: Fingerprint::default(),
        }
    }

    /// Stash the metadata for `build_creation_event`. Leader-only.
    pub fn with_metadata(mut self, meta: RecoveryChannelMetadata) -> Self {
        self.metadata = Some(meta);
        self
    }

    /// Override the fuzzy-recovery fingerprint. Tests pass a cheap
    /// fingerprint (e.g. `TEST_FINGERPRINT`); production sticks with
    /// `Fingerprint::default()`.
    pub fn with_fingerprint(mut self, fingerprint: Fingerprint) -> Self {
        self.fingerprint = fingerprint;
        self
    }

    /// `frostsnap://recovery/<hex>` invite link.
    pub fn invite_link(&self) -> String {
        self.channel_secret.recovery_invite_link()
    }

    /// Build the NIP-28 `ChannelCreation` event carrying the stashed
    /// `RecoveryChannelMetadata`. Leader-only; requires `with_metadata`.
    pub async fn build_creation_event(&self, identity: &crate::NostrIdentity) -> Result<Event> {
        let metadata = self
            .metadata
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("build_creation_event requires with_metadata first"))?;
        let keys = identity.keys()?;
        let content = metadata.encode_content()?;
        let inner_event = EventBuilder::new(Kind::ChannelCreation, content)
            .build(keys.public_key())
            .sign(&keys)
            .await?;
        Ok(inner_event)
    }

    /// Spawn the per-lobby runner. Forwards `identity` to
    /// `ChannelRunner::with_identity(identity)?` — that call
    /// captures the signing Keys AND the in-channel profile publish
    /// policy. `init_event` is the leader's `ChannelCreation`; joiners
    /// pass `None`.
    pub async fn run(
        self,
        client: Client,
        identity: crate::NostrIdentity,
        init_event: Option<Event>,
        sink: impl Sink<RecoveryLobbyEvent> + Clone + Sync,
    ) -> Result<RecoveryLobbyHandle> {
        let _is_leader = init_event.is_some();
        let mut runner = ChannelRunner::new(self.channel_keys.clone())
            .with_message_expiration(super::RECOVERY_MESSAGE_TTL)
            .with_identity(identity)?;
        if let Some(init) = init_event {
            runner = runner.with_init_event(init);
        }
        let (runner_handle, mut events) = runner.run(client.clone()).await?;

        // Share only the folded state with the wrapper task — NOT a full
        // `ChannelRunnerHandle` clone. Keeping shutdown_tx alive here
        // would defeat the drop-of-last-handle-clone teardown chain.
        let state_for_task = runner_handle.state_arc();
        let channel_keys = self.channel_keys.clone();
        let fingerprint = self.fingerprint;
        tokio::spawn(async move {
            // Wrapper state — see plan §"Emission gate". Everything
            // held here is pre-gate scratch: we can't build a
            // `RecoveryLobbyState` before `RecoveryChannelMetadata`
            // arrives, so we buffer wire events until it does.
            let mut pre_gate_buffer: Vec<Event> = Vec::new();
            // Latest member block from the runner (block pattern).
            // Stashed unconditionally — NOT behind the metadata gate —
            // so early profile events ride out with the first
            // snapshot instead of being dropped.
            let mut members: Vec<GroupMember> = Vec::new();
            let mut fold: Option<RecoveryLobbyState> = None;

            while let Some(event) = events.recv().await {
                match event {
                    ChannelRunnerEvent::CreationEventReceived => {
                        // Metadata decoded here → fold created →
                        // buffered events replayed → first StateChanged.
                        let creation = state_for_task.lock().unwrap().creation_event.clone();
                        let Some(creation) = creation else { continue };
                        if fold.is_some() {
                            // Re-emit shouldn't happen (creation is
                            // once-latched inside the runner state);
                            // if it does, be idempotent.
                            continue;
                        }
                        match RecoveryChannelMetadata::decode_content(&creation.content) {
                            Ok(metadata) => {
                                let leader: PublicKey = creation.pubkey.into();
                                let mut state = RecoveryLobbyState {
                                    metadata,
                                    leader,
                                    participants: HashMap::new(),
                                    shares: Vec::new(),
                                    current_recovery: None,
                                    finished: None,
                                    cancelled: false,
                                };
                                // Leader is a participant from t=0.
                                upsert_participant(
                                    &mut state,
                                    leader,
                                    creation.created_at.as_u64(),
                                );

                                // Drain buffered events into the fold.
                                for ev in pre_gate_buffer.drain(..) {
                                    process_event(
                                        &ev,
                                        &mut state,
                                        &members,
                                        &state_for_task,
                                        &sink,
                                        fingerprint,
                                    );
                                }
                                sink.send(snapshot(&state, &members));
                                fold = Some(state);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    event_id = %creation.id,
                                    error = %e,
                                    "malformed RecoveryChannelMetadata; emitting Cancelled",
                                );
                                sink.send(RecoveryLobbyEvent::Cancelled);
                                return;
                            }
                        }
                    }
                    ChannelRunnerEvent::AppEvent { inner_event, ack } => {
                        if inner_event.kind == KIND_FROSTSNAP_RECOVERY_LOBBY {
                            match &mut fold {
                                Some(state) => {
                                    process_event(
                                        &inner_event,
                                        state,
                                        &members,
                                        &state_for_task,
                                        &sink,
                                        fingerprint,
                                    );
                                }
                                None => {
                                    pre_gate_buffer.push(inner_event);
                                }
                            }
                        }
                        // Signal the dispatch ack AFTER processing so a
                        // local `dispatch` caller only resolves once
                        // the sink has fired for it.
                        if let Some(ack) = ack {
                            let _ = ack.send(());
                        }
                    }
                    ChannelRunnerEvent::Channel(ChannelInfraEvent::Members {
                        members: new_members,
                        ..
                    }) => {
                        members = new_members;
                        if let Some(state) = &fold {
                            sink.send(snapshot(state, &members));
                        }
                    }
                    ChannelRunnerEvent::ChatMessage { .. } => {}
                }
            }
            // silences unused; channel_keys was kept in case future
            // fetch-inner-event lookups land here (mirrors keygen).
            let _ = &channel_keys;
        });

        Ok(RecoveryLobbyHandle { runner_handle })
    }
}

// =============================================================================
// Handle
// =============================================================================

#[derive(Clone)]
pub struct RecoveryLobbyHandle {
    runner_handle: ChannelRunnerHandle,
}

impl RecoveryLobbyHandle {
    pub fn runner_handle(&self) -> &ChannelRunnerHandle {
        &self.runner_handle
    }

    pub async fn announce_presence(&self) -> Result<SendOutcome> {
        self.send(&RecoveryLobbyMessage::Presence, &[]).await
    }

    pub async fn post_share(&self, post: SharePost) -> Result<SendOutcome> {
        self.send(&RecoveryLobbyMessage::Share(post), &[]).await
    }

    /// Leader-only. Non-leader publishes are dropped receiver-side.
    pub async fn finish(&self, share_refs: Vec<EventId>) -> Result<SendOutcome> {
        self.send(&RecoveryLobbyMessage::Finish { share_refs }, &[])
            .await
    }

    pub async fn leave(&self) -> Result<SendOutcome> {
        self.send(&RecoveryLobbyMessage::Leave, &[]).await
    }

    /// Leader-only. Non-leader publishes are dropped receiver-side.
    pub async fn cancel_lobby(&self) -> Result<SendOutcome> {
        self.send(&RecoveryLobbyMessage::CancelLobby, &[]).await
    }

    async fn send(&self, msg: &RecoveryLobbyMessage, e_tags: &[EventId]) -> Result<SendOutcome> {
        let draft = ChannelMessageDraft::app(KIND_FROSTSNAP_RECOVERY_LOBBY, msg, e_tags.to_vec())?;
        self.runner_handle.dispatch_signed(draft).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn pk(byte: u8) -> PublicKey {
        PublicKey([byte; 32])
    }
    fn eid(byte: u8) -> EventId {
        EventId([byte; 32])
    }
    fn did(byte: u8) -> DeviceId {
        DeviceId([byte; 33])
    }

    fn base_state() -> RecoveryLobbyState {
        RecoveryLobbyState {
            metadata: RecoveryChannelMetadata {
                key_name: "t".into(),
                purpose: KeyPurpose::Test,
                threshold_hint: Some(2),
            },
            leader: pk(0),
            participants: HashMap::new(),
            shares: Vec::new(),
            current_recovery: None,
            finished: None,
            cancelled: false,
        }
    }

    fn add_participant(state: &mut RecoveryLobbyState, who: PublicKey, posted: Vec<EventId>) {
        state.participants.insert(
            who,
            RecoveryParticipantInfo {
                pubkey: who,
                joined_at_secs: 0,
                posted_shares: posted,
                left: false,
            },
        );
    }

    fn dummy_share_image(idx: u16) -> frostsnap_core::schnorr_fun::frost::ShareImage {
        use core::num::NonZeroU32;
        use frostsnap_core::schnorr_fun::fun::{marker::Zero, Point, G};
        frostsnap_core::schnorr_fun::frost::ShareImage {
            index: frostsnap_core::schnorr_fun::frost::ShareIndex::from(
                NonZeroU32::new(u32::from(idx) + 1).unwrap(),
            ),
            image: Point::<_, _, Zero>::zero(),
        }
    }

    fn push_share(
        state: &mut RecoveryLobbyState,
        event_id: EventId,
        author: PublicKey,
        device_id: DeviceId,
    ) {
        state.shares.push(ObservedShare {
            event_id,
            author,
            post: SharePost {
                device_id,
                device_name: format!("d-{:?}", device_id.0[0]),
                device_kind: DeviceKind::Frostsnap,
                share_image: dummy_share_image(u16::from(device_id.0[0])),
                needs_consolidation: true,
            },
        });
    }

    #[test]
    fn my_local_devices_empty_when_participant_absent() {
        let state = base_state();
        assert!(my_local_devices(&state, pk(1)).is_empty());
    }

    #[test]
    fn my_local_devices_empty_when_no_shares_posted() {
        let mut state = base_state();
        add_participant(&mut state, pk(1), vec![]);
        assert!(my_local_devices(&state, pk(1)).is_empty());
    }

    #[test]
    fn my_local_devices_returns_devices_of_own_posted_shares() {
        let mut state = base_state();
        let me = pk(1);
        let peer = pk(2);
        add_participant(&mut state, me, vec![eid(0xA1), eid(0xA2)]);
        add_participant(&mut state, peer, vec![eid(0xB1)]);
        push_share(&mut state, eid(0xA1), me, did(0x11));
        push_share(&mut state, eid(0xA2), me, did(0x22));
        push_share(&mut state, eid(0xB1), peer, did(0x33));

        let expected: BTreeSet<_> = [did(0x11), did(0x22)].into_iter().collect();
        assert_eq!(my_local_devices(&state, me), expected);
        // Peer sees a different set.
        assert_eq!(
            my_local_devices(&state, peer),
            [did(0x33)].into_iter().collect(),
        );
    }

    /// The channel-creation assignment comes from WINNING shares
    /// only, grouped by author — a posted-but-losing share must not
    /// appear in the recovered wallet's metadata.
    #[test]
    fn channel_participants_groups_winning_shares_by_author() {
        let mut state = base_state();
        let alice = pk(1);
        let bob = pk(2);
        add_participant(&mut state, alice, vec![eid(0xA1), eid(0xA2)]);
        add_participant(&mut state, bob, vec![eid(0xB1), eid(0xB2)]);
        push_share(&mut state, eid(0xA1), alice, did(1));
        push_share(&mut state, eid(0xA2), alice, did(2));
        push_share(&mut state, eid(0xB1), bob, did(3));
        // Bob also posted a share that did NOT make the winning subset.
        push_share(&mut state, eid(0xB2), bob, did(5));

        // No finished recovery yet -> no assignment.
        assert!(state.channel_participants().is_empty());

        state.finished = Some(FinishedRecovery {
            access_structure_ref: AccessStructureRef {
                key_id: frostsnap_core::KeyId([0; 32]),
                access_structure_id: frostsnap_core::AccessStructureId([0; 32]),
            },
            share_refs: vec![eid(0xA1), eid(0xA2), eid(0xB1)],
        });

        let participants = state.channel_participants();
        // dummy_share_image(i) has index i + 1.
        assert_eq!(
            participants
                .iter()
                .map(|p| (p.pubkey, p.share_indices.clone()))
                .collect::<Vec<_>>(),
            vec![(alice, vec![2, 3]), (bob, vec![4])],
        );
    }

    /// Regression: "I don't know the threshold" must reach
    /// `find_valid_subset` as `None` (inference mode), not `Some(0)`
    /// (a pinned threshold of zero — no zero-share subset ever
    /// reconstructs, so recovery could never complete).
    #[test]
    fn recompute_finds_recovery_without_threshold_hint() {
        use core::num::NonZeroU32;
        use frostsnap_core::schnorr_fun::frost::ShareIndex;
        use frostsnap_core::schnorr_fun::fun::{g, marker::*, s, Scalar, G};

        // Real share images on f(x) = 5 + 3x (a threshold-2 poly);
        // a zero-bit fingerprint accepts the first reconstruction.
        let poly_share_image = |idx: u32| {
            let x = Scalar::<Public, Zero>::from(idx + 1);
            let a0 = Scalar::<Public, Zero>::from(5_u32);
            let a1 = Scalar::<Public, Zero>::from(3_u32);
            let v = s!(a0 + a1 * x);
            ShareImage {
                index: ShareIndex::from(NonZeroU32::new(idx + 1).unwrap()),
                image: g!(v * G).normalize(),
            }
        };
        let zero_fp = Fingerprint {
            bits_per_coeff: 0,
            max_bits_total: 0,
            tag: "test",
        };

        let mut state = base_state();
        state.metadata.threshold_hint = None;
        for i in 0..3_u32 {
            let author = pk(i as u8 + 1);
            let event_id = eid(0xA0 + i as u8);
            add_participant(&mut state, author, vec![event_id]);
            state.shares.push(ObservedShare {
                event_id,
                author,
                post: SharePost {
                    device_id: did(i as u8 + 1),
                    device_name: format!("d-{i}"),
                    device_kind: DeviceKind::Frostsnap,
                    share_image: poly_share_image(i),
                    needs_consolidation: true,
                },
            });
        }

        recompute_current_recovery(&mut state, zero_fp);

        let recovered = state
            .current_recovery
            .as_ref()
            .expect("fuzzy recovery must infer the threshold when the hint is None");
        // All three shares lie on the recovered polynomial.
        let winning: BTreeSet<_> = recovered.winning_share_refs.iter().copied().collect();
        let expected: BTreeSet<_> = [eid(0xA0), eid(0xA1), eid(0xA2)].into_iter().collect();
        assert_eq!(winning, expected);
    }

    /// Ghost posted_shares (event_id present on the participant but
    /// no matching ObservedShare) get silently dropped. This is
    /// defensive against fold/state drift; must not panic.
    #[test]
    fn my_local_devices_drops_ghost_posted_share_refs() {
        let mut state = base_state();
        let me = pk(1);
        // Participant claims to have posted eid(0xA1), but no
        // matching ObservedShare in `shares`.
        add_participant(&mut state, me, vec![eid(0xA1)]);
        assert!(my_local_devices(&state, me).is_empty());
    }
}

// =============================================================================
// Local-device derivation
// =============================================================================

/// Compute the set of local `DeviceId`s for a participant, by
/// walking the fold's `shares` for entries the participant
/// authored. A device is "mine" iff I posted a `SharePost` bearing
/// its DeviceId — the wire-level participants + shares view is the
/// single source of truth for what's local at persistence time.
///
/// Extracted so tests can exercise the derivation directly without
/// standing up the full FRB handle harness. Consumed by the FRB
/// wrapper's `persist_recovered`.
pub fn my_local_devices(
    state: &RecoveryLobbyState,
    me: PublicKey,
) -> std::collections::BTreeSet<DeviceId> {
    let Some(info) = state.participants.get(&me) else {
        return Default::default();
    };
    info.posted_shares
        .iter()
        .filter_map(|id| {
            state
                .shares
                .iter()
                .find(|obs| obs.event_id == *id)
                .map(|obs| obs.post.device_id)
        })
        .collect()
}

// =============================================================================
// Fold helpers
// =============================================================================

fn upsert_participant(state: &mut RecoveryLobbyState, pubkey: PublicKey, joined_at_secs: u64) {
    state
        .participants
        .entry(pubkey)
        .or_insert(RecoveryParticipantInfo {
            pubkey,
            joined_at_secs,
            posted_shares: Vec::new(),
            left: false,
        });
}

fn snapshot(state: &RecoveryLobbyState, members: &[GroupMember]) -> RecoveryLobbyEvent {
    RecoveryLobbyEvent::StateChanged(RecoveryLobbySnapshot {
        state: state.clone(),
        members: members.to_vec(),
    })
}

fn process_event(
    inner_event: &Event,
    state: &mut RecoveryLobbyState,
    members: &[GroupMember],
    state_arc: &std::sync::Arc<std::sync::Mutex<crate::channel_runner::ChannelState>>,
    sink: &impl Sink<RecoveryLobbyEvent>,
    fingerprint: Fingerprint,
) {
    let event_id: EventId = inner_event.id.into();
    let author: PublicKey = inner_event.pubkey.into();

    let msg: RecoveryLobbyMessage = match decode_bincode(inner_event) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(%event_id, error = %e, "failed to decode recovery lobby message");
            return;
        }
    };

    // Leader identity comes from the runner's ChannelState — plan
    // invariant: don't duplicate it into our fold.
    let leader: Option<PublicKey> = state_arc
        .lock()
        .unwrap()
        .creation_event
        .as_ref()
        .map(|e| PublicKey::from(e.pubkey));

    match msg {
        RecoveryLobbyMessage::Presence => {
            upsert_participant(state, author, inner_event.created_at.as_u64());
            sink.send(snapshot(state, members));
        }
        RecoveryLobbyMessage::Share(post) => {
            upsert_participant(state, author, inner_event.created_at.as_u64());
            let observed = ObservedShare {
                event_id,
                author,
                post,
            };
            state.shares.push(observed);
            if let Some(entry) = state.participants.get_mut(&author) {
                entry.posted_shares.push(event_id);
            }
            recompute_current_recovery(state, fingerprint);
            sink.send(snapshot(state, members));
            if let Some(recovered) = &state.current_recovery {
                sink.send(RecoveryLobbyEvent::RecoveryAvailable(recovered.clone()));
            }
        }
        RecoveryLobbyMessage::Finish { share_refs } => {
            let Some(leader) = leader else {
                tracing::warn!(%event_id, "Finish before ChannelCreation landed; ignoring");
                return;
            };
            if author != leader {
                tracing::warn!(%event_id, %author, "Finish from non-leader; ignoring");
                return;
            }
            if state.finished.is_some() {
                return;
            }
            let mut selected = Vec::with_capacity(share_refs.len());
            for ref_id in &share_refs {
                let Some(observed) = state.shares.iter().find(|o| o.event_id == *ref_id) else {
                    tracing::warn!(%event_id, %ref_id, "Finish references unknown share");
                    sink.send(RecoveryLobbyEvent::FinishVerificationFailed);
                    return;
                };
                selected.push(observed.post.to_recover_share());
            }
            let ras = RecoveringAccessStructure::new(
                &selected,
                state.metadata.threshold_hint,
                fingerprint,
            );
            match ras.shared_key.clone() {
                Some(shared_key) => {
                    let asr = AccessStructureRef::from_root_shared_key(&shared_key);
                    let finished = FinishedRecovery {
                        access_structure_ref: asr,
                        share_refs: share_refs.clone(),
                    };
                    let key_name = state.metadata.key_name.clone();
                    let purpose = state.metadata.purpose;
                    state.finished = Some(finished.clone());
                    sink.send(snapshot(state, members));
                    sink.send(RecoveryLobbyEvent::Finished(
                        finished, ras, key_name, purpose,
                    ));
                }
                None => {
                    sink.send(RecoveryLobbyEvent::FinishVerificationFailed);
                }
            }
        }
        RecoveryLobbyMessage::Leave => {
            if let Some(entry) = state.participants.get_mut(&author) {
                entry.left = true;
            }
            sink.send(snapshot(state, members));
        }
        RecoveryLobbyMessage::CancelLobby => {
            let Some(leader) = leader else {
                tracing::warn!(%event_id, "CancelLobby before ChannelCreation; ignoring");
                return;
            };
            if author != leader {
                tracing::warn!(%event_id, %author, "CancelLobby from non-leader; ignoring");
                return;
            }
            if state.cancelled {
                return;
            }
            state.cancelled = true;
            sink.send(snapshot(state, members));
            sink.send(RecoveryLobbyEvent::Cancelled);
        }
    }
}

/// Recompute the fuzzy-recovery snapshot over the full bundle of
/// observed shares. Called after every `Share` fold.
fn recompute_current_recovery(state: &mut RecoveryLobbyState, fingerprint: Fingerprint) {
    let shares: Vec<RecoverShare> = state
        .shares
        .iter()
        .map(|obs| obs.post.to_recover_share())
        .collect();
    let ras = RecoveringAccessStructure::new(&shares, state.metadata.threshold_hint, fingerprint);
    state.current_recovery = match ras.shared_key {
        Some(shared_key) => {
            let asr = AccessStructureRef::from_root_shared_key(&shared_key);
            let expected_images: Vec<_> = shares
                .iter()
                .filter(|s| {
                    shared_key.share_image(s.held_share.share_image.index)
                        == s.held_share.share_image
                })
                .map(|s| s.held_share.share_image)
                .collect();
            let winning_share_refs = state
                .shares
                .iter()
                .filter(|obs| expected_images.contains(&obs.post.share_image))
                .map(|obs| obs.event_id)
                .collect();
            Some(RecoveredKey {
                access_structure_ref: asr,
                winning_share_refs,
            })
        }
        None => None,
    };
}
