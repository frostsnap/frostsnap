pub mod keygen_run;
pub mod remote_keygen;

use crate::frb_generated::{RustAutoOpaque, StreamSink};
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    message::EncodedSignature,
    AccessStructureId, AccessStructureRef, SignSessionId, SymmetricKey, WireSignTask,
};
pub use frostsnap_nostr::NostrProfile;
use frostsnap_nostr::{
    channel::{parse_frostsnap_link, parse_keygen_link},
    keygen::{LobbyChannelMetadata, LobbyClient},
    ChannelClient, ChannelHandle, ChannelInitData, ChannelKeys, ChannelRunner, Client, Keys,
    NostrDatabaseExt, NostrLMDB, ToBech32,
};
use rusqlite::Connection;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::Duration,
};

pub use frostsnap_nostr::{
    ChannelEvent, ChannelSecret, ConfirmedSubsetEntry, ConnectionState, EventId, GroupMember,
    PublicKey, SigningEvent,
};

#[frb(opaque)]
pub struct ChannelConnectionParams {
    pub(crate) key_context: KeyContext,
    pub(crate) init_data: Option<ChannelInitData>,
}

static NOSTR_LMDB: OnceLock<Arc<NostrLMDB>> = OnceLock::new();

fn get_or_init_nostr_lmdb(data_dir: &Path) -> Arc<NostrLMDB> {
    NOSTR_LMDB
        .get_or_init(|| {
            let lmdb_path = data_dir.join("nostr-lmdb");
            let db = NostrLMDB::open(&lmdb_path).expect("failed to open nostr lmdb");
            Arc::new(db)
        })
        .clone()
}

fn get_nostr_lmdb() -> Result<Arc<NostrLMDB>> {
    NOSTR_LMDB
        .get()
        .cloned()
        .ok_or_else(|| anyhow!("nostr lmdb not initialized"))
}

// ============================================================================
// NostrSettings - Identity settings with reactive updates
// ============================================================================

/// Nostr identity settings - follows same pattern as Settings struct.
/// Created during app load, passed to Dart via context.
#[frb(opaque)]
pub struct NostrSettings {
    db: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    data_dir: PathBuf,
    inner: RwLock<NostrSettingsInner>,
}

struct NostrSettingsInner {
    pubkey: Option<PublicKey>,
    /// Sink for identity updates. Stream value is the local pubkey;
    /// Dart computes npub on demand via `PublicKeyExt::toNpub()`.
    identity_sink: Option<StreamSink<Option<PublicKey>>>,
}

impl NostrSettings {
    /// Create by loading existing identity from SQLite.
    /// Called during app initialization.
    pub(crate) fn new(db: Arc<Mutex<Connection>>, data_dir: PathBuf) -> Result<Self> {
        // Ensure table exists
        {
            let db_guard = db.lock().unwrap();
            db_guard.execute(
                "CREATE TABLE IF NOT EXISTS nostr_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )",
                [],
            )?;
        }

        // Initialize LMDB
        get_or_init_nostr_lmdb(&data_dir);

        // Load existing identity
        let pubkey = Self::load_pubkey_from_db(&db);

        Ok(Self {
            db,
            data_dir,
            inner: RwLock::new(NostrSettingsInner {
                pubkey,
                identity_sink: None,
            }),
        })
    }

    fn load_pubkey_from_db(db: &Arc<Mutex<Connection>>) -> Option<PublicKey> {
        let db_guard = db.lock().unwrap();
        let nsec: Option<String> = db_guard
            .query_row(
                "SELECT value FROM nostr_settings WHERE key = 'nsec'",
                [],
                |row| row.get(0),
            )
            .ok();
        nsec.and_then(|n| Keys::parse(&n).ok().map(|k| k.public_key().into()))
    }

    /// Subscribe to identity changes. Emits current value immediately.
    /// Stream values are the local nostr pubkey; Dart computes the
    /// bech32 `npub` on demand via `PublicKey.toNpub()`.
    pub fn sub_identity(&self, sink: StreamSink<Option<PublicKey>>) -> Result<()> {
        {
            let mut inner = self.inner.write().unwrap();
            inner.identity_sink.replace(sink);
        }
        self.emit_identity();
        Ok(())
    }

    fn emit_identity(&self) {
        let inner = self.inner.read().unwrap();
        if let Some(sink) = &inner.identity_sink {
            let _ = sink.add(inner.pubkey);
        }
    }

    /// Get current identity synchronously.
    #[frb(sync)]
    pub fn current(&self) -> Option<PublicKey> {
        self.inner.read().unwrap().pubkey
    }

    /// Set/import nsec. Persists to DB and notifies subscribers.
    pub fn set_nsec(&self, nsec: String) -> Result<()> {
        let keys = Keys::parse(&nsec)?;

        // Persist
        {
            let db = self.db.lock().unwrap();
            db.execute(
                "INSERT OR REPLACE INTO nostr_settings (key, value) VALUES ('nsec', ?1)",
                [&nsec],
            )?;
        }

        // Update in-memory + emit
        {
            let mut inner = self.inner.write().unwrap();
            inner.pubkey = Some(keys.public_key().into());
        }
        self.emit_identity();
        tracing::info!("Nostr identity configured");
        Ok(())
    }

    /// Generate new random identity.
    pub fn generate(&self) -> Result<String> {
        let keys = Keys::generate();
        let nsec = keys.secret_key().to_bech32()?;
        self.set_nsec(nsec.clone())?;
        Ok(nsec)
    }

    /// Get nsec for export/backup.
    #[frb(sync)]
    pub fn get_nsec(&self) -> Result<String> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT value FROM nostr_settings WHERE key = 'nsec'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| anyhow!("no Nostr identity configured"))
    }

    /// Check if identity exists.
    #[frb(sync)]
    pub fn has_identity(&self) -> bool {
        self.inner.read().unwrap().pubkey.is_some()
    }
}

// ============================================================================
// NostrClient - Unified client for profiles and channels
// ============================================================================

/// Unified Nostr client for profiles and channels.
/// Create once and keep around - cloning shares the connection pool.
pub struct NostrClient {
    client: Client,
    channels: Mutex<HashMap<AccessStructureId, ChannelHandle>>,
    /// Per-channel `KeyContext`, kept after `connect_to_channel`
    /// consumes the params so we can build `SealedSigningData` on
    /// demand when a `RoundConfirmed` event lands.
    key_contexts: Mutex<HashMap<AccessStructureId, KeyContext>>,
}

impl NostrClient {
    /// Connect to default relays and return a client ready for use.
    pub async fn connect() -> Result<Self> {
        let database = get_nostr_lmdb()?;
        let client = Client::builder().database(database).build();

        for url in default_relay_urls() {
            if let Err(e) = client.add_relay(&url).await {
                tracing::warn!(relay = %url, error = %e, "failed to add relay");
            }
        }

        client.connect().await;

        Ok(Self {
            client,
            channels: Mutex::new(HashMap::new()),
            key_contexts: Mutex::new(HashMap::new()),
        })
    }

    /// Fetch profile metadata for a public key.
    /// Checks the local cache first, then fetches from relays if not found.
    /// Returns None if the user has no profile.
    pub async fn fetch_profile(&self, pubkey: &PublicKey) -> Result<Option<NostrProfile>> {
        let nostr_pk = (*pubkey).into();
        // 📦 Check cache first
        if let Ok(Some(metadata)) = self.client.database().metadata(nostr_pk).await {
            return Ok(Some(NostrProfile::from_metadata(*pubkey, metadata)));
        }

        // 🌐 Fetch from relays
        match self
            .client
            .fetch_metadata(nostr_pk, Duration::from_secs(5))
            .await
        {
            Ok(Some(metadata)) => Ok(Some(NostrProfile::from_metadata(*pubkey, metadata))),
            Ok(None) => Ok(None),
            Err(e) => {
                tracing::debug!(pubkey = %pubkey, error = %e, "failed to fetch profile");
                Ok(None)
            }
        }
    }

    /// Connect to a channel for chat/signing coordination.
    /// Events are streamed to the sink. Use send_message to interact.
    pub async fn connect_to_channel(
        &self,
        params: ChannelConnectionParams,
        sink: StreamSink<ChannelEvent>,
    ) {
        let access_structure_id = params.key_context.access_structure_id();
        let key_context = params.key_context.clone();
        let channel_sink = ChannelEventSink { sink };
        let channel_client = ChannelClient::new(params.key_context, params.init_data);
        let handle = match channel_client.run(self.client.clone(), channel_sink).await {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "failed to connect to channel");
                return;
            }
        };

        self.channels
            .lock()
            .unwrap()
            .insert(access_structure_id, handle);
        self.key_contexts
            .lock()
            .unwrap()
            .insert(access_structure_id, key_context);
    }

    /// Build a `SealedSigningData` bundle on demand when Dart receives
    /// a `SigningEvent::RoundConfirmed`. The channel's `key_context`
    /// (cached at `connect_to_channel` time) supplies the access
    /// structure scope; the rest of the fields come from the event.
    /// `binonces` is typically the `subset.iter().flat_map(|e|
    /// e.binonces.iter().cloned())` from the source event.
    #[frb(sync)]
    pub fn seal_round_confirmed(
        &self,
        access_structure_id: AccessStructureId,
        request_id: EventId,
        sign_task: WireSignTask,
        binonces: Vec<ParticipantBinonces>,
    ) -> Result<SealedSigningData> {
        let key_context = self
            .key_contexts
            .lock()
            .unwrap()
            .get(&access_structure_id)
            .cloned()
            .ok_or_else(|| anyhow!("no channel for access structure {access_structure_id}"))?;
        Ok(SealedSigningData {
            request_id: request_id.into(),
            sign_task,
            binonces,
            key_context,
        })
    }

    /// Join a wallet from a nostr invite link. Fetches channel data from relays,
    /// adds the key to the coordinator, and returns the new wallet's KeyId.
    pub async fn join_from_link(
        &self,
        coord: &super::coordinator::Coordinator,
        channel_secret: ChannelSecret,
        encryption_key: SymmetricKey,
    ) -> Result<frostsnap_core::KeyId> {
        let channel_keys = ChannelKeys::from_channel_secret(&channel_secret);
        let runner = ChannelRunner::new(channel_keys.clone());
        let init_event = runner
            .fetch_init_event(&self.client)
            .await?
            .ok_or_else(|| anyhow!("no channel found for this link"))?;
        let init_data =
            frostsnap_nostr::channel_runner::decode_bincode::<ChannelInitData>(&init_event)?;

        let derived_channel_keys =
            ChannelKeys::from_access_structure_id(&init_data.access_structure_id());
        anyhow::ensure!(
            derived_channel_keys == channel_keys,
            "channel secret mismatch in channel init data"
        );

        let as_ref = {
            let mut db = coord.0.db.lock().unwrap();
            let mut coordinator = coord.0.coordinator.lock().unwrap();
            coordinator.staged_mutate(&mut *db, |coordinator| {
                Ok(coordinator.add_key_and_access_structure(
                    init_data.key_name.clone(),
                    init_data.root_shared_key.clone(),
                    init_data.purpose,
                    encryption_key,
                    &mut rand::thread_rng(),
                ))
            })?
        };

        coord.0.emit_key_state();
        Ok(as_ref.key_id)
    }

    /// Send a chat message to a channel, optionally as a reply.
    /// The nsec is used to sign the message.
    pub async fn send_message(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        content: String,
        reply_to: Option<EventId>,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let event_id = handle
            .send_message(content, reply_to.map(|id| id.into()), &keys)
            .await?;
        Ok(event_id.into())
    }

    /// Propose a transaction for signing over the channel.
    pub async fn send_sign_request(
        &self,
        access_structure_ref: AccessStructureRef,
        nsec: String,
        unsigned_tx: super::signing::UnsignedTx,
        message: String,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let sign_task = WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone());
        let handle = self.get_handle(access_structure_ref.access_structure_id)?;
        let event_id = handle.send_sign_request(&keys, sign_task, message).await?;
        Ok(event_id.into())
    }

    /// Propose a test message for signing over the channel.
    pub async fn send_test_sign_request(
        &self,
        access_structure_ref: AccessStructureRef,
        nsec: String,
        test_message: String,
        message: String,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let sign_task = WireSignTask::Test {
            message: test_message,
        };
        let handle = self.get_handle(access_structure_ref.access_structure_id)?;
        let event_id = handle.send_sign_request(&keys, sign_task, message).await?;
        Ok(event_id.into())
    }

    /// Send a signing offer with pre-allocated binonces over the channel.
    pub async fn send_sign_offer(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        request_id: EventId,
        binonces: Vec<ParticipantBinonces>,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let event_id = handle
            .send_sign_offer(&keys, request_id.into(), binonces)
            .await?;
        Ok(event_id.into())
    }

    /// Send signature shares over the channel. `offer_subset` is the
    /// ordered list of offer event ids the shares were computed against;
    /// the combiner uses them to recover binonces and derive session_id.
    /// Dart is responsible for getting shares from the coordinator first.
    pub async fn send_sign_partial(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        request_id: EventId,
        offer_subset: Vec<EventId>,
        shares: frostsnap_core::coordinator::ParticipantSignatureShares,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let offer_subset: Vec<frostsnap_nostr::EventId> =
            offer_subset.into_iter().map(|e| e.into()).collect();
        let event_id = handle
            .send_sign_partial(&keys, request_id.into(), offer_subset, shares)
            .await?;
        Ok(event_id.into())
    }

    /// Cancel a signing request. Only the original requester should call this.
    pub async fn send_sign_cancel(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        request_id: EventId,
    ) -> Result<EventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let event_id = handle.send_sign_cancel(&keys, request_id.into()).await?;
        Ok(event_id.into())
    }

    /// Disconnect from a channel.
    pub fn disconnect_channel(&self, access_structure_id: AccessStructureId) {
        self.channels.lock().unwrap().remove(&access_structure_id);
    }

    /// Open a remote keygen lobby at `channel_secret` as the initiator.
    /// Publishes the nostr `ChannelCreation` event — with the wallet
    /// name + purpose carried inline in its content — and returns a
    /// handle the caller uses to register devices or cancel.
    pub async fn create_remote_lobby(
        &self,
        channel_secret: ChannelSecret,
        nsec: String,
        key_name: String,
        purpose: KeyPurpose,
    ) -> Result<self::remote_keygen::RemoteLobbyHandle> {
        let keys = Keys::parse(&nsec)?;
        let lobby_client = LobbyClient::new(channel_secret.clone());
        let invite_link = channel_secret.keygen_invite_link();
        let metadata = LobbyChannelMetadata { key_name, purpose };
        let init_event = lobby_client.build_creation_event(&keys, &metadata).await?;
        let (bridge, sink) = self::remote_keygen::RemoteLobbyHandle::build_bridge();
        let handle = lobby_client
            .run(self.client.clone(), keys.clone(), Some(init_event), sink)
            .await?;
        Ok(self::remote_keygen::RemoteLobbyHandle::new(
            handle,
            keys,
            invite_link,
            self.client.clone(),
            bridge,
        ))
    }

    /// Join an existing remote keygen lobby. `channel_secret` comes from
    /// parsing the initiator's `frostsnap://channel/…` invite link
    /// (see `ChannelSecret::from_invite_link`).
    pub async fn join_remote_lobby(
        &self,
        channel_secret: ChannelSecret,
        nsec: String,
    ) -> Result<self::remote_keygen::RemoteLobbyHandle> {
        let keys = Keys::parse(&nsec)?;
        let lobby_client = LobbyClient::new(channel_secret.clone());
        let invite_link = channel_secret.keygen_invite_link();
        let (bridge, sink) = self::remote_keygen::RemoteLobbyHandle::build_bridge();
        let handle = lobby_client
            .run(self.client.clone(), keys.clone(), None, sink)
            .await?;
        Ok(self::remote_keygen::RemoteLobbyHandle::new(
            handle,
            keys,
            invite_link,
            self.client.clone(),
            bridge,
        ))
    }

    fn get_handle(&self, access_structure_id: AccessStructureId) -> Result<ChannelHandle> {
        self.channels
            .lock()
            .unwrap()
            .get(&access_structure_id)
            .cloned()
            .ok_or_else(|| anyhow!("channel not connected"))
    }
}

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
        Keys::parse(&self.0).expect("validated").public_key().into()
    }
}

// ============================================================================
// PublicKey - Value mirror (32 bytes, x-only)
// ============================================================================

#[frb(mirror(ChannelSecret))]
pub struct _ChannelSecret(pub [u8; 16]);

#[frb(external)]
impl ChannelSecret {
    #[frb(sync)]
    pub fn from_access_structure_id(_id: &AccessStructureId) -> Self {}

    /// `frostsnap://channel/<hex>` — post-keygen wallet channel invite.
    #[frb(sync)]
    pub fn invite_link(&self) -> String {}

    /// `frostsnap://keygen/<hex>` — visually distinct scheme used for
    /// remote keygen lobby invites so the receiver UI can route to the
    /// lobby-join flow rather than the already-completed-wallet flow.
    #[frb(sync)]
    pub fn keygen_invite_link(&self) -> String {}
}

pub trait ChannelSecretExt {
    #[frb(sync)]
    fn from_invite_link(link: &str) -> Result<ChannelSecret>;

    #[frb(sync)]
    fn from_keygen_link(link: &str) -> Result<ChannelSecret>;

    /// Generate a fresh random `ChannelSecret`. Named `generate` to avoid
    /// colliding with the native `ChannelSecret::random(rng)` (which Dart
    /// can't call since it needs an RNG argument).
    #[frb(sync)]
    fn generate() -> ChannelSecret;
}

impl ChannelSecretExt for ChannelSecret {
    #[frb(sync)]
    fn from_invite_link(link: &str) -> Result<ChannelSecret> {
        parse_frostsnap_link(link).map_err(|e| anyhow!("{e}"))
    }

    #[frb(sync)]
    fn from_keygen_link(link: &str) -> Result<ChannelSecret> {
        parse_keygen_link(link).map_err(|e| anyhow!("{e}"))
    }

    #[frb(sync)]
    fn generate() -> ChannelSecret {
        ChannelSecret::random(&mut rand::thread_rng())
    }
}

/// Mirrors `frostsnap_nostr::PublicKey`. Same `dart_code` override as
/// `EventId` — content-equality `==` / `hashCode` so `PublicKey` works
/// as a `Map` key on the Dart side.
#[frb(
    mirror(PublicKey),
    non_opaque,
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is PublicKey && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _PublicKey(pub [u8; 32]);

#[frb(external)]
impl PublicKey {
    #[frb(sync)]
    pub fn to_hex(&self) -> String {}

    #[frb(sync)]
    pub fn to_npub(&self) -> String {}
}

// ============================================================================
// EventId mirror — value-typed, 32 bytes. Mirrors `frostsnap_nostr::EventId`.
// ============================================================================

/// Mirrors `frostsnap_nostr::EventId`. Custom dart_code overrides
/// `==` and `hashCode` to compare by content (so `EventId` works as a
/// `Map` key in Dart), since the auto-generated tuple-struct equality
/// would compare by reference.
#[frb(
    mirror(EventId),
    non_opaque,
    non_hash,
    non_eq,
    dart_code = "
  @override
  int get hashCode => Object.hashAll(field0);

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is EventId && _listEquals(field0, other.field0));

  static bool _listEquals(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
"
)]
pub struct _EventId(pub [u8; 32]);

#[frb(external)]
impl EventId {
    #[frb(sync)]
    pub fn to_hex(&self) -> String {}
}

/// A member of the channel group with their profile. Mirrors
/// `frostsnap_nostr::GroupMember`.
#[frb(mirror(GroupMember), non_opaque)]
pub struct _GroupMember {
    pub pubkey: PublicKey,
    pub profile: Option<NostrProfile>,
}

/// Mirrors `frostsnap_nostr::ConnectionState`.
#[frb(mirror(ConnectionState), non_opaque)]
pub enum _ConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}

/// Mirrors `frostsnap_nostr::signing::ConfirmedSubsetEntry` — one entry
/// in `SigningEvent::RoundConfirmed.subset`.
#[frb(mirror(ConfirmedSubsetEntry), non_opaque)]
pub struct _ConfirmedSubsetEntry {
    pub event_id: EventId,
    pub author: PublicKey,
    pub timestamp: u64,
    pub binonces: Vec<ParticipantBinonces>,
}

/// Mirrors `frostsnap_nostr::SigningEvent`. Variants stay 1:1 with the
/// source; the previously-embedded `signing_details` (Request) and
/// `share_indices` (Offer) projections are now Dart-callable helpers
/// (see `signing_details`, `offer_share_indices` below). The
/// `RoundConfirmed.sealed` opaque bundle is built on demand via
/// `ChannelHandle::seal_round_confirmed(...)`.
#[frb(mirror(SigningEvent), non_opaque)]
pub enum _SigningEvent {
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
    RoundConfirmed {
        request_id: EventId,
        subset: Vec<ConfirmedSubsetEntry>,
        session_id: SignSessionId,
        sign_task: WireSignTask,
        timestamp: u64,
    },
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

/// Mirrors `frostsnap_nostr::ChannelEvent`. The previous bespoke
/// `FfiChannelEvent` is gone — variants flow through unchanged.
#[frb(mirror(ChannelEvent), non_opaque)]
pub enum _ChannelEvent {
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

/// Compute the UI-shape `SigningDetails` for a sign task. Dart calls
/// this on demand; the field used to ride embedded on
/// `SigningEvent::Request` but it's just a derivation of the task.
#[frb(sync)]
pub fn signing_details(sign_task: &WireSignTask) -> super::signing::SigningDetails {
    use super::signing::WireSignTaskExt;
    sign_task.signing_details()
}

/// Compute the per-binonce share indices, the UI-friendly projection
/// of `SigningEvent::Offer.binonces`. Dart calls this on demand; the
/// field used to ride embedded on the variant.
#[frb(sync)]
pub fn offer_share_indices(binonces: &[ParticipantBinonces]) -> Vec<u32> {
    binonces
        .iter()
        .map(|b| u32::try_from(b.share_index).expect("share index fits in u32"))
        .collect()
}

/// Opaque bundle of round data needed to combine signatures standalone.
/// Built on demand by `ChannelHandle::seal_round_confirmed(...)` when a
/// `SigningEvent::RoundConfirmed` lands, since it carries the channel's
/// `key_context` (which isn't on the source `SigningEvent` itself).
#[frb(opaque)]
#[derive(Debug, Clone)]
pub struct SealedSigningData {
    pub(crate) request_id: frostsnap_nostr::EventId,
    pub(crate) sign_task: WireSignTask,
    pub(crate) binonces: Vec<ParticipantBinonces>,
    pub(crate) key_context: KeyContext,
}

impl SealedSigningData {
    #[frb(sync)]
    pub fn sign_task(&self) -> WireSignTask {
        self.sign_task.clone()
    }

    #[frb(sync)]
    pub fn access_structure_ref(&self) -> AccessStructureRef {
        self.key_context.access_structure_ref()
    }

    #[frb(sync)]
    pub fn binonces(&self) -> Vec<ParticipantBinonces> {
        self.binonces.clone()
    }

    #[frb(sync)]
    pub fn sign_session_id(&self) -> SignSessionId {
        use frostsnap_core::message::GroupSignReq;
        GroupSignReq::from_binonces(
            self.sign_task.clone(),
            self.key_context.access_structure_id(),
            &self.binonces,
        )
        .session_id()
    }

    #[frb(sync)]
    pub fn combine_signatures(
        &self,
        all_shares: Vec<RustAutoOpaque<ParticipantSignatureShares>>,
    ) -> anyhow::Result<Vec<EncodedSignature>> {
        let guards: Vec<_> = all_shares
            .iter()
            .map(|s: &RustAutoOpaque<ParticipantSignatureShares>| s.blocking_read())
            .collect();
        let share_refs: Vec<&ParticipantSignatureShares> = guards.iter().map(|g| &**g).collect();
        Ok(frostsnap_core::coordinator::signing::combine_signatures(
            self.sign_task.clone(),
            &self.key_context,
            &self.binonces,
            &share_refs,
        )?)
    }
}

/// `Sink<ChannelEvent>` that forwards the source enum directly to a
/// `StreamSink<ChannelEvent>` — no per-event conversion. The
/// `SealedSigningData` bundle that used to ride embedded on
/// `RoundConfirmed` is now built on demand via
/// `ChannelHandle::seal_round_confirmed(...)`, which already has the
/// channel's `key_context`.
#[derive(Clone)]
struct ChannelEventSink {
    sink: StreamSink<ChannelEvent>,
}

impl frostsnap_coordinator::Sink<ChannelEvent> for ChannelEventSink {
    fn send(&self, event: ChannelEvent) {
        let _ = self.sink.add(event);
    }
}

/// Nostr profile metadata (NIP-01 kind 0). Mirrors `frostsnap_nostr::NostrProfile`.
#[frb(mirror(NostrProfile), non_opaque)]
pub struct _NostrProfile {
    pub pubkey: Option<PublicKey>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub banner: Option<String>,
    pub nip05: Option<String>,
    pub website: Option<String>,
}

/// Get default relay URLs (public relays for profile/event discovery).
#[frb(sync)]
pub fn default_relay_urls() -> Vec<String> {
    vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
        "wss://relay.primal.net".to_string(),
    ]
}
