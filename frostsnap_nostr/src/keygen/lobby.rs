use crate::channel::{ChannelKeys, ChannelSecret};
use crate::channel_runner::{
    decode_bincode, extract_e_tags, ChannelMessageDraft, ChannelRunner, ChannelRunnerEvent,
    ChannelRunnerHandle, SendOutcome, BINCODE_CONFIG,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use anyhow::Result;
use frostsnap_coordinator::Sink;
use frostsnap_core::{coordinator::BeginKeygen, device::KeyPurpose, DeviceId, KeygenId};
use nostr_sdk::{nips::nip44, Client, Event, EventBuilder, EventId, Keys, Kind, PublicKey};
use rand_core::OsRng;
use std::collections::{BTreeMap, HashMap};
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
    /// `Register` from the same pubkey. Moves sender to `Ready`.
    Register { devices: Vec<DeviceRegistration> },

    /// Initiator broadcasts the final participant set, threshold, and
    /// per-recipient encrypted subchannel keys. Key name + purpose
    /// are NOT on the wire here — they live on the `ChannelCreation`
    /// event's metadata (see `LobbyChannelMetadata`) and are
    /// retrieved from local `LobbyState` when this event lands.
    /// Accepting (i.e. continuing into keygen) is signalled implicitly
    /// by publishing the first round-1 DKG output on the subchannel —
    /// there is no separate accept message on the lobby channel.
    StartKeygen {
        invites: Vec<SubchannelInvite>,
        threshold: u16,
    },

    /// Explicit, idempotent departure. Removes the sender from
    /// `LobbyState.participants`.
    Leave,

    /// Initiator-only. Signals the round is aborted before
    /// `StartKeygen` — consumers receive `LobbyEvent::Cancelled` and
    /// tear down.
    CancelLobby,

    /// "I confirm — proceed with the keygen referenced by my e-tag."
    /// Empty payload; the carrier is a single NIP-10 `e`-tag pointing
    /// at the `StartKeygen` event id. Receivers ignore acks whose
    /// e-tag doesn't match the current `lobby.keygen.keygen_event_id`,
    /// or whose author isn't in the selected set. The host is treated
    /// as implicitly acked the moment they publish `StartKeygen`, so
    /// they don't need to also publish this.
    AckKeygen,
}

/// Host-authored channel metadata carried inline in the NIP-28
/// `ChannelCreation` event's `content` field. Joiners can paint the
/// wallet name as soon as the creation event lands — no separate
/// `SetKeyName` round-trip. Immutable for the life of the channel.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct LobbyChannelMetadata {
    pub key_name: String,
    pub purpose: KeyPurpose,
}

impl LobbyChannelMetadata {
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
    /// Set when StartKeygen is received and resolved locally (selected coordinators only).
    pub keygen: Option<ResolvedKeygen>,
    /// Pubkeys that have acknowledged the keygen — published `AckKeygen`
    /// referencing `keygen.keygen_event_id`, or are the initiator (who
    /// is seeded here at `StartKeygen`-process time). Empty until
    /// `keygen` is set.
    pub acked: std::collections::BTreeSet<PublicKey>,
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
    }

    pub fn total_device_count(&self) -> usize {
        self.participants
            .values()
            .map(|p| p.commitment.as_ref().map(|c| c.devices.len()).unwrap_or(0))
            .sum()
    }

    /// True when every participant has published `Register` at least
    /// once (status is `Ready`). Empty lobby is `false`.
    pub fn all_ready(&self) -> bool {
        !self.participants.is_empty()
            && self
                .participants
                .values()
                .all(|p| p.status == ParticipantStatus::Ready)
    }

    /// True once every selected participant in `keygen.participants` is
    /// in `acked`. False if `keygen` is `None`.
    pub fn all_acked(&self) -> bool {
        let Some(resolved) = self.keygen.as_ref() else {
            return false;
        };
        resolved.participants.iter().all(|(pk, _)| self.acked.contains(pk))
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
    /// Every selected participant has published `AckKeygen` (or is the
    /// initiator, whose `StartKeygen` publication counts as the implicit
    /// ack). Consumers can now begin the DKG protocol on the subchannel.
    AllAcked,
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

    pub async fn build_creation_event(
        &self,
        keys: &Keys,
        metadata: &LobbyChannelMetadata,
    ) -> Result<Event> {
        let content = metadata.encode_content()?;
        let inner_event = EventBuilder::new(Kind::ChannelCreation, content)
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
                            // host's implicit "I'm in the lobby" signal
                            // AND carries the wallet name + purpose
                            // inline. Insert the initiator as `Joining`
                            // here so the invariant "initiator is set ⇒
                            // initiator is in participants" holds.
                            lobby.initiator = Some(creation.pubkey);
                            lobby.upsert_joining(creation.pubkey);
                            match LobbyChannelMetadata::decode_content(&creation.content) {
                                Ok(meta) => {
                                    lobby.key_name = Some(meta.key_name);
                                    lobby.purpose = Some(meta.purpose);
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        event_id = %creation.id,
                                        error = %e,
                                        "failed to decode lobby channel metadata",
                                    );
                                }
                            }
                            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
                        }
                    }
                    ChannelRunnerEvent::AppEvent { inner_event, ack } => {
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
                        // Signal the dispatch ack AFTER `process_event`
                        // so a local `dispatch` caller only resolves
                        // once the sink has fired with the state
                        // transition. `ack` is `None` for events
                        // arriving via the relay subscription.
                        if let Some(ack) = ack {
                            let _ = ack.send(());
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
    pub async fn announce_presence(&self, keys: &Keys) -> Result<SendOutcome> {
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
    ) -> Result<SendOutcome> {
        let msg = KeygenLobbyMessage::Register { devices };
        self.send(keys, &msg, &[]).await
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
    ) -> Result<SendOutcome> {
        let invites = build_subchannel_invites(keys, selected_coordinators)?;
        let msg = KeygenLobbyMessage::StartKeygen { invites, threshold };
        let e_tags: Vec<EventId> = selected_coordinators
            .iter()
            .map(|selected| selected.register_event_id)
            .collect();
        self.send(keys, &msg, &e_tags).await
    }

    /// Publish `Leave` so other participants remove us from their lobby
    /// view. Idempotent.
    pub async fn leave(&self, keys: &Keys) -> Result<SendOutcome> {
        self.send(keys, &KeygenLobbyMessage::Leave, &[]).await
    }

    /// Publish `CancelLobby` (initiator only — non-initiators will be
    /// ignored receiver-side). Signals all participants that the round
    /// is aborted before `StartKeygen`.
    pub async fn cancel_lobby(&self, keys: &Keys) -> Result<SendOutcome> {
        self.send(keys, &KeygenLobbyMessage::CancelLobby, &[]).await
    }

    /// Publish `AckKeygen` referencing the given `StartKeygen` event id
    /// via a NIP-10 `e`-tag. Caller (Dart, in the live app) is
    /// authoritative on which event id this references — the handle
    /// holds no `LobbyState`. See `FfiPendingKeygen.start_keygen_event_id`.
    pub async fn ack_keygen(
        &self,
        keys: &Keys,
        start_keygen_event_id: EventId,
    ) -> Result<SendOutcome> {
        self.send(keys, &KeygenLobbyMessage::AckKeygen, &[start_keygen_event_id])
            .await
    }

    async fn send(
        &self,
        keys: &Keys,
        msg: &KeygenLobbyMessage,
        e_tags: &[EventId],
    ) -> Result<SendOutcome> {
        let draft = ChannelMessageDraft::app(KIND_FROSTSNAP_KEYGEN_LOBBY, msg, e_tags.to_vec())?;
        self.runner_handle.dispatch(keys, draft).await
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
        KeygenLobbyMessage::StartKeygen { invites, threshold } => {
            if !is_initiator {
                tracing::warn!(event_id = %event_id, author = %author, "StartKeygen from non-initiator, ignoring");
                return;
            }
            // `key_name` and `purpose` ride on the `ChannelCreation`
            // event's metadata (see `LobbyChannelMetadata`). Pull them
            // from our already-populated `LobbyState`. If they're
            // missing, something's wrong with the creation-event
            // decode and we shouldn't proceed.
            let Some(key_name) = lobby.key_name.clone() else {
                tracing::warn!(event_id = %event_id, "StartKeygen before channel metadata arrived; ignoring");
                return;
            };
            let Some(purpose) = lobby.purpose else {
                tracing::warn!(event_id = %event_id, "StartKeygen before channel metadata arrived; ignoring");
                return;
            };

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
                            // Make sure the lobby's `participants` map
                            // covers everyone in the selected set, even
                            // when their `Register` was fetched
                            // on-demand via the relay (e.g. we joined
                            // late and never saw it land live). Without
                            // this, the FFI's `pending.participants`
                            // filter_map would silently drop them.
                            lobby.process_register(
                                reg_event.pubkey,
                                reg_event.id,
                                devices.clone(),
                            );
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

            // Resolve the keygen for *every* receiver, selected or
            // not. The participants list, threshold, and event id are
            // public on the StartKeygen event itself — only the
            // subchannel secret is gated by the per-recipient invite
            // ciphertext. Excluded receivers still want this state so
            // they can see the round is happening and render an
            // appropriate "started without me" banner; FFI consumers
            // detect their own inclusion via
            // `FfiPendingKeygen::includes(my_pubkey)`.
            let resolved = ResolvedKeygen {
                keygen_event_id: event_id,
                participants,
                threshold,
                key_name,
                purpose,
            };
            lobby.keygen = Some(resolved.clone());
            // The initiator is implicitly acked by virtue of having
            // published `StartKeygen` — seed `acked` so the rest of the
            // logic (including `all_acked()`) treats them uniformly.
            // Everyone observes this; the `acked` set is public state.
            lobby.acked.insert(author);
            sink.send(LobbyEvent::LobbyChanged(lobby.clone()));

            // Selected receivers also get `KeygenResolved` (carrying
            // the decrypted subchannel keys) so they can spin up the
            // DKG protocol client. Excluded / decrypt-error receivers
            // see only the public state above.
            match decrypt_subchannel_secret(local_nostr_keys, author, &invites) {
                Ok(Some(channel_keys)) => {
                    sink.send(LobbyEvent::KeygenResolved {
                        resolved,
                        channel_keys,
                    });
                    if lobby.all_acked() {
                        sink.send(LobbyEvent::AllAcked);
                    }
                }
                Ok(None) => {
                    tracing::info!(event_id = %event_id, local_pubkey = %local_nostr_keys.public_key(), "local coordinator not included in private keygen");
                }
                Err(e) => {
                    tracing::warn!(event_id = %event_id, error = %e, "failed to decrypt keygen subchannel secret");
                }
            }
        }
        KeygenLobbyMessage::AckKeygen => {
            let Some(resolved) = lobby.keygen.as_ref() else {
                tracing::warn!(event_id = %event_id, "AckKeygen before StartKeygen, ignoring");
                return;
            };
            let e_tags = extract_e_tags(inner_event);
            if e_tags.first() != Some(&resolved.keygen_event_id) {
                tracing::warn!(event_id = %event_id, "AckKeygen e-tag does not match current StartKeygen, ignoring");
                return;
            }
            let in_selected = resolved.participants.iter().any(|(pk, _)| *pk == author);
            if !in_selected {
                tracing::warn!(event_id = %event_id, %author, "AckKeygen from non-selected participant, ignoring");
                return;
            }
            let inserted = lobby.acked.insert(author);
            if inserted {
                let now_all_acked = lobby.all_acked();
                sink.send(LobbyEvent::LobbyChanged(lobby.clone()));
                if now_all_acked {
                    sink.send(LobbyEvent::AllAcked);
                }
            }
        }
        KeygenLobbyMessage::Leave => {
            // If a *selected* participant leaves after `StartKeygen`,
            // the round is dead — fire `Cancelled` and skip the
            // intermediate `LobbyChanged`. The post-removal snapshot
            // would be torn-down (leaver gone from `participants` but
            // still in `keygen.participants` and possibly `acked`),
            // and consumers are supposed to drop the handle on
            // `Cancelled` anyway. One terminal event, no race.
            let was_selected = lobby
                .keygen
                .as_ref()
                .is_some_and(|r| r.participants.iter().any(|(pk, _)| *pk == author));
            let removed = lobby.participants.remove(&author).is_some();
            // Keep `acked` in sync as a hygiene measure even though
            // nothing reads it after `Cancelled` — defends against
            // a future refactor that does.
            lobby.acked.remove(&author);
            if was_selected {
                sink.send(LobbyEvent::Cancelled);
            } else if removed {
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
