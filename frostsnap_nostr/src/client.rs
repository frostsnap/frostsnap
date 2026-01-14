use crate::{
    channel::ChannelKeys,
    events::{ChannelEvent, ConnectionState},
};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use frostsnap_coordinator::Sink;
use frostsnap_core::KeyId;
use nostr_sdk::{
    nips::nip44::v2::{self, ConversationKey},
    Alphabet, Client, Event, EventBuilder, EventId, Filter, Keys, Kind, PublicKey,
    RelayPoolNotification, SingleLetterTag, Tag, TagKind,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Client for connecting to and communicating on a Nostr channel.
pub struct ChannelClient {
    channel_keys: ChannelKeys,
    user_keys: Keys,
}

impl ChannelClient {
    /// Create a new channel client for the given key_id and user identity.
    pub fn new(key_id: &KeyId, user_keys: Keys) -> Self {
        let channel_keys = ChannelKeys::from_key_id(key_id);
        Self {
            channel_keys,
            user_keys,
        }
    }

    /// Connect to relays and start receiving events.
    /// Events are emitted through the sink.
    /// Returns a handle for sending messages.
    pub async fn run(
        self,
        relay_urls: Vec<String>,
        sink: impl Sink<ChannelEvent>,
    ) -> Result<ChannelHandle> {
        let client = Client::new(self.user_keys.clone());

        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connecting));

        for url in &relay_urls {
            if let Err(e) = client.add_relay(url).await {
                tracing::warn!(relay = %url, error = %e, "failed to add relay");
            }
        }

        client.connect().await;
        sink.send(ChannelEvent::ConnectionState(ConnectionState::Connected));

        let channel_id_hex = self.channel_keys.channel_id_hex();
        let filter = Filter::new().custom_tag(
            SingleLetterTag::lowercase(Alphabet::H),
            [channel_id_hex.clone()],
        );

        client.subscribe(vec![filter], None).await?;

        let conversation_key = ConversationKey::new(self.channel_keys.shared_secret);
        let our_pubkey = self.user_keys.public_key();

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ChannelCommand>(32);

        let client_for_task = client.clone();
        let channel_keys_for_task = self.channel_keys.clone();
        let user_keys_for_task = self.user_keys.clone();

        tokio::spawn(async move {
            let mut notifications = client_for_task.notifications();

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(ChannelCommand::SendPreparedMessage { inner_event, content, reply_to }) => {
                                let message_id = inner_event.id;
                                let timestamp = inner_event.created_at.as_u64();

                                // 📤 Emit immediately so UI shows the message
                                sink.send(ChannelEvent::ChatMessage {
                                    message_id,
                                    author: user_keys_for_task.public_key(),
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
                            Some(ChannelCommand::InitializeChannel { wallet_name }) => {
                                if let Err(e) = initialize_channel_if_needed(
                                    &client_for_task,
                                    &channel_keys_for_task,
                                    &user_keys_for_task,
                                    &wallet_name,
                                ).await {
                                    tracing::error!(error = %e, "failed to initialize channel");
                                }
                            }
                            None => break,
                        }
                    }
                    notification = notifications.recv() => {
                        match notification {
                            Ok(RelayPoolNotification::Event { event, .. }) => {
                                if let Some(channel_event) = process_event(
                                    &event,
                                    &conversation_key,
                                    our_pubkey,
                                ) {
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
            user_keys: self.user_keys,
            client: Arc::new(client),
        })
    }
}

enum ChannelCommand {
    SendPreparedMessage {
        inner_event: Event,
        content: String,
        reply_to: Option<EventId>,
    },
    InitializeChannel { wallet_name: String },
}

/// Handle for sending messages to an active channel.
#[derive(Clone)]
pub struct ChannelHandle {
    cmd_tx: mpsc::Sender<ChannelCommand>,
    user_keys: Keys,
    #[allow(dead_code)]
    client: Arc<Client>,
}

impl ChannelHandle {
    /// Send a chat message, optionally replying to another message.
    /// Returns the message ID immediately; relay send happens in background.
    pub async fn send_message(&self, content: String, reply_to: Option<EventId>) -> Result<EventId> {
        let inner_event = create_inner_event(&self.user_keys, &content, reply_to).await?;
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

    /// Initialize channel with wallet name.
    /// Idempotent - only creates the channel if it doesn't exist.
    pub async fn initialize_channel(&self, wallet_name: &str) -> Result<()> {
        self.cmd_tx
            .send(ChannelCommand::InitializeChannel {
                wallet_name: wallet_name.to_string(),
            })
            .await
            .map_err(|_| anyhow!("channel closed"))
    }
}

fn process_event(
    outer_event: &Event,
    conversation_key: &ConversationKey,
    _our_pubkey: PublicKey,
) -> Option<ChannelEvent> {
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

    match inner_event.kind {
        Kind::ChannelCreation => {
            let metadata: serde_json::Value = serde_json::from_str(&inner_event.content).ok()?;
            let name = metadata.get("name")?.as_str()?.to_string();
            let about = metadata
                .get("about")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(ChannelEvent::ChannelMetadata { name, about })
        }
        Kind::ChannelMessage => {
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
                timestamp: inner_event.created_at.as_u64(),
                reply_to,
                pending: false,
            })
        }
        _ => {
            tracing::debug!(kind = ?inner_event.kind, "unknown inner event kind");
            None
        }
    }
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
            [channel_keys.channel_id_hex()],
        ))
        .build(ephemeral_keys.public_key())
        .sign_with_keys(&ephemeral_keys)?;

    client.send_event(outer_event).await?;
    Ok(())
}

async fn initialize_channel_if_needed(
    client: &Client,
    channel_keys: &ChannelKeys,
    user_keys: &Keys,
    wallet_name: &str,
) -> Result<()> {
    let channel_id_hex = channel_keys.channel_id_hex();
    let filter = Filter::new()
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::H),
            [channel_id_hex.clone()],
        )
        .kind(Kind::Custom(4))
        .limit(100);

    let events = client
        .fetch_events(vec![filter], None)
        .await
        .context("failed to fetch events")?;

    let conversation_key = ConversationKey::new(channel_keys.shared_secret);

    for event in events.iter() {
        if let Ok(payload) = BASE64.decode(&event.content) {
            if let Ok(decrypted_bytes) = v2::decrypt_to_bytes(&conversation_key, &payload) {
                if let Ok(decrypted) = String::from_utf8(decrypted_bytes) {
                    if let Ok(inner) = serde_json::from_str::<Event>(&decrypted) {
                        if inner.kind == Kind::ChannelCreation {
                            tracing::debug!("channel already initialized");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    let metadata = serde_json::json!({
        "name": wallet_name,
        "about": format!("Frostsnap signing channel for {}", wallet_name),
    });

    let inner_event = EventBuilder::new(Kind::ChannelCreation, metadata.to_string())
        .build(user_keys.public_key())
        .sign(user_keys)
        .await?;

    let encrypted = encrypt_inner_event(&inner_event, channel_keys)?;

    let outer_event = EventBuilder::new(Kind::Custom(4), encrypted)
        .tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::H)),
            [channel_keys.channel_id_hex()],
        ))
        .build(user_keys.public_key())
        .sign(user_keys)
        .await?;

    client.send_event(outer_event).await?;
    tracing::info!(wallet_name = %wallet_name, "initialized channel");
    Ok(())
}

fn encrypt_inner_event(inner_event: &Event, channel_keys: &ChannelKeys) -> Result<String> {
    let inner_json = serde_json::to_string(inner_event)?;
    let conversation_key = ConversationKey::new(channel_keys.shared_secret);
    let encrypted_bytes = v2::encrypt_to_bytes(&conversation_key, inner_json)?;
    Ok(BASE64.encode(&encrypted_bytes))
}
