use crate::channel::{ChannelKeys, ChannelSecret};
use crate::channel_runner::{
    decode_bincode, extract_e_tags, ChannelMessageDraft, ChannelRunner, ChannelRunnerEvent,
    ChannelRunnerHandle,
};
use anyhow::Result;
use frostsnap_coordinator::Sink;
use frostsnap_core::{coordinator::BeginKeygen, device::KeyPurpose, DeviceId, KeygenId};
use nostr_sdk::{nips::nip44, Client, Event, EventBuilder, EventId, Keys, Kind, PublicKey};
use rand_core::OsRng;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

pub const KIND_FROSTSNAP_KEYGEN_LOBBY: Kind = Kind::Custom(9002);

/// NIP-40 expiration we set on every outer event published for the
/// keygen lobby. A keygen round shouldn't live beyond a day — stale
/// events beyond that are strictly noise. Cooperating relays will
/// drop them; non-cooperating ones keep them per their own policy.
pub const KEYGEN_MESSAGE_TTL: Duration = Duration::from_secs(24 * 3600);

// =============================================================================
// Wire types
// =============================================================================

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
#[allow(clippy::large_enum_variant)]
pub enum KeygenLobbyMessage {
    /// "I am in the lobby." Sent on join before any device set-up.
    /// Receivers treat the sender as `Joining` unless a prior
    /// `Register` from the same pubkey has already landed.
    Presence,

    /// "I have committed these devices." Supersedes any prior
    /// `Register` from the same pubkey. Moves sender to `Ready` and
    /// clears their acceptance of the current proposed threshold
    /// (re-committing changes the device set — the host's threshold
    /// proposal may no longer make sense and needs to be re-accepted).
    Register { devices: Vec<DeviceRegistration> },

    /// Host-only. Wallet name + purpose set once right after the host
    /// opens the lobby. Immutable for the session — re-sending is a
    /// no-op on receivers.
    SetKeyName {
        key_name: String,
        purpose: KeyPurpose,
    },

    /// Host-only. Broadcasts the proposed threshold. Re-sending
    /// supersedes any prior value AND resets all `AcceptThreshold`
    /// votes (equivalent to the mockup's "unlockThreshold" — any
    /// re-proposal requires everyone to re-accept).
    SetThreshold { threshold: u16 },

    /// Participant consent to the current proposed threshold. Ignored
    /// receiver-side if no threshold is set or the value doesn't match
    /// `state.threshold` (host may have re-proposed meanwhile).
    AcceptThreshold { threshold: u16 },

    /// Initiator broadcasts the final participant set + per-recipient
    /// encrypted subchannel keys. Threshold/key_name/purpose are
    /// embedded so the subchannel coordinator doesn't need to
    /// re-derive them via e-tag lookups.
    StartKeygen {
        invites: Vec<SubchannelInvite>,
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
    },

    /// Explicit, idempotent departure. Removes the sender from
    /// `LobbyState.participants`.
    Leave,

    /// Initiator-only. Signals the round is aborted before
    /// `StartKeygen` — consumers receive `LobbyEvent::Cancelled` and
    /// tear down.
    CancelLobby,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct SubchannelInvite {
    pub recipient: DeviceId,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct DeviceRegistration {
    pub device_id: DeviceId,
    pub name: String,
    pub kind: DeviceKind,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, Eq)]
pub enum DeviceKind {
    Frostsnap,
    AppKey,
}

/// Input to `LobbyHandle::start_keygen`: the initiator's chosen participant set.
#[derive(Clone, Debug)]
pub struct SelectedCoordinator {
    pub register_event_id: EventId,
    pub pubkey: PublicKey,
}

// =============================================================================
// Lobby state
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantStatus {
    /// Joined the lobby, hasn't committed devices yet.
    Joining,
    /// Published a `Register` with their device set.
    Ready,
    /// In `Ready` + accepted the current proposed threshold.
    Accepted,
}

#[derive(Clone, Debug)]
pub struct DeviceCommitment {
    pub devices: Vec<DeviceRegistration>,
}

#[derive(Clone, Debug)]
pub struct ParticipantInfo {
    pub pubkey: PublicKey,
    pub status: ParticipantStatus,
    pub commitment: Option<DeviceCommitment>,
    /// `None` until the participant publishes `Register`.
    /// Consumed by `StartKeygen`'s e-tags for ordering + impersonation
    /// checks, so it stays sticky to the most recent `Register`.
    pub register_event_id: Option<EventId>,
}

#[derive(Clone, Debug, Default)]
pub struct LobbyState {
    pub initiator: Option<PublicKey>,
    pub key_name: Option<String>,
    pub purpose: Option<KeyPurpose>,
    pub participants: BTreeMap<PublicKey, ParticipantInfo>,
    /// Current proposed threshold (set by the host via `SetThreshold`).
    /// Cleared / replaced on re-proposal; all `threshold_accepted_by`
    /// entries are cleared too.
    pub threshold: Option<u16>,
    /// Set of pubkeys who have accepted the current `threshold`. When
    /// `threshold` is re-proposed, this clears.
    pub threshold_accepted_by: BTreeSet<PublicKey>,
    /// Set when StartKeygen is received and resolved locally (selected coordinators only).
    pub keygen: Option<ResolvedKeygen>,
}

/// Resolved keygen parameters from a StartKeygen event. Contains everything
/// needed to call `begin_remote_keygen` on the core coordinator.
#[derive(Clone, Debug)]
pub struct ResolvedKeygen {
    /// The `EventId` of the `StartKeygen` nostr event. Converts to a
    /// `KeygenId` via `to_keygen_id()` / inside `to_begin_keygen()`.
    pub keygen_event_id: EventId,
    /// Ordered list of (coordinator nostr pubkey, their devices). The order
    /// comes from the `StartKeygen` event's e-tag sequence and flows through
    /// `devices_in_order()` into the FROST device list — every coordinator
    /// must agree on this ordering or the DKG will not converge, so a `Vec`
    /// is used rather than a keyed map.
    pub participants: Vec<(PublicKey, Vec<DeviceRegistration>)>,
    pub threshold: u16,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

impl ResolvedKeygen {
    pub fn to_keygen_id(&self) -> KeygenId {
        KeygenId(self.keygen_event_id.to_bytes())
    }

    pub fn coordinator_ids(&self) -> Vec<DeviceId> {
        self.participants
            .iter()
            .map(|(pk, _)| nostr_pubkey_to_device_id(pk))
            .collect()
    }

    pub fn devices_in_order(&self) -> Vec<DeviceId> {
        self.participants
            .iter()
            .flat_map(|(_, devs)| devs.iter().map(|d| d.device_id))
            .collect()
    }

    pub fn to_begin_keygen(&self) -> BeginKeygen {
        BeginKeygen::new_with_id(
            self.devices_in_order(),
            self.threshold,
            self.key_name.clone(),
            self.purpose,
            self.to_keygen_id(),
        )
    }

    /// For each coordinator's nostr pubkey, the set of `DeviceId`s that
    /// coordinator is allowed to send protocol messages as: the coordinator
    /// itself (`nostr_pubkey_to_device_id(pk)`) plus each registered device.
    /// Consumed by `ProtocolClient::run` to reject impersonation.
    pub fn allowed_senders(&self) -> BTreeMap<PublicKey, Vec<DeviceId>> {
        self.participants
            .iter()
            .map(|(pk, devs)| {
                let mut allowed = Vec::with_capacity(devs.len() + 1);
                allowed.push(nostr_pubkey_to_device_id(pk));
                allowed.extend(devs.iter().map(|d| d.device_id));
                (*pk, allowed)
            })
            .collect()
    }
}

impl LobbyState {
    fn upsert_joining(&mut self, author: PublicKey) {
        self.participants
            .entry(author)
            .or_insert_with(|| ParticipantInfo {
                pubkey: author,
                status: ParticipantStatus::Joining,
                commitment: None,
                register_event_id: None,
            });
    }

    fn process_register(
        &mut self,
        author: PublicKey,
        event_id: EventId,
        devices: Vec<DeviceRegistration>,
    ) {
        let entry = self
            .participants
            .entry(author)
            .or_insert_with(|| ParticipantInfo {
                pubkey: author,
                status: ParticipantStatus::Joining,
                commitment: None,
                register_event_id: None,
            });
        entry.commitment = Some(DeviceCommitment { devices });
        entry.register_event_id = Some(event_id);
        entry.status = ParticipantStatus::Ready;
        // Any device-set change invalidates acceptance of the current
        // threshold — the participant has to re-accept.
        self.threshold_accepted_by.remove(&author);
    }

    fn set_threshold(&mut self, threshold: u16) {
        self.threshold = Some(threshold);
        self.threshold_accepted_by.clear();
        // Demote anyone previously Accepted back to Ready.
        for p in self.participants.values_mut() {
            if p.status == ParticipantStatus::Accepted {
                p.status = ParticipantStatus::Ready;
            }
        }
    }

    fn accept_threshold(&mut self, author: PublicKey, threshold: u16) -> bool {
        if self.threshold != Some(threshold) {
            return false;
        }
        let Some(entry) = self.participants.get_mut(&author) else {
            return false;
        };
        if entry.status == ParticipantStatus::Joining {
            return false;
        }
        entry.status = ParticipantStatus::Accepted;
        self.threshold_accepted_by.insert(author)
    }

    pub fn total_device_count(&self) -> usize {
        self.participants
            .values()
            .map(|p| p.commitment.as_ref().map(|c| c.devices.len()).unwrap_or(0))
            .sum()
    }

    /// True when every participant has published `Register` at least
    /// once (status ≥ Ready). Empty lobby is `false`.
    pub fn all_ready(&self) -> bool {
        !self.participants.is_empty()
            && self
                .participants
                .values()
                .all(|p| p.status != ParticipantStatus::Joining)
    }

    /// True when every participant has accepted the current proposed
    /// threshold. Requires `all_ready()` and a threshold to be set.
    pub fn all_accepted(&self) -> bool {
        self.threshold.is_some()
            && !self.participants.is_empty()
            && self
                .participants
                .values()
                .all(|p| p.status == ParticipantStatus::Accepted)
    }
}

// =============================================================================
// Events emitted through the sink
// =============================================================================

#[derive(Clone, Debug)]
pub enum LobbyEvent {
    /// Lobby state changed.
    LobbyChanged(LobbyState),
    /// A `StartKeygen` event has been received, its references resolved, and —
    /// because this coordinator is in the selected set — the private subchannel
    /// secret has been decrypted. The caller should now spin up a
    /// `ProtocolClient` with `channel_keys` + `resolved.keygen_event_id`.
    ///
    /// Not emitted for coordinators excluded from the selected set.
    KeygenResolved {
        resolved: ResolvedKeygen,
        channel_keys: ChannelKeys,
    },
    /// The initiator aborted the lobby with `CancelLobby`. Receivers should
    /// drop their handles.
    Cancelled,
}

// =============================================================================
// Helpers
// =============================================================================

pub fn nostr_pubkey_to_device_id(pubkey: &PublicKey) -> DeviceId {
    let mut bytes = [0u8; 33];
    bytes[0] = 0x02;
    bytes[1..].copy_from_slice(&pubkey.to_bytes());
    DeviceId(bytes)
}

pub fn build_subchannel_invites(
    sender_keys: &Keys,
    selected_coordinators: &[SelectedCoordinator],
) -> Result<Vec<SubchannelInvite>> {
    let protocol_secret = ChannelSecret::random(&mut OsRng);
    selected_coordinators
        .iter()
        .map(|selected| {
            let conversation_key =
                nip44::v2::ConversationKey::derive(sender_keys.secret_key(), &selected.pubkey)?;
            let ciphertext = nip44::v2::encrypt_to_bytes(&conversation_key, &protocol_secret.0)?;
            Ok(SubchannelInvite {
                recipient: nostr_pubkey_to_device_id(&selected.pubkey),
                ciphertext,
            })
        })
        .collect()
}

fn decrypt_subchannel_secret(
    recipient_keys: &Keys,
    sender_pubkey: PublicKey,
    invites: &[SubchannelInvite],
) -> Result<Option<ChannelKeys>> {
    let local_device_id = nostr_pubkey_to_device_id(&recipient_keys.public_key());
    let Some(envelope) = invites
        .iter()
        .find(|envelope| envelope.recipient == local_device_id)
    else {
        return Ok(None);
    };

    let conversation_key =
        nip44::v2::ConversationKey::derive(recipient_keys.secret_key(), &sender_pubkey)?;
    let decrypted = nip44::v2::decrypt_to_bytes(&conversation_key, &envelope.ciphertext)?;
    anyhow::ensure!(
        decrypted.len() == 16,
        "invalid decrypted subchannel secret length: {}",
        decrypted.len()
    );
    let mut secret = [0u8; 16];
    secret.copy_from_slice(&decrypted);
    Ok(Some(ChannelKeys::from_channel_secret(&ChannelSecret(
        secret,
    ))))
}

/// Fetch a referenced inner event from the relay (used when a `StartKeygen`
/// e-tags an event we haven't seen yet).
async fn fetch_inner_event(
    client: &Client,
    channel_keys: &ChannelKeys,
    inner_event_id: EventId,
) -> Option<Event> {
    use crate::channel_runner::decrypt_inner_event;
    use nostr_sdk::{nips::nip44::v2::ConversationKey, Alphabet, Filter, SingleLetterTag};

    let filter = Filter::new()
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::H),
            channel_keys.channel_id_hex(),
        )
        .kind(Kind::Custom(4));

    let conversation_key = ConversationKey::new(channel_keys.shared_secret);

    let events = match client
        .fetch_events(filter, std::time::Duration::from_secs(5))
        .await
    {
        Ok(events) => events,
        Err(e) => {
            tracing::warn!(error = %e, "failed to fetch events for inner event lookup");
            return None;
        }
    };

    let mut found = None;
    for event in events.into_iter() {
        if let Ok(inner) = decrypt_inner_event(&event, &conversation_key) {
            if inner.id == inner_event_id {
                found = Some(inner);
                break;
            }
        }
    }

    if found.is_none() {
        tracing::warn!(%inner_event_id, "could not find referenced inner event on relay");
    }

    found
}

// =============================================================================
// Client
// =============================================================================

pub struct LobbyClient {
    channel_keys: ChannelKeys,
    channel_secret: ChannelSecret,
}

impl LobbyClient {
    pub fn new(channel_secret: ChannelSecret) -> Self {
        let channel_keys = ChannelKeys::from_channel_secret(&channel_secret);
        Self {
            channel_keys,
            channel_secret,
        }
    }

    pub fn invite_link(&self) -> String {
        self.channel_secret.invite_link()
    }

    pub async fn build_creation_event(&self, keys: &Keys) -> Result<Event> {
        let inner_event = EventBuilder::new(Kind::ChannelCreation, "")
            .build(keys.public_key())
            .sign(keys)
            .await?;
        Ok(inner_event)
    }

    pub async fn run(
        self,
        client: Client,
        local_nostr_keys: Keys,
        init_event: Option<Event>,
        sink: impl Sink<LobbyEvent> + Clone + Sync,
    ) -> Result<LobbyHandle> {
        // Hosts identify themselves via the NIP-28 ChannelCreation event;
        // joiners need to publish an explicit Presence so others see them.
        let is_host = init_event.is_some();
        let mut runner = ChannelRunner::new(self.channel_keys.clone())
            .with_message_expiration(KEYGEN_MESSAGE_TTL);
        if let Some(init) = init_event {
            runner = runner.with_init_event(init);
        }
        let (runner_handle, mut events) = runner.run(client.clone()).await?;

        let runner_handle_for_task = runner_handle.clone();
        let channel_keys = self.channel_keys.clone();
        let event_loop_keys = local_nostr_keys.clone();
        tokio::spawn(async move {
            let mut lobby = LobbyState::default();
            let mut events_by_id: HashMap<EventId, Event> = HashMap::new();
            while let Some(event) = events.recv().await {
                match event {
                    ChannelRunnerEvent::CreationEventReceived => {
                        if let Some(creation) = runner_handle_for_task.creation_event() {
                            // The NIP-28 ChannelCreation event is the
                            // host's implicit "I'm in the lobby" signal.
                            // Insert them as `Joining` here so the
                            // invariant "initiator is set ⇒ initiator is
                            // in participants" holds — no separate
                            // Presence required from the host.
                            lobby.initiator = Some(creation.pubkey);
                            lobby.upsert_joining(creation.pubkey);
                            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
                        }
                    }
                    ChannelRunnerEvent::AppEvent { inner_event } => {
                        if inner_event.kind == KIND_FROSTSNAP_KEYGEN_LOBBY {
                            process_event(
                                &inner_event,
                                &mut lobby,
                                &mut events_by_id,
                                &sink,
                                &client,
                                &channel_keys,
                                &event_loop_keys,
                            )
                            .await;
                        }
                    }
                    ChannelRunnerEvent::MembersChanged => {}
                    ChannelRunnerEvent::ChatMessage { .. } => {}
                }
            }
        });

        let handle = LobbyHandle { runner_handle };

        // Joiners publish Presence so others see them in `Joining` state
        // before they've picked devices. Hosts skip — they're already
        // surfaced via the ChannelCreation handler upserting them.
        if !is_host {
            if let Err(e) = handle.announce_presence(&local_nostr_keys).await {
                tracing::warn!(error = %e, "failed to announce initial presence");
            }
        }

        Ok(handle)
    }
}

// =============================================================================
// Handle
// =============================================================================

#[derive(Clone)]
pub struct LobbyHandle {
    runner_handle: ChannelRunnerHandle,
}

impl LobbyHandle {
    /// Announce "I am in the lobby" so others see us in `Joining`
    /// state before we've committed devices. Called by `LobbyClient`
    /// automatically on `run()` start; exposed for manual re-emission.
    pub async fn announce_presence(&self, keys: &Keys) -> Result<EventId> {
        self.send(keys, &KeygenLobbyMessage::Presence, &[]).await
    }

    /// Commit a device set ("Continue with N devices" in the mockup).
    /// Re-callable — each call supersedes the prior commitment and
    /// resets the sender's acceptance of the currently-proposed
    /// threshold.
    pub async fn register_devices(
        &self,
        keys: &Keys,
        devices: Vec<DeviceRegistration>,
    ) -> Result<EventId> {
        let msg = KeygenLobbyMessage::Register { devices };
        self.send(keys, &msg, &[]).await
    }

    /// Host-only. Publish the wallet name + purpose once, immediately
    /// after opening the lobby. Re-sending is a receiver-side no-op.
    pub async fn set_key_name(
        &self,
        keys: &Keys,
        key_name: String,
        purpose: KeyPurpose,
    ) -> Result<EventId> {
        let msg = KeygenLobbyMessage::SetKeyName { key_name, purpose };
        self.send(keys, &msg, &[]).await
    }

    /// Host-only. Propose a threshold. Re-sending supersedes and
    /// clears any prior acceptances.
    pub async fn set_threshold(&self, keys: &Keys, threshold: u16) -> Result<EventId> {
        self.send(keys, &KeygenLobbyMessage::SetThreshold { threshold }, &[])
            .await
    }

    /// Accept the currently-proposed threshold. `threshold` must match
    /// the most recent `SetThreshold`; otherwise the event is ignored
    /// receiver-side.
    pub async fn accept_threshold(&self, keys: &Keys, threshold: u16) -> Result<EventId> {
        self.send(
            keys,
            &KeygenLobbyMessage::AcceptThreshold { threshold },
            &[],
        )
        .await
    }

    /// Publish `StartKeygen` selecting the given coordinators. The resulting
    /// event is echoed back to the initiator's own relay subscription, so the
    /// initiator learns the derived subchannel `ChannelKeys` the same way
    /// everyone else does: via a `LobbyEvent::KeygenResolved` on the sink.
    pub async fn start_keygen(
        &self,
        keys: &Keys,
        selected_coordinators: &[SelectedCoordinator],
        threshold: u16,
        key_name: String,
        purpose: KeyPurpose,
    ) -> Result<()> {
        let invites = build_subchannel_invites(keys, selected_coordinators)?;
        let msg = KeygenLobbyMessage::StartKeygen {
            invites,
            threshold,
            key_name,
            purpose,
        };
        let e_tags: Vec<EventId> = selected_coordinators
            .iter()
            .map(|selected| selected.register_event_id)
            .collect();
        self.send(keys, &msg, &e_tags).await?;
        Ok(())
    }

    /// Publish `Leave` so other participants remove us from their lobby
    /// view. Idempotent.
    pub async fn leave(&self, keys: &Keys) -> Result<EventId> {
        self.send(keys, &KeygenLobbyMessage::Leave, &[]).await
    }

    /// Publish `CancelLobby` (initiator only — non-initiators will be
    /// ignored receiver-side). Signals all participants that the round
    /// is aborted before `StartKeygen`.
    pub async fn cancel_lobby(&self, keys: &Keys) -> Result<EventId> {
        self.send(keys, &KeygenLobbyMessage::CancelLobby, &[]).await
    }

    async fn send(
        &self,
        keys: &Keys,
        msg: &KeygenLobbyMessage,
        e_tags: &[EventId],
    ) -> Result<EventId> {
        let draft = ChannelMessageDraft::app(KIND_FROSTSNAP_KEYGEN_LOBBY, msg, e_tags.to_vec())?;
        let prepared = self.runner_handle.send(keys, draft).await?;
        Ok(prepared.id)
    }
}

// =============================================================================
// Event processing
// =============================================================================

#[allow(clippy::too_many_arguments)]
async fn process_event(
    inner_event: &Event,
    lobby: &mut LobbyState,
    events_by_id: &mut HashMap<EventId, Event>,
    sink: &impl Sink<LobbyEvent>,
    client: &Client,
    channel_keys: &ChannelKeys,
    local_nostr_keys: &Keys,
) {
    let event_id = inner_event.id;
    let author = inner_event.pubkey;

    events_by_id.insert(event_id, inner_event.clone());

    let msg: KeygenLobbyMessage = match decode_bincode(inner_event) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(event_id = %event_id, error = %e, "failed to decode keygen lobby message");
            return;
        }
    };

    let is_initiator = lobby.initiator.as_ref() == Some(&author);

    match msg {
        KeygenLobbyMessage::Presence => {
            lobby.upsert_joining(author);
            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
        }
        KeygenLobbyMessage::Register { devices } => {
            lobby.process_register(author, event_id, devices);
            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
        }
        KeygenLobbyMessage::SetKeyName { key_name, purpose } => {
            if !is_initiator {
                tracing::warn!(event_id = %event_id, author = %author, "SetKeyName from non-initiator, ignoring");
                return;
            }
            // Immutable once set — ignore repeats.
            if lobby.key_name.is_none() {
                lobby.key_name = Some(key_name);
                lobby.purpose = Some(purpose);
                sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
            }
        }
        KeygenLobbyMessage::SetThreshold { threshold } => {
            if !is_initiator {
                tracing::warn!(event_id = %event_id, author = %author, "SetThreshold from non-initiator, ignoring");
                return;
            }
            lobby.set_threshold(threshold);
            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
        }
        KeygenLobbyMessage::AcceptThreshold { threshold } => {
            if lobby.accept_threshold(author, threshold) {
                sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
            } else {
                tracing::debug!(event_id = %event_id, author = %author, "AcceptThreshold ignored (no matching threshold or sender not Ready)");
            }
        }
        KeygenLobbyMessage::StartKeygen {
            invites,
            threshold,
            key_name,
            purpose,
        } => {
            if !is_initiator {
                tracing::warn!(event_id = %event_id, author = %author, "StartKeygen from non-initiator, ignoring");
                return;
            }

            let e_tags = extract_e_tags(inner_event);
            if e_tags.is_empty() {
                tracing::warn!(event_id = %event_id, "StartKeygen has no e-tags");
                return;
            }

            let register_event_ids = &e_tags[..];

            // Resolve referenced events, fetching from relay if not yet seen.
            for &ref_id in register_event_ids {
                if !events_by_id.contains_key(&ref_id) {
                    if let Some(inner) = fetch_inner_event(client, channel_keys, ref_id).await {
                        events_by_id.insert(inner.id, inner);
                    }
                }
            }

            let mut participants = Vec::new();
            for &reg_id in register_event_ids {
                match events_by_id.get(&reg_id) {
                    Some(reg_event) => match decode_bincode::<KeygenLobbyMessage>(reg_event) {
                        Ok(KeygenLobbyMessage::Register { devices }) => {
                            participants.push((reg_event.pubkey, devices));
                        }
                        Ok(other) => {
                            tracing::warn!(%reg_id, msg_type = ?std::mem::discriminant(&other), "e-tag references non-Register event");
                        }
                        Err(e) => {
                            tracing::warn!(%reg_id, error = %e, "failed to decode referenced register event");
                        }
                    },
                    None => {
                        tracing::warn!(%reg_id, "referenced register event not found after fetch");
                    }
                }
            }

            let protocol_channel_keys = match decrypt_subchannel_secret(
                local_nostr_keys,
                author,
                &invites,
            ) {
                Ok(Some(keys)) => keys,
                Ok(None) => {
                    tracing::info!(event_id = %event_id, local_pubkey = %local_nostr_keys.public_key(), "local coordinator not included in private keygen");
                    return;
                }
                Err(e) => {
                    tracing::warn!(event_id = %event_id, error = %e, "failed to decrypt keygen subchannel secret");
                    return;
                }
            };

            let resolved = ResolvedKeygen {
                keygen_event_id: event_id,
                participants,
                threshold,
                key_name,
                purpose,
            };
            lobby.keygen = Some(resolved.clone());
            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
            sink.send(LobbyEvent::KeygenResolved {
                resolved,
                channel_keys: protocol_channel_keys,
            });
        }
        KeygenLobbyMessage::Leave => {
            if lobby.participants.remove(&author).is_some() {
                sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
            }
        }
        KeygenLobbyMessage::CancelLobby => {
            if !is_initiator {
                tracing::warn!(event_id = %event_id, author = %author, "CancelLobby from non-initiator, ignoring");
                return;
            }
            sink.send(LobbyEvent::Cancelled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subchannel_invite_roundtrips() {
        let sender = Keys::generate();
        let recipient = Keys::generate();
        let selected = vec![SelectedCoordinator {
            register_event_id: EventId::all_zeros(),
            pubkey: recipient.public_key(),
        }];

        let invites = build_subchannel_invites(&sender, &selected).unwrap();
        let decrypted = decrypt_subchannel_secret(&recipient, sender.public_key(), &invites)
            .unwrap()
            .unwrap();

        assert_ne!(decrypted.channel_id.0, [0u8; 32]);
        assert_ne!(decrypted.shared_secret, [0u8; 32]);
    }

    #[test]
    fn subchannel_invite_missing_recipient_returns_none() {
        let sender = Keys::generate();
        let recipient = Keys::generate();
        let other = Keys::generate();
        let selected = vec![SelectedCoordinator {
            register_event_id: EventId::all_zeros(),
            pubkey: recipient.public_key(),
        }];

        let invites = build_subchannel_invites(&sender, &selected).unwrap();

        assert!(
            decrypt_subchannel_secret(&other, sender.public_key(), &invites)
                .unwrap()
                .is_none()
        );
    }
}
