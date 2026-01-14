use nostr_sdk::{EventId, PublicKey};

/// Events emitted by ChannelClient through the Sink.
/// Dart receives these and builds the chat state.
#[derive(Debug, Clone)]
pub enum ChannelEvent {
    /// A chat message was received (or sent by us)
    ChatMessage {
        /// Unique ID for this message
        message_id: EventId,
        author: PublicKey,
        content: String,
        timestamp: u64,
        /// If this is a reply, the message_id of the parent
        reply_to: Option<EventId>,
        /// True if this is a local send (pending relay confirmation)
        pending: bool,
    },
    /// Our message was confirmed by relay
    MessageSent { message_id: EventId },
    /// Our message failed to send to relay
    MessageSendFailed { message_id: EventId, reason: String },
    /// Channel metadata received (NIP28 kind 40)
    ChannelMetadata { name: String, about: String },
    /// Connection state changed
    ConnectionState(ConnectionState),
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}
