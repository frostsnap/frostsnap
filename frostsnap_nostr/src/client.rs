use crate::{
    channel::ChannelKeys,
    events::{
        ChannelEvent, ConnectionState, FrostsnapEvent, GroupMember, NostrProfile, SigningEvent,
        SigningMessage,
    },
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use frostsnap_coordinator::Sink;
use frostsnap_core::{
    coordinator::{ParticipantBinonces, ParticipantSignatureShares, KeyContext},
    AccessStructureRef, SignSessionId, WireSignTask,
};
use nostr_sdk::{
    nips::nip44::v2::{self, ConversationKey},
    Alphabet, Client, Event, EventBuilder, EventId, Filter, Keys, Kind, PublicKey,
    RelayPoolNotification, SingleLetterTag, SyncOptions, Tag, TagKind,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

const PROFILE_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

const KIND_FROSTSNAP_SIGNING: Kind = Kind::Custom(9001);

const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

// ============================================================================
// SigningEventTree — tracks decoded signing events for chain walking
// ============================================================================

#[derive(Default)]
pub struct SigningEventTree {
    events: HashMap<EventId, CachedSigningData>,
}

enum CachedSigningData {
    Request {
        sign_task: WireSignTask,
        access_structure_ref: AccessStructureRef,
    },
    Offer {
        parent_id: EventId,
        binonces: ParticipantBinonces,
    },
}

#[derive(Debug, Clone)]
pub struct SigningChain {
    pub request_id: EventId,
    pub sign_task: WireSignTask,
    pub access_structure_ref: AccessStructureRef,
    pub binonces: Vec<ParticipantBinonces>,
    pub signing_key: KeyContext,
}

impl SigningEventTree {
    fn add_request(
        &mut self,
        event_id: EventId,
        sign_task: WireSignTask,
        access_structure_ref: AccessStructureRef,
    ) {
        self.events.insert(
            event_id,
            CachedSigningData::Request {
                sign_task,
                access_structure_ref,
            },
        );
    }

    fn add_offer(
        &mut self,
        event_id: EventId,
        parent_id: EventId,
        binonces: ParticipantBinonces,
    ) {
        self.events.insert(
            event_id,
            CachedSigningData::Offer {
                parent_id,
                binonces,
            },
        );
    }

    /// Walk from `start_id` back through the chain of offers to the root request.
    fn walk_chain(
        &self,
        start_id: EventId,
        signing_key: &KeyContext,
    ) -> Result<SigningChain, String> {
        let mut collected = Vec::new();
        let mut current = start_id;

        loop {
            match self.events.get(&current) {
                Some(CachedSigningData::Request {
                    sign_task,
                    access_structure_ref,
                }) => {
                    collected.reverse();
                    return Ok(SigningChain {
                        request_id: current,
                        sign_task: sign_task.clone(),
                        access_structure_ref: *access_structure_ref,
                        binonces: collected,
                        signing_key: signing_key.clone(),
                    });
                }
                Some(CachedSigningData::Offer {
                    parent_id,
                    binonces,
                }) => {
                    collected.push(binonces.clone());
                    current = *parent_id;
                }
                None => {
                    return Err(format!("missing event {current} in signing chain"));
                }
            }
        }
    }
}

// ============================================================================
// ChannelClient
// ============================================================================

/// Client for connecting to and communicating on a Nostr channel.
pub struct ChannelClient {
    channel_keys: ChannelKeys,
    signing_key: KeyContext,
}

impl ChannelClient {
    pub fn new(signing_key: KeyContext) -> Self {
        let key_id = signing_key.master_appkey().key_id();
        let channel_keys = ChannelKeys::from_key_id(&key_id);
        Self { channel_keys, signing_key }
    }

    /// Start receiving channel events using the provided client.
    /// The client should already be connected to relays.
    /// Events are emitted through the sink.
    /// Returns a handle for sending messages.
    pub async fn run(
        self,
        client: Client,
        sink: impl Sink<ChannelEvent> + Clone,
    ) -> Result<ChannelHandle> {
        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connecting));

        let channel_id_hex = self.channel_keys.channel_id_hex();
        let filter = Filter::new()
            .custom_tag(
                SingleLetterTag::lowercase(Alphabet::H),
                channel_id_hex.clone(),
            )
            .kind(Kind::Custom(4));

        let conversation_key = ConversationKey::new(self.channel_keys.shared_secret);
        let signing_events = Arc::new(Mutex::new(SigningEventTree::default()));

        // 📦 Query cached events immediately so UI shows them right away
        let stored_events = client.database().query(filter.clone()).await?;
        tracing::debug!(count = stored_events.len(), "loaded cached events");

        let mut members: HashMap<PublicKey, Option<NostrProfile>> = HashMap::new();
        let mut seen_ids: std::collections::HashSet<EventId> = std::collections::HashSet::new();

        // ⏱️ Oldest first so Requests are processed before their Offers
        for event in stored_events.to_vec().into_iter().rev() {
            seen_ids.insert(event.id);
            let channel_event = process_event(
                &event,
                &conversation_key,
                &signing_events,
                &self.signing_key,
            );
            if let ChannelEvent::ChatMessage { ref author, .. } = channel_event {
                if !members.contains_key(author) {
                    members.insert(*author, None);
                }
            }
            sink.send(channel_event);
        }

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ChannelCommand>(32);
        let (profile_tx, mut profile_rx) = mpsc::channel::<(PublicKey, NostrProfile)>(32);

        if !members.is_empty() {
            emit_group_metadata(&sink, &members);
            for pubkey in members.keys() {
                spawn_profile_fetch(*pubkey, client.clone(), profile_tx.clone());
            }
        }

        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connected));

        // 📡 Subscribe for real-time updates
        client.subscribe(filter.clone(), None).await?;

        // 🔄 Sync with relays in background (negentropy set reconciliation)
        let sync_client = client.clone();
        let sync_filter = filter;
        let sync_sink = sink.clone();
        let sync_conversation_key = conversation_key.clone();
        let sync_cache = signing_events.clone();
        let sync_signing_key = self.signing_key.clone();
        tokio::spawn(async move {
            let sync_opts = SyncOptions::default();
            if let Err(e) = sync_client.sync(sync_filter.clone(), &sync_opts).await {
                tracing::warn!(error = %e, "background sync failed");
                return;
            }

            // ✨ Emit any new events discovered during sync
            match sync_client.database().query(sync_filter).await {
                Ok(events) => {
                    let mut new_count = 0;
                    for event in events.to_vec().into_iter().rev() {
                        if seen_ids.contains(&event.id) {
                            continue;
                        }
                        let channel_event = process_event(
                            &event,
                            &sync_conversation_key,
                            &sync_cache,
                            &sync_signing_key,
                        );
                        sync_sink.send(channel_event);
                        new_count += 1;
                    }
                    if new_count > 0 {
                        tracing::debug!(count = new_count, "emitted events from sync");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "failed to query after sync"),
            }
        });

        let client_for_task = client.clone();
        let channel_keys_for_task = self.channel_keys.clone();
        let task_signing_events = signing_events.clone();
        let task_signing_key = self.signing_key.clone();

        tokio::spawn(async move {
            let mut notifications = client_for_task.notifications();

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(ChannelCommand::SendPreparedMessage { inner_event, content, reply_to }) => {
                                let message_id = inner_event.id;
                                let author = inner_event.pubkey;
                                let timestamp = inner_event.created_at.as_secs();

                                sink.send(ChannelEvent::ChatMessage {
                                    message_id,
                                    author,
                                    content,
                                    timestamp,
                                    reply_to,
                                    pending: true,
                                });

                                match send_prepared_message(
                                    &client_for_task,
                                    &channel_keys_for_task,
                                    inner_event,
                                ).await {
                                    Ok(()) => {
                                        sink.send(ChannelEvent::MessageSent {
                                            message_id,
                                        });
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "failed to send message");
                                        sink.send(ChannelEvent::MessageSendFailed {
                                            message_id,
                                            reason: e.to_string(),
                                        });
                                    }
                                }
                            }
                            Some(ChannelCommand::SendSigningEvent { inner_event }) => {
                                let channel_event = process_signing_inner_event(
                                    &inner_event,
                                    &task_signing_events,
                                    &task_signing_key,
                                );
                                sink.send(channel_event);
                                if let Err(e) = send_prepared_message(
                                    &client_for_task,
                                    &channel_keys_for_task,
                                    inner_event,
                                ).await {
                                    tracing::error!(error = %e, "failed to send signing event");
                                }
                            }
                            None => break,
                        }
                    }
                    Some((pubkey, profile)) = profile_rx.recv() => {
                        members.insert(pubkey, Some(profile));
                        emit_group_metadata(&sink, &members);
                    }
                    notification = notifications.recv() => {
                        match notification {
                            Ok(RelayPoolNotification::Event { event, .. }) => {
                                let channel_event = process_event(
                                    &event,
                                    &conversation_key,
                                    &task_signing_events,
                                    &task_signing_key,
                                );
                                if let ChannelEvent::ChatMessage { ref author, .. } = channel_event {
                                    if !members.contains_key(author) {
                                        members.insert(*author, None);
                                        emit_group_metadata(&sink, &members);
                                        spawn_profile_fetch(
                                            *author,
                                            client_for_task.clone(),
                                            profile_tx.clone(),
                                        );
                                    }
                                }
                                sink.send(channel_event);
                            }
                            Ok(RelayPoolNotification::Shutdown) => {
                                sink.send(ChannelEvent::ConnectionState(
                                    ConnectionState::Disconnected { reason: Some("shutdown".to_string()) }
                                ));
                                break;
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(error = %e, "notification error");
                            }
                        }
                    }
                }
            }
        });

        Ok(ChannelHandle {
            cmd_tx,
            client: Arc::new(client),
            signing_events,
        })
    }
}

fn emit_group_metadata(
    sink: &impl Sink<ChannelEvent>,
    members: &HashMap<PublicKey, Option<NostrProfile>>,
) {
    sink.send(ChannelEvent::GroupMetadata {
        members: members
            .iter()
            .map(|(pubkey, profile)| GroupMember {
                pubkey: *pubkey,
                profile: profile.clone(),
            })
            .collect(),
    });
}

fn spawn_profile_fetch(
    pubkey: PublicKey,
    client: Client,
    tx: mpsc::Sender<(PublicKey, NostrProfile)>,
) {
    tokio::spawn(async move {
        // 📦 Check cache first for instant display
        if let Some(profile) = get_cached_profile(&client, pubkey).await {
            let _ = tx.send((pubkey, profile)).await;
            return;
        }
        // 🌐 Fall back to relay fetch
        if let Some(profile) = fetch_profile_from_relays(&client, pubkey).await {
            let _ = tx.send((pubkey, profile)).await;
        }
    });
}

async fn get_cached_profile(client: &Client, pubkey: PublicKey) -> Option<NostrProfile> {
    let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
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
    match client.fetch_metadata(pubkey, PROFILE_FETCH_TIMEOUT).await {
        Ok(Some(metadata)) => Some(NostrProfile::from_metadata(pubkey, metadata)),
        Ok(None) => None,
        Err(e) => {
            tracing::debug!(pubkey = %pubkey, error = %e, "failed to fetch profile from relays");
            None
        }
    }
}

enum ChannelCommand {
    SendPreparedMessage {
        inner_event: Event,
        content: String,
        reply_to: Option<EventId>,
    },
    SendSigningEvent {
        inner_event: Event,
    },
}

/// Handle for sending messages to an active channel.
#[derive(Clone)]
pub struct ChannelHandle {
    cmd_tx: mpsc::Sender<ChannelCommand>,
    #[allow(dead_code)]
    client: Arc<Client>,
    pub signing_events: Arc<Mutex<SigningEventTree>>,
}

impl ChannelHandle {
    /// Send a chat message, optionally replying to another message.
    /// Returns the message ID immediately; relay send happens in background.
    pub async fn send_message(
        &self,
        content: String,
        reply_to: Option<EventId>,
        keys: &Keys,
    ) -> Result<EventId> {
        let inner_event = create_inner_event(keys, &content, reply_to).await?;
        let message_id = inner_event.id;

        self.cmd_tx
            .send(ChannelCommand::SendPreparedMessage {
                inner_event,
                content,
                reply_to,
            })
            .await
            .map_err(|_| anyhow!("channel closed"))?;

        Ok(message_id)
    }

    /// Send a sign request to the channel. Returns the inner event ID
    /// (used as `StagingSessionId`).
    pub async fn send_sign_request(
        &self,
        keys: &Keys,
        sign_task: WireSignTask,
        access_structure_ref: AccessStructureRef,
        message: String,
    ) -> Result<EventId> {
        let signing_msg = SigningMessage::Request {
            sign_task,
            access_structure_ref,
            message,
        };
        self.send_signing_event(keys, &signing_msg, None).await
    }

    /// Send a sign offer (binonces) replying to the chain tip (previous offer or request).
    pub async fn send_sign_offer(
        &self,
        keys: &Keys,
        reply_to: EventId,
        binonces: ParticipantBinonces,
    ) -> Result<EventId> {
        let message = SigningMessage::Offer { binonces };
        self.send_signing_event(keys, &message, Some(reply_to))
            .await
    }

    /// Send signature shares for an active signing session.
    pub async fn send_sign_partial(
        &self,
        keys: &Keys,
        request_id: EventId,
        session_id: SignSessionId,
        signature_shares: ParticipantSignatureShares,
    ) -> Result<EventId> {
        let message = SigningMessage::Partial {
            session_id,
            signature_shares,
        };
        self.send_signing_event(keys, &message, Some(request_id))
            .await
    }

    async fn send_signing_event(
        &self,
        keys: &Keys,
        message: &SigningMessage,
        reply_to: Option<EventId>,
    ) -> Result<EventId> {
        let inner_event =
            create_bincode_inner_event(keys, KIND_FROSTSNAP_SIGNING, message, reply_to).await?;
        let event_id = inner_event.id;
        self.cmd_tx
            .send(ChannelCommand::SendSigningEvent { inner_event })
            .await
            .map_err(|_| anyhow!("channel closed"))?;
        Ok(event_id)
    }

    /// Fetch profile metadata for a public key.
    pub async fn fetch_profile(&self, pubkey: PublicKey) -> Result<NostrProfile> {
        let metadata = self
            .client
            .fetch_metadata(pubkey, PROFILE_FETCH_TIMEOUT)
            .await
            .context("failed to fetch profile")?
            .ok_or_else(|| anyhow!("no profile found"))?;
        Ok(NostrProfile::from_metadata(pubkey, metadata))
    }
}

// ============================================================================
// Event processing
// ============================================================================

fn decrypt_inner_event(
    outer_event: &Event,
    conversation_key: &ConversationKey,
) -> Result<Event> {
    let encrypted_content = &outer_event.content;
    anyhow::ensure!(!encrypted_content.is_empty(), "empty content");

    let payload = BASE64.decode(encrypted_content)?;
    let decrypted_bytes = v2::decrypt_to_bytes(conversation_key, &payload)?;
    let decrypted = String::from_utf8(decrypted_bytes)?;
    let inner_event: Event = serde_json::from_str(&decrypted)?;

    anyhow::ensure!(inner_event.verify().is_ok(), "inner event signature invalid");

    Ok(inner_event)
}

fn extract_e_tag(event: &Event) -> Option<EventId> {
    event.tags.iter().find_map(|tag| {
        if tag.kind() == TagKind::e() {
            tag.content().and_then(|s| EventId::from_hex(s).ok())
        } else {
            None
        }
    })
}

fn decode_bincode<T: bincode::Decode<()>>(inner_event: &Event) -> Result<T> {
    let content_bytes = BASE64.decode(&inner_event.content)?;
    let (val, _) = bincode::decode_from_slice(&content_bytes, BINCODE_CONFIG)?;
    Ok(val)
}

/// Process a decrypted inner event that contains a signing message.
fn process_signing_inner_event(
    inner_event: &Event,
    signing_events: &Mutex<SigningEventTree>,
    signing_key: &KeyContext,
) -> ChannelEvent {
    let event_id = inner_event.id;
    let author = inner_event.pubkey;
    let timestamp = inner_event.created_at.as_secs();

    let message: SigningMessage = match decode_bincode(inner_event) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(event_id = %event_id, error = %e, "failed to decode signing message");
            return ChannelEvent::Error {
                event_id,
                author,
                timestamp,
                reason: format!("failed to decode signing message: {e}"),
            };
        }
    };

    match message {
        SigningMessage::Request {
            sign_task,
            access_structure_ref,
            message,
        } => {
            signing_events.lock().unwrap().add_request(
                event_id,
                sign_task.clone(),
                access_structure_ref,
            );

            ChannelEvent::Frostsnap(FrostsnapEvent::Signing(SigningEvent::Request {
                event_id,
                author,
                sign_task,
                access_structure_ref,
                message,
                timestamp,
            }))
        }
        SigningMessage::Offer { binonces } => {
            let parent_id = match extract_e_tag(inner_event) {
                Some(id) => id,
                None => {
                    return ChannelEvent::Error {
                        event_id,
                        author,
                        timestamp,
                        reason: "signing offer missing e-tag".into(),
                    };
                }
            };

            let chain = {
                let mut cache = signing_events.lock().unwrap();
                let chain = match cache.walk_chain(parent_id, signing_key) {
                    Ok(c) => c,
                    Err(reason) => {
                        return ChannelEvent::Error {
                            event_id,
                            author,
                            timestamp,
                            reason,
                        };
                    }
                };
                cache.add_offer(event_id, parent_id, binonces.clone());
                chain
            };

            let request_id = chain.request_id;

            let mut all_binonces = chain.binonces;
            all_binonces.push(binonces.clone());

            let mut seen_indices = std::collections::BTreeSet::new();
            all_binonces.retain(|b| seen_indices.insert(b.share_index));

            let sealed = if seen_indices.len() >= signing_key.threshold() {
                Some(SigningChain {
                    request_id,
                    sign_task: chain.sign_task,
                    access_structure_ref: chain.access_structure_ref,
                    binonces: all_binonces,
                    signing_key: signing_key.clone(),
                })
            } else {
                None
            };

            ChannelEvent::Frostsnap(FrostsnapEvent::Signing(SigningEvent::Offer {
                event_id,
                author,
                request_id,
                binonces,
                sealed,
                timestamp,
            }))
        }
        SigningMessage::Partial {
            session_id,
            signature_shares,
        } => {
            let request_id = match extract_e_tag(inner_event) {
                Some(id) => id,
                None => {
                    return ChannelEvent::Error {
                        event_id,
                        author,
                        timestamp,
                        reason: "signing partial missing e-tag".into(),
                    };
                }
            };

            ChannelEvent::Frostsnap(FrostsnapEvent::Signing(SigningEvent::Partial {
                event_id,
                author,
                request_id,
                session_id,
                signature_shares,
                timestamp,
            }))
        }
    }
}

/// Process an outer (encrypted) event from Nostr.
fn process_event(
    outer_event: &Event,
    conversation_key: &ConversationKey,
    signing_events: &Mutex<SigningEventTree>,
    signing_key: &KeyContext,
) -> ChannelEvent {
    let inner_event = match decrypt_inner_event(outer_event, conversation_key) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(event_id = %outer_event.id, error = %e, "failed to decrypt event");
            return ChannelEvent::Error {
                event_id: outer_event.id,
                author: outer_event.pubkey,
                timestamp: outer_event.created_at.as_secs(),
                reason: format!("failed to decrypt: {e}"),
            };
        }
    };
    let kind = inner_event.kind;
    let event_id = inner_event.id;
    let author = inner_event.pubkey;
    let timestamp = inner_event.created_at.as_secs();
    tracing::info!(event_id = %event_id, kind = ?kind, "decoded event");

    if kind == Kind::ChannelMessage {
        let reply_to = extract_e_tag(&inner_event);
        ChannelEvent::ChatMessage {
            message_id: event_id,
            author,
            content: inner_event.content.clone(),
            timestamp,
            reply_to,
            pending: false,
        }
    } else if kind == KIND_FROSTSNAP_SIGNING {
        process_signing_inner_event(&inner_event, signing_events, signing_key)
    } else {
        tracing::warn!(event_id = %event_id, kind = ?kind, "unknown inner event kind");
        ChannelEvent::Error {
            event_id,
            author,
            timestamp,
            reason: format!("unknown event kind: {kind:?}"),
        }
    }
}

// ============================================================================
// Event construction helpers
// ============================================================================

async fn create_bincode_inner_event(
    user_keys: &Keys,
    kind: Kind,
    payload: &impl bincode::Encode,
    reply_to: Option<EventId>,
) -> Result<Event> {
    let encoded = bincode::encode_to_vec(payload, BINCODE_CONFIG)?;
    let content = BASE64.encode(&encoded);
    let mut builder = EventBuilder::new(kind, content);
    if let Some(parent_id) = reply_to {
        builder = builder.tag(Tag::event(parent_id));
    }
    let inner_event = builder
        .build(user_keys.public_key())
        .sign(user_keys)
        .await?;
    Ok(inner_event)
}

async fn create_inner_event(
    user_keys: &Keys,
    content: &str,
    reply_to: Option<EventId>,
) -> Result<Event> {
    let mut builder = EventBuilder::new(Kind::ChannelMessage, content);

    if let Some(parent_id) = reply_to {
        builder = builder.tag(Tag::event(parent_id));
    }

    let inner_event = builder
        .build(user_keys.public_key())
        .sign(user_keys)
        .await?;

    Ok(inner_event)
}

async fn send_prepared_message(
    client: &Client,
    channel_keys: &ChannelKeys,
    inner_event: Event,
) -> Result<()> {
    let encrypted = encrypt_inner_event(&inner_event, channel_keys)?;
    let ephemeral_keys = Keys::generate();

    let outer_event = EventBuilder::new(Kind::Custom(4), encrypted)
        .tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::H)),
            vec![channel_keys.channel_id_hex()],
        ))
        .build(ephemeral_keys.public_key())
        .sign_with_keys(&ephemeral_keys)?;

    client.send_event(&outer_event).await?;
    Ok(())
}

fn encrypt_inner_event(inner_event: &Event, channel_keys: &ChannelKeys) -> Result<String> {
    let inner_json = serde_json::to_string(inner_event)?;
    let conversation_key = ConversationKey::new(channel_keys.shared_secret);
    let encrypted_bytes = v2::encrypt_to_bytes(&conversation_key, inner_json.as_bytes())?;
    Ok(BASE64.encode(&encrypted_bytes))
}
