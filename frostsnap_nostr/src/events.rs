use crate::client::SigningChain;
use frostsnap_core::{
    coordinator::{ParticipantBinonces, ParticipantSignatureShares},
    AccessStructureRef, SignSessionId, WireSignTask,
};
use nostr_sdk::{EventId, Metadata, PublicKey};

/// A member of the channel group with their profile.
#[derive(Debug, Clone)]
pub struct GroupMember {
    pub pubkey: PublicKey,
    pub profile: Option<NostrProfile>,
}

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
    /// Connection state changed
    ConnectionState(ConnectionState),
    /// Group membership/profiles changed
    GroupMetadata { members: Vec<GroupMember> },
    /// Frostsnap protocol event (signing, future: keygen, etc.)
    Frostsnap(FrostsnapEvent),
    /// An event we received but couldn't process.
    Error {
        event_id: EventId,
        author: PublicKey,
        timestamp: u64,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub enum FrostsnapEvent {
    Signing(SigningEvent),
}

#[derive(Debug, Clone)]
pub enum SigningEvent {
    Request {
        event_id: EventId,
        author: PublicKey,
        sign_task: WireSignTask,
        access_structure_ref: AccessStructureRef,
        message: String,
        timestamp: u64,
    },
    Offer {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        binonces: ParticipantBinonces,
        /// If this offer completes the signing set (threshold met), the full chain data
        /// needed to promote to an active signing session.
        sealed: Option<SigningChain>,
        timestamp: u64,
    },
    Partial {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        session_id: SignSessionId,
        signature_shares: ParticipantSignatureShares,
        timestamp: u64,
    },
}

/// Wire format for all frostsnap signing messages in the channel.
#[derive(bincode::Encode, bincode::Decode)]
pub enum SigningMessage {
    Request {
        sign_task: WireSignTask,
        access_structure_ref: AccessStructureRef,
        message: String,
    },
    Offer {
        binonces: ParticipantBinonces,
    },
    Partial {
        session_id: SignSessionId,
        signature_shares: ParticipantSignatureShares,
    },
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}

/// Nostr profile metadata (NIP-01 kind 0 event content)
#[derive(Debug, Clone, Default)]
pub struct NostrProfile {
    pub pubkey: Option<PublicKey>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub banner: Option<String>,
    pub nip05: Option<String>,
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
