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

use crate::channel::ParticipantShares;
use crate::{EventId, PublicKey};
use frostsnap_core::{
    coordinator::{remote_signing::SpSubset, ParticipantBinonces, ParticipantSignatureShares},
    silent_payments::EcdhShare,
    SignSessionId, WireSignTask,
};
use nostr_sdk::{Event, TagKind};

// ============================================================================
// Channel events (top-level sink stream)
// ============================================================================

/// Re-exported: member state is owned by the channel runner (block
/// pattern — see `ChannelInfraEvent::Members`); this alias keeps the
/// established `frostsnap_nostr::GroupMember` path stable.
pub use crate::channel_runner::GroupMember;

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
    ReceiveAddress {
        message_id: EventId,
        author: PublicKey,
        timestamp: u64,
        pending: bool,
        derivation_index: u32,
        memo: String,
    },
    ReceiveAddressSendFailed {
        message_id: EventId,
        reason: String,
    },
    /// A tx that touches this group's wallet was observed (in
    /// mempool or just confirmed). Carries identifiers + chat-side
    /// correlations only; Dart fetches the tx data via
    /// `SuperWallet::get_tx(txid)` at render time.
    TxObservation {
        txid: String,
        kind: ObservationKind,
        /// `last_seen` for `Mempool`, `confirmation_time.time` for
        /// `Confirmed`. Used by Dart for chat-timeline ordering.
        timestamp: u64,
        /// Set when this tx pays to an address that was previously
        /// announced via a `ReceiveAddress` chat message.
        /// Chronology-only — resolved once on first observation,
        /// no back-patching.
        address_reveal_event: Option<EventId>,
        /// Set when this tx is the signed result of a chat
        /// `SigningEvent::Request`. Same chronology rule.
        signing_start_event: Option<EventId>,
    },
    ConnectionState(ConnectionState),
    /// The channel's full member block (runner-owned; block pattern).
    /// Emitted unperturbed on every membership/profile change —
    /// consumers display it, they never reconstruct member state
    /// from deltas.
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
    /// The channel's fold state — currently just the initial participant
    /// mapping from the creation event. Emitted when the creation event
    /// is received (either fetched from relay or just published).
    ChannelState {
        participants: Vec<ChannelParticipant>,
    },
}

/// Dart-friendly representation of a participant's share ownership.
/// Converted from `ParticipantShares` (which uses opaque `ShareIndex`).
#[derive(Debug, Clone)]
pub struct ChannelParticipant {
    pub pubkey: PublicKey,
    pub share_indices: Vec<u32>,
}

impl From<&ParticipantShares> for ChannelParticipant {
    fn from(p: &ParticipantShares) -> Self {
        Self {
            pubkey: p.pubkey,
            share_indices: p
                .share_indices
                .iter()
                .map(|si| u32::try_from(*si).expect("share index fits u32"))
                .collect(),
        }
    }
}

impl ChannelEvent {
    pub fn from_inner_chat_message(inner_event: &Event, pending: bool) -> Self {
        Self::ChatMessage {
            message_id: inner_event.id.into(),
            author: inner_event.pubkey.into(),
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

/// Which moment-in-time a `TxObservation` event represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationKind {
    /// Wallet has seen the tx (typically in mempool) but it
    /// hasn't been confirmed in a block yet.
    Mempool,
    /// Wallet has seen a block containing the tx.
    Confirmed,
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
    /// The offer's silent payment ECDH shares. Empty unless the request is a silent payment send.
    pub sp_ecdh: Vec<EcdhShare>,
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
        /// The offer's silent payment ECDH shares. Empty unless the request is a silent payment
        /// send. Offers whose shares fail verification never become events, see `Rejected`.
        sp_ecdh: Vec<EcdhShare>,
        timestamp: u64,
    },
    /// Emitted when the settling timer expires with at least `threshold`
    /// offers observed. The subset is locked in; included participants
    /// should sign.
    ///
    /// For a silent payment send, `sign_task` is still the requested template and `sp` is the
    /// subset's shares with the task completed from them. `session_id` is the session of the
    /// completed task. Pass `sp` to `sign_with_nonce_reservation`.
    RoundConfirmed {
        request_id: EventId,
        subset: Vec<ConfirmedSubsetEntry>,
        session_id: SignSessionId,
        sign_task: WireSignTask,
        sp: Option<SpSubset>,
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
    /// An offer to sign a silent payment send: the binonces and, for each of the offering
    /// devices, its ECDH share and proof for every scan key of the request.
    SpOffer {
        binonces: Vec<ParticipantBinonces>,
        sp_ecdh: Vec<EcdhShare>,
    },
}

/// Wire format for receive-address share messages (kind 7800).
/// Carries only the structural information needed to derive the
/// address — receivers compute the actual address themselves from
/// their wallet descriptor. Keychain is always external and
/// account is the wallet's default; neither needs to be on the
/// wire today.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
pub struct ReceiveAddressPayload {
    pub derivation_index: u32,
    pub memo: String,
}

#[cfg(test)]
mod encoding {
    use super::SigningMessage;

    fn encoded(message: &SigningMessage) -> String {
        let bytes = bincode::encode_to_vec(message, bincode::config::standard()).unwrap();
        frostsnap_core::hex::encode(&bytes)
    }

    /// `Offer` stays variant 1. `SpOffer` is appended after `Cancel`, so existing offer bytes
    /// still decode. `standard()` encodes the variant index and lengths as varints.
    #[test]
    fn sp_offer_is_an_appended_variant() {
        assert_eq!(encoded(&SigningMessage::Offer { binonces: vec![] }), "0100");
        assert_eq!(encoded(&SigningMessage::Cancel), "03");
        assert_eq!(
            encoded(&SigningMessage::SpOffer {
                binonces: vec![],
                sp_ecdh: vec![],
            }),
            "040000"
        );
    }
}
