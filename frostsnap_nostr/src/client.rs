use crate::{
    channel::ChannelKeys,
    events::{ChannelEvent, ConnectionState, GroupMember, NostrProfile},
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use frostsnap_coordinator::Sink;
use frostsnap_core::KeyId;
use nostr_sdk::{
    nips::nip44::v2::{self, ConversationKey},
    Alphabet, Client, Event, EventBuilder, EventId, Filter, Keys, Kind, PublicKey,
    RelayPoolNotification, SingleLetterTag, SyncOptions, Tag, TagKind,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::mpsc;

const PROFILE_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Client for connecting to and communicating on a Nostr channel.
pub struct ChannelClient {
    channel_keys: ChannelKeys,
}

impl ChannelClient {
    /// Create a new channel client for the given key_id.
    pub fn new(key_id: &KeyId) -> Self {
        let channel_keys = ChannelKeys::from_key_id(key_id);
        Self { channel_keys }
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

        // 📦 Query cached events immediately so UI shows them right away
        let stored_events = client.database().query(filter.clone()).await?;
        tracing::debug!(count = stored_events.len(), "loaded cached events");

        let mut members: HashMap<PublicKey, Option<NostrProfile>> = HashMap::new();
        let mut seen_ids: std::collections::HashSet<EventId> = std::collections::HashSet::new();

        for event in stored_events.into_iter() {
            seen_ids.insert(event.id);
            if let Some(channel_event) = process_event(&event, &conversation_key) {
                if let ChannelEvent::ChatMessage { author, .. } = &channel_event {
                    if !members.contains_key(author) {
                        members.insert(*author, None);
                    }
                }
                sink.send(channel_event);
            }
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
                    for event in events {
                        if seen_ids.contains(&event.id) {
                            continue;
                        }
                        if let Some(channel_event) = process_event(&event, &sync_conversation_key) {
                            sync_sink.send(channel_event);
                            new_count += 1;
                        }
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
                                if let Some(channel_event) = process_event(&event, &conversation_key) {
                                    if let ChannelEvent::ChatMessage { author, .. } = &channel_event {
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
}

/// Handle for sending messages to an active channel.
#[derive(Clone)]
pub struct ChannelHandle {
    cmd_tx: mpsc::Sender<ChannelCommand>,
    #[allow(dead_code)]
    client: Arc<Client>,
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

fn process_event(outer_event: &Event, conversation_key: &ConversationKey) -> Option<ChannelEvent> {
    let encrypted_content = &outer_event.content;
    if encrypted_content.is_empty() {
        return None;
    }

    let payload = match BASE64.decode(encrypted_content) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(error = %e, "failed to decode base64");
            return None;
        }
    };

    let decrypted_bytes = match v2::decrypt_to_bytes(conversation_key, &payload) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = %e, "failed to decrypt");
            return None;
        }
    };

    let decrypted = match String::from_utf8(decrypted_bytes) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "failed to convert to UTF-8");
            return None;
        }
    };

    let inner_event: Event = match serde_json::from_str(&decrypted) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(error = %e, "failed to parse inner event");
            return None;
        }
    };

    if inner_event.verify().is_err() {
        tracing::debug!("inner event signature invalid");
        return None;
    }

    if inner_event.kind != Kind::ChannelMessage {
        tracing::debug!(kind = ?inner_event.kind, "ignoring non-message event");
        return None;
    }

    let reply_to = inner_event.tags.iter().find_map(|tag| {
        if tag.kind() == TagKind::e() {
            tag.content().and_then(|s| EventId::from_hex(s).ok())
        } else {
            None
        }
    });

    Some(ChannelEvent::ChatMessage {
        message_id: inner_event.id,
        author: inner_event.pubkey,
        content: inner_event.content.clone(),
        timestamp: inner_event.created_at.as_secs(),
        reply_to,
        pending: false,
    })
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
