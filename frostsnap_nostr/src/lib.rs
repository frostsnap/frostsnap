pub mod channel;
pub mod channel_runner;
pub mod keygen;
pub mod settings;
pub mod signing;

pub use channel::{ChannelInitData, ChannelKeys, ChannelSecret};
pub use channel_runner::{ChannelRunner, NostrProfile};
pub use nostr_lmdb::NostrLMDB;
pub use nostr_sdk::nips::nip19::ToBech32;
pub use nostr_sdk::prelude::NostrDatabaseExt;
pub use nostr_sdk::{Client, Keys, Metadata, PublicKey};
pub use settings::NostrSettings;
pub use signing::{
    ChannelClient, ChannelEvent, ChannelHandle, ConfirmedSubsetEntry, ConnectionState, GroupMember,
    SigningEvent,
};

/// Owned event-id newtype — 32 bytes, value-typed, `Copy`. Used in
/// every public type of this crate so consumers (notably the Flutter
/// FFI) get a translatable type instead of `nostr_sdk::EventId`,
/// which is opaque to FRB.
///
/// Conversion to/from the `nostr-sdk` type happens only at the
/// boundary where we hand off to `nostr_sdk::Client` / `Event` /
/// `EventBuilder`. Inside this crate, always use `EventId` directly.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventId(pub [u8; 32]);

impl EventId {
    pub const ZERO: Self = Self([0u8; 32]);

    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for b in self.0 {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }
}

impl From<nostr_sdk::EventId> for EventId {
    fn from(id: nostr_sdk::EventId) -> Self {
        Self(id.to_bytes())
    }
}

impl From<EventId> for nostr_sdk::EventId {
    fn from(id: EventId) -> Self {
        nostr_sdk::EventId::from_byte_array(id.0)
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl std::fmt::Debug for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EventId({})", self.to_hex())
    }
}
