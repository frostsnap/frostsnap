//! Events and wire format for the signing channel.
//!
//! - [`ChannelEvent`] is the top-level stream emitted by
//!   [`crate::signing::ChannelClient`] through its `Sink<ChannelEvent>`.
//!   It carries chat messages, connection state, group metadata, and
//!   signing-protocol events.
//! - [`SigningMessage`] is the bincode wire payload of kind-9001 inner
//!   events on the nostr channel.
//! - [`SigningEvent`] is the decoded signing-protocol event, carried inside
//!   `ChannelEvent::Signing { event, pending }`.

use crate::channel_runner::NostrProfile;
use crate::EventId;
use frostsnap_core::{
    coordinator::{ParticipantBinonces, ParticipantSignatureShares},
    SignSessionId, WireSignTask,
};
use nostr_sdk::{Event, PublicKey, TagKind};

// ============================================================================
// Channel events (top-level sink stream)
// ============================================================================

/// A member of the channel group with their profile.
#[derive(Debug, Clone)]
pub struct GroupMember {
    pub pubkey: PublicKey,
    pub profile: Option<NostrProfile>,
}

/// Events emitted by ChannelClient through the Sink.
/// Dart receives these and builds the chat + signing state.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ChannelEvent {
    ChatMessage {
        message_id: EventId,
        author: PublicKey,
        content: String,
        timestamp: u64,
        reply_to: Option<EventId>,
        pending: bool,
    },
    MessageSent {
        message_id: EventId,
    },
    MessageSendFailed {
        message_id: EventId,
        reason: String,
    },
    ConnectionState(ConnectionState),
    GroupMetadata {
        members: Vec<GroupMember>,
    },
    Signing {
        event: SigningEvent,
        pending: bool,
    },
    Error {
        event_id: EventId,
        author: PublicKey,
        timestamp: u64,
        reason: String,
    },
}

impl ChannelEvent {
    pub fn from_inner_chat_message(inner_event: &Event, pending: bool) -> Self {
        Self::ChatMessage {
            message_id: inner_event.id.into(),
            author: inner_event.pubkey,
            content: inner_event.content.clone(),
            timestamp: inner_event.created_at.as_secs(),
            reply_to: inner_event.tags.iter().find_map(|tag| {
                if tag.kind() == TagKind::e() {
                    tag.content()
                        .and_then(|s| nostr_sdk::EventId::from_hex(s).ok())
                        .map(EventId::from)
                } else {
                    None
                }
            }),
            pending,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}

// ============================================================================
// Signing-protocol events
// ============================================================================

/// One entry in a [`SigningEvent::RoundConfirmed`]'s selected subset.
#[derive(Debug, Clone)]
pub struct ConfirmedSubsetEntry {
    pub event_id: EventId,
    pub author: PublicKey,
    pub timestamp: u64,
    pub binonces: Vec<ParticipantBinonces>,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SigningEvent {
    Request {
        event_id: EventId,
        author: PublicKey,
        sign_task: WireSignTask,
        message: String,
        timestamp: u64,
    },
    Offer {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        binonces: Vec<ParticipantBinonces>,
        timestamp: u64,
    },
    /// Emitted when the settling timer expires with at least `threshold`
    /// offers observed. The subset is locked in; included participants
    /// should sign.
    RoundConfirmed {
        request_id: EventId,
        subset: Vec<ConfirmedSubsetEntry>,
        session_id: SignSessionId,
        sign_task: WireSignTask,
        timestamp: u64,
    },
    /// Emitted when the settling timer expires with fewer than `threshold`
    /// offers. The round is still collecting; this is a provisional
    /// snapshot. Offers already in `observed` will almost certainly remain
    /// in the final confirmed subset (selector is oldest-first), so the
    /// UI can surface "your offer is likely accepted" to their authors.
    /// Re-emitted on every subsequent quiet period as new offers arrive.
    RoundPending {
        request_id: EventId,
        observed: Vec<EventId>,
        threshold: usize,
        timestamp: u64,
    },
    Partial {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        offer_subset: Vec<EventId>,
        session_id: SignSessionId,
        signature_shares: ParticipantSignatureShares,
        timestamp: u64,
    },
    Cancel {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        timestamp: u64,
    },
    Rejected {
        event_id: EventId,
        author: PublicKey,
        timestamp: u64,
        reason: String,
    },
}

// ============================================================================
// Wire format
// ============================================================================

/// Bincode-friendly wrapper around `EventId`. Nostr-sdk's serde `Serialize`
/// unconditionally emits hex strings, so `#[bincode(with_serde)]` on a raw
/// `EventId` would encode 72 bytes instead of 32. This newtype uses the
/// frostsnap_core macros to encode/decode as raw bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct WireEventId(pub [u8; 32]);

frostsnap_core::impl_display_debug_serialize!(
    fn to_bytes(weid: &WireEventId) -> [u8; 32] {
        weid.0
    }
);

frostsnap_core::impl_fromstr_deserialize!(
    name => "wire event id",
    fn from_bytes(bytes: [u8; 32]) -> WireEventId { WireEventId(bytes) }
);

impl From<EventId> for WireEventId {
    fn from(id: EventId) -> Self {
        WireEventId(id.0)
    }
}

impl From<WireEventId> for EventId {
    fn from(w: WireEventId) -> Self {
        EventId(w.0)
    }
}

/// Wire format for all frostsnap signing messages in the channel.
#[derive(bincode::Encode, bincode::Decode)]
pub(crate) enum SigningMessage {
    Request {
        sign_task: WireSignTask,
        message: String,
    },
    Offer {
        binonces: Vec<ParticipantBinonces>,
    },
    Partial {
        offer_subset: Vec<WireEventId>,
        signature_shares: ParticipantSignatureShares,
    },
    Cancel,
}
