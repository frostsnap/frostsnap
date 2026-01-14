use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_core::KeyId;
use frostsnap_nostr::{ChannelClient, ChannelHandle, Keys, ToBech32};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub use frostsnap_nostr::{ChannelEvent, ConnectionState, EventId, PublicKey};

// ============================================================================
// Nsec - Our newtype wrapper for Nostr secret keys
// ============================================================================

/// A validated Nostr secret key (nsec).
#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub struct Nsec(pub String);

impl Nsec {
    /// Generate a new random Nostr identity.
    #[frb(sync)]
    pub fn generate() -> Self {
        let keys = Keys::generate();
        Nsec(keys.secret_key().to_bech32().expect("valid key"))
    }

    /// Parse and validate an nsec string.
    #[frb(sync)]
    pub fn parse(s: String) -> Result<Self> {
        Keys::parse(&s)?;
        Ok(Nsec(s))
    }

    /// Get the nsec as a string (for storage/display).
    #[frb(sync)]
    pub fn as_str(&self) -> String {
        self.0.clone()
    }

    /// Derive the public key from this secret key.
    #[frb(sync)]
    pub fn public_key(&self) -> PublicKey {
        Keys::parse(&self.0).expect("validated").public_key()
    }
}

// ============================================================================
// PublicKey - Opaque mirror of nostr_sdk::PublicKey
// ============================================================================

#[frb(mirror(PublicKey), opaque)]
pub struct _PublicKey {}

#[frb(external)]
impl PublicKey {
    #[frb(sync)]
    pub fn to_hex(&self) -> String {}

    #[frb(sync)]
    pub fn to_npub(&self) -> Result<String> {}

    #[frb(sync)]
    pub fn equals(&self, _other: &PublicKey) -> bool {}
}

pub trait PublicKeyExt {
    #[frb(sync)]
    fn to_hex(&self) -> String;

    #[frb(sync)]
    fn to_npub(&self) -> Result<String>;

    #[frb(sync)]
    fn equals(&self, other: &PublicKey) -> bool;
}

impl PublicKeyExt for PublicKey {
    #[frb(sync)]
    fn to_hex(&self) -> String {
        frostsnap_nostr::PublicKey::to_hex(self)
    }

    #[frb(sync)]
    fn to_npub(&self) -> Result<String> {
        Ok(self.to_bech32()?)
    }

    #[frb(sync)]
    fn equals(&self, other: &PublicKey) -> bool {
        self == other
    }
}

// ============================================================================
// NostrEventId - Non-opaque wrapper for EventId with proper Dart equality
// ============================================================================

/// A Nostr event ID (32 bytes). This is a non-opaque wrapper that provides
/// proper equality semantics in Dart for use as Map keys.
#[frb(non_opaque, non_hash, non_eq, dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is NostrEventId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
")]
#[derive(Debug, Clone)]
pub struct NostrEventId(pub [u8; 32]);

impl NostrEventId {
    #[frb(sync)]
    pub fn to_hex(&self) -> String {
        EventId::from(self.clone()).to_hex()
    }
}

impl From<EventId> for NostrEventId {
    fn from(id: EventId) -> Self {
        NostrEventId(*id.as_bytes())
    }
}

impl From<NostrEventId> for EventId {
    fn from(id: NostrEventId) -> Self {
        EventId::from_byte_array(id.0)
    }
}

lazy_static::lazy_static! {
    static ref CHANNEL_HANDLES: Mutex<HashMap<[u8; 32], ChannelHandle>> = Mutex::new(HashMap::new());
}

#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub enum FfiChannelEvent {
    ChatMessage {
        message_id: NostrEventId,
        author: PublicKey,
        content: String,
        timestamp: u64,
        reply_to: Option<NostrEventId>,
        pending: bool,
    },
    MessageSent {
        message_id: NostrEventId,
    },
    MessageSendFailed {
        message_id: NostrEventId,
        reason: String,
    },
    ChannelMetadata {
        name: String,
        about: String,
    },
    ConnectionState(FfiConnectionState),
}

#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub enum FfiConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}

impl From<ChannelEvent> for FfiChannelEvent {
    fn from(event: ChannelEvent) -> Self {
        match event {
            ChannelEvent::ChatMessage {
                message_id,
                author,
                content,
                timestamp,
                reply_to,
                pending,
            } => FfiChannelEvent::ChatMessage {
                message_id: message_id.into(),
                author,
                content,
                timestamp,
                reply_to: reply_to.map(|id| id.into()),
                pending,
            },
            ChannelEvent::MessageSent { message_id } => {
                FfiChannelEvent::MessageSent { message_id: message_id.into() }
            }
            ChannelEvent::MessageSendFailed { message_id, reason } => {
                FfiChannelEvent::MessageSendFailed { message_id: message_id.into(), reason }
            }
            ChannelEvent::ChannelMetadata { name, about } => {
                FfiChannelEvent::ChannelMetadata { name, about }
            }
            ChannelEvent::ConnectionState(state) => {
                FfiChannelEvent::ConnectionState(match state {
                    ConnectionState::Connecting => FfiConnectionState::Connecting,
                    ConnectionState::Connected => FfiConnectionState::Connected,
                    ConnectionState::Disconnected { reason } => {
                        FfiConnectionState::Disconnected { reason }
                    }
                })
            }
        }
    }
}

/// Connect to a Nostr channel and receive events.
///
/// # Arguments
/// * `key_id` - The wallet's key ID (determines the channel)
/// * `nsec` - The user's Nostr secret key
/// * `relay_urls` - List of relay URLs to connect to
/// * `sink` - Stream sink for receiving channel events
pub async fn connect_to_channel(
    key_id: KeyId,
    nsec: Nsec,
    relay_urls: Vec<String>,
    sink: StreamSink<FfiChannelEvent>,
) -> Result<()> {
    let user_keys = Keys::parse(&nsec.0)?;
    let client = ChannelClient::new(&key_id, user_keys);
    let handle = client.run(relay_urls, SinkWrap(sink)).await?;

    CHANNEL_HANDLES.lock().unwrap().insert(key_id.0, handle);
    Ok(())
}

/// Get a channel handle for sending messages.
/// Returns None if not connected to this channel.
pub fn get_channel_handle(key_id: KeyId) -> Option<NostrChannelHandle> {
    let handles = CHANNEL_HANDLES.lock().unwrap();
    handles.get(&key_id.0).cloned().map(|h| NostrChannelHandle {
        handle: Arc::new(Mutex::new(Some(h))),
        key_id,
    })
}

/// Disconnect from a channel.
pub fn disconnect_channel(key_id: KeyId) {
    CHANNEL_HANDLES.lock().unwrap().remove(&key_id.0);
}

/// Handle to an active Nostr channel connection.
pub struct NostrChannelHandle {
    handle: Arc<Mutex<Option<ChannelHandle>>>,
    #[allow(dead_code)]
    key_id: KeyId,
}

impl NostrChannelHandle {
    /// Send a chat message to the channel, optionally as a reply to another message.
    /// Returns the message ID immediately; relay send happens in background.
    pub async fn send_message(&self, content: String, reply_to: Option<NostrEventId>) -> Result<NostrEventId> {
        let handle = self
            .handle
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| anyhow!("channel not connected"))?
            .clone();
        let event_id = handle.send_message(content, reply_to.map(|id| id.into())).await?;
        Ok(event_id.into())
    }

    /// Initialize the channel with the wallet name.
    /// This is idempotent - it only creates the channel if it doesn't exist.
    pub async fn initialize_channel(&self, wallet_name: String) -> Result<()> {
        let handle = self
            .handle
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| anyhow!("channel not connected"))?
            .clone();
        handle.initialize_channel(&wallet_name).await
    }
}

/// Default port for the local test relay.
pub const TEST_RELAY_PORT: u16 = 7447;

/// Get default relay URLs.
#[frb(sync)]
pub fn default_relay_urls() -> Vec<String> {
    vec![format!("ws://localhost:{}", TEST_RELAY_PORT)]
}
