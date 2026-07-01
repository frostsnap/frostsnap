//! Signing identity types shared across the frostsnap-nostr stack.
//!
//! `Nsec` is a validated bech32-encoded secret key (`nsec1…`) — the
//! FFI-safe form we hand across the Rust ↔ Dart boundary. Rust code
//! parses it to `nostr_sdk::Keys` on demand via [`Nsec::keys`].
//!
//! `NostrIdentity` bundles an `Nsec` with the policy for publishing a
//! kind-0 profile inside encrypted channels. The two variants encode
//! the two supported user modes:
//!
//! - `Imported`: user brought an nsec whose public kind 0 is already
//!   on relays. We NEVER publish an in-channel copy — peers fetch
//!   from the public network via the runner's profile-fetch path.
//! - `Generated`: app made the nsec locally; nothing else knows the
//!   name/pubkey mapping. We DO publish an encrypted kind 0 into
//!   every channel this identity joins so peers see the name via
//!   `MemberProfileUpdated`.

use crate::channel_runner::NostrProfile;
use crate::PublicKey;
use anyhow::Result;
use nostr_sdk::{nips::nip19::ToBech32, Keys};

/// Validated bech32 secret key. FFI-safe (wraps a `String`), so it
/// crosses the Rust ↔ Dart boundary as a value. Callers that need
/// signing keys go through [`Nsec::keys`].
///
/// The `Deserialize` impl validates via [`Nsec::parse`] — a corrupt
/// stored string can't deserialize into a semantically invalid
/// `Nsec`. Persistence layers (nostr_settings_state) rely on this
/// so `identity.public_key()` / `identity.keys()` on a loaded
/// `NostrIdentity` are effectively infallible in practice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Nsec(pub String);

impl<'de> serde::Deserialize<'de> for Nsec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        Nsec::parse(s).map_err(|e| D::Error::custom(format!("invalid nsec: {e}")))
    }
}

impl Nsec {
    /// Generate a fresh random identity.
    pub fn generate() -> Self {
        let keys = Keys::generate();
        Nsec(keys.secret_key().to_bech32().expect("valid key"))
    }

    /// Validate a user-supplied bech32 string.
    pub fn parse(s: String) -> Result<Self> {
        Keys::parse(&s)?;
        Ok(Nsec(s))
    }

    /// The stored string form (for persistence / display).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse into signing `Keys`. Fails if the stored string somehow
    /// became invalid after construction (shouldn't happen — but
    /// exposed as `Result` so callers can propagate cleanly).
    pub fn keys(&self) -> Result<Keys> {
        Ok(Keys::parse(&self.0)?)
    }

    /// Derive the x-only public key.
    pub fn public_key(&self) -> Result<PublicKey> {
        Ok(self.keys()?.public_key().into())
    }
}

/// A signing identity plus its in-channel profile distribution policy.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NostrIdentity {
    /// User's nsec has a public NIP-01 kind 0 out on relays. Peers
    /// fetch the profile from the public network via the runner's
    /// existing profile-fetch path. We do NOT publish an encrypted
    /// in-channel kind 0.
    Imported {
        nsec: Nsec,
        /// The public profile as we last observed it — used for
        /// local UI display of "me" without an extra fetch. Not
        /// authoritative for peers.
        cached_profile: NostrProfile,
    },
    /// User's nsec is app-generated and has no public kind 0. We
    /// publish an encrypted kind 0 into every channel we join.
    Generated {
        nsec: Nsec,
        name: String,
        created_at: u64,
    },
}

impl NostrIdentity {
    /// Parse the nsec into signing `Keys`.
    pub fn keys(&self) -> Result<Keys> {
        self.nsec().keys()
    }

    pub fn public_key(&self) -> Result<PublicKey> {
        self.nsec().public_key()
    }

    /// The profile to publish encrypted into channels this identity
    /// joins. `Some` for `Generated`; `None` for `Imported` (already
    /// on public relays — never publish in-channel).
    pub fn in_channel_profile(&self) -> Option<NostrProfile> {
        match self {
            NostrIdentity::Imported { .. } => None,
            NostrIdentity::Generated { name, nsec, .. } => {
                let pubkey = nsec.public_key().ok();
                Some(NostrProfile {
                    pubkey,
                    name: Some(name.clone()),
                    ..Default::default()
                })
            }
        }
    }

    fn nsec(&self) -> &Nsec {
        match self {
            NostrIdentity::Imported { nsec, .. } | NostrIdentity::Generated { nsec, .. } => nsec,
        }
    }
}
