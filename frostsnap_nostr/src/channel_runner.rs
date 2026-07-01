use crate::channel::ChannelKeys;
use crate::{EventId, PublicKey};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use nostr_sdk::{
    nips::nip44::v2::{self, ConversationKey},
    pool::Output,
    Alphabet, Client, Event, EventBuilder, Filter, Keys, Kind, Metadata, RelayPoolNotification,
    RelayUrl, SingleLetterTag, SubscriptionId, SyncOptions, Tag, TagKind, Timestamp,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const PROFILE_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

// =============================================================================
// Event metadata
// =============================================================================

/// The three outer-nostr-event fields that downstream decoders (e.g. the
/// signing tree) need alongside the decoded inner payload. Extracting this
/// once at the decode boundary avoids passing the full `nostr_sdk::Event`
/// into layers that only care about identity, author, and time.
#[derive(Debug, Clone)]
pub struct EventMeta {
    pub event_id: EventId,
    pub author: PublicKey,
    pub timestamp: u64,
}

impl EventMeta {
    pub fn from_event(event: &Event) -> Self {
        Self {
            event_id: event.id.into(),
            author: event.pubkey.into(),
            timestamp: event.created_at.as_secs(),
        }
    }
}

// =============================================================================
// Nostr profile (NIP-01 kind 0 metadata)
// =============================================================================

/// Nostr profile metadata (NIP-01 kind 0 event content). Lives with the
/// channel runner because it's the runner that fetches and caches profiles
/// as it sees author pubkeys go past.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NostrProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<PublicKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nip05: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
}

impl NostrProfile {
    pub fn from_metadata(pubkey: PublicKey, metadata: Metadata) -> Self {
        Self {
            pubkey: Some(pubkey),
            name: metadata.name,
            display_name: metadata.display_name,
            about: metadata.about,
            picture: metadata.picture,
            banner: metadata.banner,
            nip05: metadata.nip05,
            website: metadata.website,
        }
    }
}

/// How a member's profile arrived in `ChannelState.members`. Tracks
/// the precedence rule: in-channel publications win over external
/// public kind-0 fetches once they exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSource {
    /// Decrypted from an in-channel `Kind::Metadata` event.
    InChannel,
    /// Fetched via the channel runner's `spawn_profile_fetch` path
    /// (cache → relay).
    External,
}

/// Runner-internal wrapper around a member's `NostrProfile` that
/// carries source / ordering metadata not part of the wire profile.
#[derive(Debug, Clone)]
pub struct MemberSlot {
    pub profile: NostrProfile,
    pub source: ProfileSource,
    /// Set only when `source == InChannel`: the `created_at` of the
    /// inner metadata event the profile came from. Used for the
    /// strict-greater-wins fold rule during cache replay.
    pub inchannel_created_at: Option<u64>,
}

// =============================================================================
// Shared state
// =============================================================================

#[derive(Debug, Default)]
pub struct ChannelState {
    pub members: HashMap<PublicKey, Option<MemberSlot>>,
    pub creation_event: Option<Event>,
}

// =============================================================================
// Send outcome
// =============================================================================

/// Result of a successful `ChannelRunnerHandle::dispatch`: the inner
/// event id (what peers see after decryption, used by protocols for
/// e-tag references) plus per-relay publish outcome from the nostr-sdk
/// `Output<EventId>`. Callers decide what to do with `relay_failed`;
/// `any_relay_success` is the minimum bar for a useful publish.
#[derive(Debug, Clone)]
pub struct SendOutcome {
    pub inner_event_id: EventId,
    pub relay_success: HashSet<RelayUrl>,
    pub relay_failed: HashMap<RelayUrl, String>,
}

impl SendOutcome {
    pub fn any_relay_success(&self) -> bool {
        !self.relay_success.is_empty()
    }
}

// =============================================================================
// Events emitted by the runner
// =============================================================================

#[derive(Debug)]
pub enum ChannelRunnerEvent {
    ChatMessage {
        message_id: EventId,
        author: PublicKey,
        content: String,
        timestamp: u64,
        reply_to: Option<EventId>,
    },
    /// Domain-specific inner event for consumers to decode. `ack` is
    /// `Some` only for events coming from a local `dispatch` — the
    /// dispatch caller awaits this oneshot, so the consumer must
    /// signal it after `process_event` has run (state updated +
    /// `Sink<T>` notified). Incoming-from-subscription events carry
    /// `ack: None`.
    AppEvent {
        inner_event: Event,
        ack: Option<oneshot::Sender<()>>,
    },
    MembersChanged,
    /// A single member's profile entered or updated `state.members`.
    /// Per-author granularity, complementing the coarser
    /// `MembersChanged` event which only signals "set of members
    /// changed."
    MemberProfileUpdated {
        pubkey: PublicKey,
        profile: NostrProfile,
    },
    CreationEventReceived,
}

// =============================================================================
// Internal command sent through `cmd_tx` to the runner's background task.
// =============================================================================

enum RunnerCmd {
    /// Publish an already-prepared inner event. On ≥1 relay success,
    /// apply locally through the same path as incoming events (carrying
    /// an ack oneshot) and signal `done` with `Ok(SendOutcome)`. On
    /// zero relay success or publish error, signal `done` with `Err`
    /// and DO NOT apply locally.
    Dispatch {
        inner_event: Event,
        done: oneshot::Sender<Result<SendOutcome>>,
    },
    /// Publish without applying locally. The event will still come
    /// through the subscription later (deduped via `seen_ids` if we
    /// see the same outer id). Used by consumers that manage their
    /// own optimistic local state (e.g. chat's `Pending` → `Sent`
    /// status flow, where the local insert happens synchronously in
    /// the consumer before the publish begins).
    Publish {
        inner_event: Event,
        done: oneshot::Sender<Result<SendOutcome>>,
    },
}

// =============================================================================
// Runner
// =============================================================================

pub struct ChannelRunner {
    channel_keys: ChannelKeys,
    state: Arc<Mutex<ChannelState>>,
    /// Optional inner event to publish as channel creation if not already present
    init_event: Option<Event>,
    /// Duration after which outer events should be marked for expiry
    /// via NIP-40. None means no expiration tag — relays keep events
    /// per their own retention policy. `Some(d)` tags every published
    /// outer event with `expiration = now() + d`. Intended for
    /// short-lived coordination channels (keygen lobby, keygen protocol
    /// subchannel) where stale events are useless after the round.
    message_expiration: Option<Duration>,
}

impl ChannelRunner {
    pub fn new(channel_keys: ChannelKeys) -> Self {
        Self {
            channel_keys,
            state: Arc::new(Mutex::new(ChannelState::default())),
            init_event: None,
            message_expiration: None,
        }
    }

    /// Set the inner channel creation event to publish if one doesn't already
    /// exist on relays. The caller builds the domain-specific init event; the
    /// runner handles lookup, encryption, and wrapping.
    pub fn with_init_event(mut self, init_event: Event) -> Self {
        self.init_event = Some(init_event);
        self
    }

    /// Tag every outer event with NIP-40 expiration `now() + duration`
    /// so cooperating relays can drop them after the round. Not
    /// enforced — relays may ignore the tag — but it signals intent.
    pub fn with_message_expiration(mut self, duration: Duration) -> Self {
        self.message_expiration = Some(duration);
        self
    }

    pub async fn fetch_init_event(&self, client: &Client) -> Result<Option<Event>> {
        let channel_id_hex = self.channel_keys.channel_id_hex();
        let conversation_key = ConversationKey::new(self.channel_keys.shared_secret);
        fetch_init_event_with_key(client, &channel_id_hex, &conversation_key).await
    }

    pub async fn run(
        self,
        client: Client,
    ) -> Result<(ChannelRunnerHandle, mpsc::Receiver<ChannelRunnerEvent>)> {
        let channel_id_hex = self.channel_keys.channel_id_hex();
        let conversation_key = ConversationKey::new(self.channel_keys.shared_secret);

        let filter = Filter::new()
            .custom_tag(
                SingleLetterTag::lowercase(Alphabet::H),
                channel_id_hex.clone(),
            )
            .kind(Kind::Custom(4));

        let message_expiration = self.message_expiration;
        let mut startup_init_event = None;
        let mut published_init_event = None;
        if let Some(init_event) = &self.init_event {
            let existing_init = match fetch_init_event_with_key(
                &client,
                &channel_id_hex,
                &conversation_key,
            )
            .await
            {
                Ok(existing) => existing,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to fetch channel init");
                    None
                }
            };

            if let Some(existing) = existing_init {
                startup_init_event = Some(existing);
            } else {
                match send_prepared_message(
                    &client,
                    &self.channel_keys,
                    init_event.clone(),
                    expiration_from(message_expiration),
                )
                .await
                {
                    Ok(output) if !output.success.is_empty() => {
                        published_init_event = Some((output.val, init_event.clone()));
                    }
                    Ok(output) => {
                        tracing::warn!("no relay accepted channel init: {:?}", output.failed,);
                    }
                    Err(e) => tracing::warn!(error = %e, "failed to publish channel init"),
                }
            }
        }

        let (event_tx, event_rx) = mpsc::channel::<ChannelRunnerEvent>(64);
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<RunnerCmd>(32);
        let (profile_tx, mut profile_rx) = mpsc::channel::<(PublicKey, NostrProfile)>(32);
        // Shutdown signal held by every `ChannelRunnerHandle` clone.
        // Explicit `shutdown()` flips it; last handle-clone drop closes
        // the channel (Err on `changed()`) — either path breaks the
        // runner's select loop.
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        // Step 1: Replay cached events from local database immediately.
        // This ensures the UI shows historical events even without internet.
        let mut seen_ids = HashSet::<nostr_sdk::EventId>::new();
        let mut seen_inner_ids = HashSet::<nostr_sdk::EventId>::new();
        if let Some(inner_event) = startup_init_event {
            process_inner_event_once(
                &inner_event,
                &mut seen_inner_ids,
                &self.state,
                &client,
                &profile_tx,
                &event_tx,
                None,
            )
            .await;
        }
        if let Some((outer_event_id, inner_event)) = published_init_event {
            seen_ids.insert(outer_event_id);
            process_inner_event_once(
                &inner_event,
                &mut seen_inner_ids,
                &self.state,
                &client,
                &profile_tx,
                &event_tx,
                None,
            )
            .await;
        }
        let stored_events = client.database().query(filter.clone()).await?;
        tracing::debug!(count = stored_events.len(), "loaded cached events");
        // Oldest first so events are processed in order
        for event in stored_events.to_vec().into_iter().rev() {
            seen_ids.insert(event.id);
            if let Ok(inner) = decrypt_inner_event(&event, &conversation_key) {
                process_inner_event_once(
                    &inner,
                    &mut seen_inner_ids,
                    &self.state,
                    &client,
                    &profile_tx,
                    &event_tx,
                    None,
                )
                .await;
            }
        }

        // Step 2: Subscribe for live events from relays
        let channel_sub_id: SubscriptionId = client.subscribe(filter.clone(), None).await?.val;
        let channel_keys = self.channel_keys.clone();
        let state = self.state.clone();

        // Step 3: Sync with relays in background. Sync-discovered events don't
        // arrive through our subscription (sync uses its own ephemeral
        // subscription), so after sync we query the database and emit anything
        // we haven't already seen.
        let (sync_done_tx, mut sync_done_rx) = mpsc::channel::<()>(1);
        {
            let sync_client = client.clone();
            let sync_filter = filter;
            tokio::spawn(async move {
                let sync_opts = SyncOptions::default();
                if let Err(e) = sync_client.sync(sync_filter, &sync_opts).await {
                    tracing::warn!(error = %e, "background sync failed");
                }
                let _ = sync_done_tx.send(()).await;
            });
        }

        let mut shutdown_rx_task = shutdown_rx.clone();
        tokio::spawn(async move {
            let mut notifications = client.notifications();

            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx_task.changed() => {
                        // close() (or Drop) was called on a handle —
                        // exit cleanly.
                        break;
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(RunnerCmd::Publish { inner_event, done }) => {
                                let inner_event_id: EventId = inner_event.id.into();
                                let send_result = send_prepared_message(
                                    &client,
                                    &channel_keys,
                                    inner_event,
                                    expiration_from(message_expiration),
                                ).await;
                                match send_result {
                                    Err(e) => {
                                        let _ = done.send(Err(e));
                                    }
                                    Ok(output) => {
                                        // Mark outer id seen so the
                                        // subscription echo is filtered —
                                        // the caller gets the relay result
                                        // via `Output`, the inner event
                                        // will still be processed via
                                        // `seen_inner_ids` dedup if it
                                        // reaches us any other way (e.g.
                                        // the consumer has already done
                                        // its optimistic local insert).
                                        seen_ids.insert(output.val);
                                        let _ = done.send(Ok(SendOutcome {
                                            inner_event_id,
                                            relay_success: output.success,
                                            relay_failed: output.failed,
                                        }));
                                    }
                                }
                            }
                            Some(RunnerCmd::Dispatch { inner_event, done }) => {
                                // Inner id is known up-front (the event is
                                // already signed by the user keys before
                                // arriving here). Surface this — not the
                                // outer id — to callers that build protocol
                                // references (e.g. keygen's StartKeygen
                                // e-tags point at Register events' inner ids).
                                let inner_event_id: EventId = inner_event.id.into();
                                let send_result = send_prepared_message(
                                    &client,
                                    &channel_keys,
                                    inner_event.clone(),
                                    expiration_from(message_expiration),
                                ).await;

                                match send_result {
                                    Err(e) => {
                                        let _ = done.send(Err(e));
                                    }
                                    Ok(output) if output.success.is_empty() => {
                                        let failed = output.failed.clone();
                                        let _ = done.send(Err(anyhow!(
                                            "no relay accepted the event: {:?}",
                                            failed
                                        )));
                                        // DO NOT apply locally — keeps
                                        // local state consistent with what
                                        // peers see.
                                    }
                                    Ok(output) => {
                                        seen_ids.insert(output.val);
                                        // Apply locally through the SAME
                                        // path as incoming events. Carry
                                        // an ack so the dispatch future
                                        // resolves only after the consumer
                                        // has finished processing.
                                        let (apply_tx, apply_rx) = oneshot::channel();
                                        process_inner_event_once(
                                            &inner_event,
                                            &mut seen_inner_ids,
                                            &state,
                                            &client,
                                            &profile_tx,
                                            &event_tx,
                                            Some(apply_tx),
                                        )
                                        .await;
                                        // `apply_rx` resolves when the
                                        // consumer signals the ack (or
                                        // immediately for non-AppEvent
                                        // kinds — see
                                        // `process_inner_event`). If the
                                        // consumer has been dropped we
                                        // still surface the relay outcome
                                        // — the network succeeded.
                                        let _ = apply_rx.await;
                                        let _ = done.send(Ok(SendOutcome {
                                            inner_event_id,
                                            relay_success: output.success,
                                            relay_failed: output.failed,
                                        }));
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    Some((pubkey, profile)) = profile_rx.recv() => {
                        let updated = {
                            let mut s = state.lock().unwrap();
                            // External fetches only write when the slot
                            // is empty or already External — never
                            // override an in-channel profile (precedence
                            // rule: in-channel wins).
                            match s.members.get(&pubkey) {
                                Some(Some(slot)) if slot.source == ProfileSource::InChannel => {
                                    None
                                }
                                _ => {
                                    let slot = MemberSlot {
                                        profile: profile.clone(),
                                        source: ProfileSource::External,
                                        inchannel_created_at: None,
                                    };
                                    s.members.insert(pubkey, Some(slot));
                                    Some(profile)
                                }
                            }
                        };
                        if let Some(profile) = updated {
                            let _ = event_tx.send(ChannelRunnerEvent::MembersChanged).await;
                            let _ = event_tx
                                .send(ChannelRunnerEvent::MemberProfileUpdated { pubkey, profile })
                                .await;
                        }
                    }
                    Some(()) = sync_done_rx.recv() => {
                        // Sync finished — query DB for any events we missed
                        let db_filter = Filter::new()
                            .custom_tag(
                                SingleLetterTag::lowercase(Alphabet::H),
                                channel_keys.channel_id_hex(),
                            )
                            .kind(Kind::Custom(4));
                        match client.database().query(db_filter).await {
                            Ok(events) => {
                                for event in events.to_vec().into_iter().rev() {
                                    if !seen_ids.insert(event.id) {
                                        continue;
                                    }
                                    if let Ok(inner) = decrypt_inner_event(&event, &conversation_key) {
                                        process_inner_event_once(
                                            &inner,
                                            &mut seen_inner_ids,
                                            &state,
                                            &client,
                                            &profile_tx,
                                            &event_tx,
                                            None,
                                        )
                                        .await;
                                    }
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, "failed to query after sync"),
                        }
                    }
                    notification = notifications.recv() => {
                        match notification {
                            Ok(RelayPoolNotification::Event { subscription_id, event, .. }) => {
                                if subscription_id != channel_sub_id {
                                    continue;
                                }
                                if !seen_ids.insert(event.id) {
                                    continue;
                                }
                                tracing::debug!(event_id = %event.id, sub = %channel_sub_id, "runner received outer event");
                                let inner = match decrypt_inner_event(&event, &conversation_key) {
                                    Ok(e) => e,
                                    Err(e) => {
                                        tracing::warn!(event_id = %event.id, error = %e, "failed to decrypt");
                                        continue;
                                    }
                                };
                                process_inner_event_once(
                                    &inner,
                                    &mut seen_inner_ids,
                                    &state,
                                    &client,
                                    &profile_tx,
                                    &event_tx,
                                    None,
                                )
                                .await;
                            }
                            Ok(RelayPoolNotification::Shutdown) => break,
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(error = %e, "notification error");
                            }
                        }
                    }
                }
            }
            // Unsubscribe from the relay so it stops sending events
            // we'll never process.
            client.unsubscribe(&channel_sub_id).await;
            tracing::debug!(sub = %channel_sub_id, "channel runner exited");
        });

        let handle = ChannelRunnerHandle {
            cmd_tx,
            state: self.state,
            channel_keys: self.channel_keys,
            shutdown_tx,
        };

        Ok((handle, event_rx))
    }
}

// =============================================================================
// Handle
// =============================================================================

#[derive(Clone)]
pub struct ChannelRunnerHandle {
    cmd_tx: mpsc::Sender<RunnerCmd>,
    state: Arc<Mutex<ChannelState>>,
    channel_keys: ChannelKeys,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

#[derive(Clone, Debug)]
pub struct ChannelMessageDraft {
    kind: Kind,
    content: String,
    reply_to: Vec<EventId>,
}

impl ChannelMessageDraft {
    pub fn text(content: impl Into<String>, reply_to: Vec<EventId>) -> Self {
        Self {
            kind: Kind::ChannelMessage,
            content: content.into(),
            reply_to,
        }
    }

    pub fn app(kind: Kind, payload: &impl bincode::Encode, reply_to: Vec<EventId>) -> Result<Self> {
        let encoded = bincode::encode_to_vec(payload, BINCODE_CONFIG)?;
        Ok(Self {
            kind,
            content: BASE64.encode(&encoded),
            reply_to,
        })
    }

    pub async fn prepare(self, user_keys: &Keys) -> Result<Event> {
        let mut builder = EventBuilder::new(self.kind, self.content);
        for event_id in self.reply_to {
            builder = builder.tag(Tag::event(nostr_sdk::EventId::from(event_id)));
        }

        let inner_event = builder
            .build(user_keys.public_key())
            .sign(user_keys)
            .await?;
        Ok(inner_event)
    }
}

impl ChannelRunnerHandle {
    /// Publish a prepared inner event to relays, then — if at least
    /// one relay OK'd it — apply it locally through the same path as
    /// incoming subscription events (so consumer `Sink<T>`s fire the
    /// same way). Resolves only after both the relay OK and the
    /// consumer's processing complete. On zero relay success, returns
    /// `Err` WITHOUT applying locally.
    pub async fn dispatch_prepared(&self, prepared: Event) -> Result<SendOutcome> {
        let (done_tx, done_rx) = oneshot::channel();
        self.cmd_tx
            .send(RunnerCmd::Dispatch {
                inner_event: prepared,
                done: done_tx,
            })
            .await
            .map_err(|_| anyhow!("channel runner stopped"))?;
        done_rx
            .await
            .map_err(|_| anyhow!("channel runner dropped pending dispatch"))?
    }

    /// Convenience: build the inner event via
    /// `ChannelMessageDraft::prepare` and dispatch it.
    pub async fn dispatch(
        &self,
        user_keys: &Keys,
        draft: ChannelMessageDraft,
    ) -> Result<SendOutcome> {
        let prepared = draft.prepare(user_keys).await?;
        self.dispatch_prepared(prepared).await
    }

    /// Pure transport — publish to relays, don't apply locally.
    /// Use when the consumer handles its own optimistic local state
    /// and only needs the relay outcome (chat's `Pending` → `Sent`
    /// status transition is the canonical example). The
    /// `SendOutcome` is the same shape as `dispatch`; callers
    /// inspect `any_relay_success` + `relay_failed` as they see fit.
    pub async fn publish_prepared(&self, prepared: Event) -> Result<SendOutcome> {
        let (done_tx, done_rx) = oneshot::channel();
        self.cmd_tx
            .send(RunnerCmd::Publish {
                inner_event: prepared,
                done: done_tx,
            })
            .await
            .map_err(|_| anyhow!("channel runner stopped"))?;
        done_rx
            .await
            .map_err(|_| anyhow!("channel runner dropped pending publish"))?
    }

    /// Convenience: build the inner event via
    /// `ChannelMessageDraft::prepare` and publish it.
    pub async fn publish(
        &self,
        user_keys: &Keys,
        draft: ChannelMessageDraft,
    ) -> Result<SendOutcome> {
        let prepared = draft.prepare(user_keys).await?;
        self.publish_prepared(prepared).await
    }

    /// Public, profile-only view of the channel membership. Slot
    /// metadata (source, ordering) stays runner-internal.
    pub fn members(&self) -> HashMap<PublicKey, Option<NostrProfile>> {
        self.state
            .lock()
            .unwrap()
            .members
            .iter()
            .map(|(pk, slot_opt)| (*pk, slot_opt.as_ref().map(|s| s.profile.clone())))
            .collect()
    }

    /// Single-author profile lookup — used by the publish path's
    /// dedup check before sending a new in-channel kind 0.
    pub fn member_profile(&self, pubkey: &PublicKey) -> Option<NostrProfile> {
        self.state
            .lock()
            .unwrap()
            .members
            .get(pubkey)
            .and_then(|slot_opt| slot_opt.as_ref())
            .map(|slot| slot.profile.clone())
    }

    pub fn creation_event(&self) -> Option<Event> {
        self.state.lock().unwrap().creation_event.clone()
    }

    /// Shared handle to the folded channel state. Wrapper tasks that
    /// only need to read state should use this instead of cloning the
    /// full `ChannelRunnerHandle` — a full clone drags `shutdown_tx`
    /// with it and would defeat the drop-based teardown chain.
    pub fn state_arc(&self) -> Arc<Mutex<ChannelState>> {
        Arc::clone(&self.state)
    }

    pub fn channel_keys(&self) -> &ChannelKeys {
        &self.channel_keys
    }

    /// Signal the runner to shut down. Returns immediately. The runner
    /// task observes the signal on its next `select!` poll, breaks out
    /// of its loop, unsubscribes from the relay, and exits. Idempotent.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Publish the local user's `NostrProfile` as an encrypted
    /// kind 0 inside this channel. The same implementation is
    /// used by both the chat wrapper and the lobby wrapper —
    /// publish/dedup/fold all belong to the encrypted-channel
    /// abstraction.
    ///
    /// Skips if the runner's view of the local user's profile
    /// already matches `profile` (after populating
    /// `profile.pubkey` from `keys`). On publish, the runner's
    /// own fold updates `members[author]` when the event
    /// echoes back from the relay subscription.
    pub async fn publish_profile(
        &self,
        profile: NostrProfile,
        keys: &Keys,
    ) -> Result<Option<EventId>> {
        let mut to_publish = profile;
        let local_pubkey: PublicKey = keys.public_key().into();
        to_publish.pubkey = Some(local_pubkey);
        if let Some(existing) = self.member_profile(&local_pubkey) {
            if existing == to_publish {
                return Ok(None);
            }
        }
        let metadata = nostr_sdk::Metadata {
            name: to_publish.name.clone(),
            display_name: to_publish.display_name.clone(),
            about: to_publish.about.clone(),
            picture: to_publish.picture.clone(),
            banner: to_publish.banner.clone(),
            nip05: to_publish.nip05.clone(),
            website: to_publish.website.clone(),
            ..Default::default()
        };
        let content = serde_json::to_string(&metadata)?;
        let inner_event = nostr_sdk::EventBuilder::new(Kind::Metadata, content)
            .build(keys.public_key())
            .sign(keys)
            .await?;
        let outcome = self.dispatch_prepared(inner_event).await?;
        Ok(Some(outcome.inner_event_id))
    }

    /// Fire-and-forget version of `publish_profile`. Spawns a
    /// tokio task that publishes (or no-ops via the dedup
    /// inside `publish_profile`); errors are logged at WARN
    /// level. Use at connect time when the caller doesn't need
    /// to await the result.
    pub fn spawn_publish_profile(self, profile: NostrProfile, keys: Keys) {
        tokio::spawn(async move {
            if let Err(e) = self.publish_profile(profile, &keys).await {
                tracing::warn!(error = %e, "spawn_publish_profile failed");
            }
        });
    }
}

// =============================================================================
// Helpers
// =============================================================================

async fn process_inner_event_once(
    inner: &Event,
    seen_inner_ids: &mut HashSet<nostr_sdk::EventId>,
    state: &Arc<Mutex<ChannelState>>,
    client: &Client,
    profile_tx: &mpsc::Sender<(PublicKey, NostrProfile)>,
    event_tx: &mpsc::Sender<ChannelRunnerEvent>,
    ack: Option<oneshot::Sender<()>>,
) {
    if !seen_inner_ids.insert(inner.id) {
        // Duplicate — signal the ack anyway so a racing dispatch
        // (e.g. relay echo arriving before our own apply completes)
        // doesn't deadlock the caller.
        if let Some(ack) = ack {
            let _ = ack.send(());
        }
        return;
    }
    process_inner_event(inner, state, client, profile_tx, event_tx, ack).await;
}

async fn process_inner_event(
    inner: &Event,
    state: &Arc<Mutex<ChannelState>>,
    client: &Client,
    profile_tx: &mpsc::Sender<(PublicKey, NostrProfile)>,
    event_tx: &mpsc::Sender<ChannelRunnerEvent>,
    ack: Option<oneshot::Sender<()>>,
) {
    let author: PublicKey = inner.pubkey.into();
    let is_new_member = {
        let mut s = state.lock().unwrap();
        if let std::collections::hash_map::Entry::Vacant(e) = s.members.entry(author) {
            e.insert(None);
            true
        } else {
            false
        }
    };
    if is_new_member {
        spawn_profile_fetch(author, client.clone(), profile_tx.clone());
        let _ = event_tx.send(ChannelRunnerEvent::MembersChanged).await;
    }

    if inner.kind == Kind::Metadata {
        let updated = match serde_json::from_str::<Metadata>(&inner.content)
            .ok()
            .map(|m| NostrProfile::from_metadata(author, m))
        {
            Some(profile) => {
                let inner_created_at = inner.created_at.as_secs();
                let mut s = state.lock().unwrap();
                let should_replace = match s.members.get(&author).and_then(|o| o.as_ref()) {
                    None => true,
                    Some(slot) => match slot.source {
                        ProfileSource::External => true,
                        ProfileSource::InChannel => {
                            // Strict-greater wins: out-of-order cache
                            // replay must not let a stale v1 overwrite v2.
                            slot.inchannel_created_at
                                .map_or(true, |t| inner_created_at > t)
                        }
                    },
                };
                if should_replace {
                    s.members.insert(
                        author,
                        Some(MemberSlot {
                            profile: profile.clone(),
                            source: ProfileSource::InChannel,
                            inchannel_created_at: Some(inner_created_at),
                        }),
                    );
                    Some(profile)
                } else {
                    None
                }
            }
            None => None,
        };
        if let Some(profile) = updated {
            let _ = event_tx
                .send(ChannelRunnerEvent::MemberProfileUpdated {
                    pubkey: author,
                    profile,
                })
                .await;
        }
        if let Some(ack) = ack {
            let _ = ack.send(());
        }
    } else if inner.kind == Kind::ChannelCreation {
        {
            let mut s = state.lock().unwrap();
            if s.creation_event.is_none() {
                s.creation_event = Some(inner.clone());
            }
        }
        let _ = event_tx
            .send(ChannelRunnerEvent::CreationEventReceived)
            .await;
        // `CreationEventReceived` has no `ack` field, so signal the
        // dispatch-ack (if any) right after queueing — the runner's
        // own state was updated synchronously above, and consumer-
        // side handling is a no-op.
        if let Some(ack) = ack {
            let _ = ack.send(());
        }
    } else if inner.kind == Kind::ChannelMessage {
        let reply_to = extract_e_tag(inner);
        let _ = event_tx
            .send(ChannelRunnerEvent::ChatMessage {
                message_id: inner.id.into(),
                author,
                content: inner.content.clone(),
                timestamp: inner.created_at.as_secs(),
                reply_to,
            })
            .await;
        // Chat messages route around the dispatch-ack: chat uses
        // its own optimistic-insert flow (see the `publish` path on
        // the handle). If this ever fires via `Dispatch` we still
        // don't block the caller — queueing on event_tx is enough.
        if let Some(ack) = ack {
            let _ = ack.send(());
        }
    } else {
        let _ = event_tx
            .send(ChannelRunnerEvent::AppEvent {
                inner_event: inner.clone(),
                ack,
            })
            .await;
    }
}

pub(crate) fn decrypt_inner_event(
    outer_event: &Event,
    conversation_key: &ConversationKey,
) -> Result<Event> {
    let encrypted_content = &outer_event.content;
    anyhow::ensure!(!encrypted_content.is_empty(), "empty content");

    let payload = BASE64.decode(encrypted_content)?;
    let decrypted_bytes = v2::decrypt_to_bytes(conversation_key, &payload)?;
    let decrypted = String::from_utf8(decrypted_bytes)?;
    let inner_event: Event = serde_json::from_str(&decrypted)?;

    anyhow::ensure!(
        inner_event.verify().is_ok(),
        "inner event signature invalid"
    );

    Ok(inner_event)
}

pub(crate) fn encrypt_inner_event(
    inner_event: &Event,
    channel_keys: &ChannelKeys,
) -> Result<String> {
    let inner_json = serde_json::to_string(inner_event)?;
    let conversation_key = ConversationKey::new(channel_keys.shared_secret);
    let encrypted_bytes = v2::encrypt_to_bytes(&conversation_key, inner_json.as_bytes())?;
    Ok(BASE64.encode(&encrypted_bytes))
}

pub(crate) async fn send_prepared_message(
    client: &Client,
    channel_keys: &ChannelKeys,
    inner_event: Event,
    expiration: Option<Timestamp>,
) -> Result<Output<nostr_sdk::EventId>> {
    let encrypted = encrypt_inner_event(&inner_event, channel_keys)?;
    let ephemeral_keys = Keys::generate();

    let mut builder = EventBuilder::new(Kind::Custom(4), encrypted).tag(Tag::custom(
        TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::H)),
        vec![channel_keys.channel_id_hex()],
    ));
    if let Some(ts) = expiration {
        builder = builder.tag(Tag::expiration(ts));
    }
    let outer_event = builder
        .build(ephemeral_keys.public_key())
        .sign_with_keys(&ephemeral_keys)?;

    // `send_event` (default `AckPolicy::all()`) awaits each selected
    // relay's OK/rejection with a 10s timeout. Returns per-relay
    // `success`/`failed` in the `Output`.
    let output = client.send_event(&outer_event).await?;
    Ok(output)
}

/// Helper: compute the expiration timestamp from an optional duration.
fn expiration_from(duration: Option<Duration>) -> Option<Timestamp> {
    duration.map(|d| Timestamp::from_secs(Timestamp::now().as_secs() + d.as_secs()))
}

pub(crate) fn extract_e_tag(event: &Event) -> Option<EventId> {
    event.tags.iter().find_map(|tag| {
        if tag.kind() == TagKind::e() {
            tag.content()
                .and_then(|s| nostr_sdk::EventId::from_hex(s).ok())
                .map(EventId::from)
        } else {
            None
        }
    })
}

pub(crate) fn extract_e_tags(event: &Event) -> Vec<EventId> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            if tag.kind() == TagKind::e() {
                tag.content()
                    .and_then(|s| nostr_sdk::EventId::from_hex(s).ok())
                    .map(EventId::from)
            } else {
                None
            }
        })
        .collect()
}

pub fn decode_bincode<T: bincode::Decode<()>>(inner_event: &Event) -> Result<T> {
    let content_bytes = BASE64.decode(&inner_event.content)?;
    let (val, _) = bincode::decode_from_slice(&content_bytes, BINCODE_CONFIG)?;
    Ok(val)
}

async fn fetch_init_event_with_key(
    client: &Client,
    channel_id_hex: &str,
    conversation_key: &ConversationKey,
) -> Result<Option<Event>> {
    let filter = Filter::new()
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::H),
            channel_id_hex.to_string(),
        )
        .kind(Kind::Custom(4));

    let events = client.fetch_events(filter, Duration::from_secs(10)).await?;

    for event in events.into_iter() {
        if let Ok(inner) = decrypt_inner_event(&event, conversation_key) {
            if inner.kind == Kind::ChannelCreation {
                return Ok(Some(inner));
            }
        }
    }

    Ok(None)
}

fn spawn_profile_fetch(
    pubkey: PublicKey,
    client: Client,
    tx: mpsc::Sender<(PublicKey, NostrProfile)>,
) {
    tokio::spawn(async move {
        if let Some(profile) = get_cached_profile(&client, pubkey).await {
            let _ = tx.send((pubkey, profile)).await;
            return;
        }
        if let Some(profile) = fetch_profile_from_relays(&client, pubkey).await {
            let _ = tx.send((pubkey, profile)).await;
        }
    });
}

async fn get_cached_profile(client: &Client, pubkey: PublicKey) -> Option<NostrProfile> {
    let filter = Filter::new()
        .author(nostr_sdk::PublicKey::from(pubkey))
        .kind(Kind::Metadata)
        .limit(1);
    match client.database().query(filter).await {
        Ok(events) => events.into_iter().next().and_then(|event| {
            serde_json::from_str::<nostr_sdk::Metadata>(&event.content)
                .ok()
                .map(|metadata| NostrProfile::from_metadata(pubkey, metadata))
        }),
        Err(_) => None,
    }
}

async fn fetch_profile_from_relays(client: &Client, pubkey: PublicKey) -> Option<NostrProfile> {
    match client
        .fetch_metadata(nostr_sdk::PublicKey::from(pubkey), PROFILE_FETCH_TIMEOUT)
        .await
    {
        Ok(Some(metadata)) => Some(NostrProfile::from_metadata(pubkey, metadata)),
        Ok(None) => None,
        Err(e) => {
            tracing::debug!(pubkey = %pubkey, error = %e, "failed to fetch profile");
            None
        }
    }
}
