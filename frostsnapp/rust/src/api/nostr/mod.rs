pub mod remote_keygen;

use crate::frb_generated::{RustAutoOpaque, StreamSink};
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    message::EncodedSignature,
    AccessStructureId, AccessStructureRef, SignSessionId, SymmetricKey, WireSignTask,
};
use frostsnap_core::device::KeyPurpose;
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
    ChannelEvent, ChannelSecret, ConnectionState, EventId, GroupMember, PublicKey,
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
    identity_sink: Option<StreamSink<FfiNostrIdentity>>,
}

/// Current Nostr identity state.
#[derive(Clone, Default)]
#[frb(non_opaque)]
pub struct FfiNostrIdentity {
    pub pubkey: Option<PublicKey>,
    pub npub: Option<String>,
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
        nsec.and_then(|n| Keys::parse(&n).ok().map(|k| k.public_key()))
    }

    /// Subscribe to identity changes. Emits current value immediately.
    pub fn sub_identity(&self, sink: StreamSink<FfiNostrIdentity>) -> Result<()> {
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
            let identity = FfiNostrIdentity {
                pubkey: inner.pubkey,
                npub: inner.pubkey.as_ref().and_then(|p| p.to_bech32().ok()),
            };
            let _ = sink.add(identity);
        }
    }

    /// Get current identity synchronously.
    #[frb(sync)]
    pub fn current(&self) -> FfiNostrIdentity {
        let inner = self.inner.read().unwrap();
        FfiNostrIdentity {
            pubkey: inner.pubkey.clone(),
            npub: inner.pubkey.as_ref().and_then(|p| p.to_bech32().ok()),
        }
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
            inner.pubkey = Some(keys.public_key());
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
        })
    }

    /// Fetch profile metadata for a public key.
    /// Checks the local cache first, then fetches from relays if not found.
    /// Returns None if the user has no profile.
    pub async fn fetch_profile(&self, pubkey: &PublicKey) -> Result<Option<NostrProfile>> {
        // 📦 Check cache first
        if let Ok(Some(metadata)) = self.client.database().metadata(*pubkey).await {
            return Ok(Some(NostrProfile::from_metadata(*pubkey, metadata)));
        }

        // 🌐 Fetch from relays
        match self
            .client
            .fetch_metadata(*pubkey, Duration::from_secs(5))
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
        sink: StreamSink<FfiChannelEvent>,
    ) {
        let access_structure_id = params.key_context.access_structure_id();
        // Custom sink that smuggles the channel's KeyContext into the
        // FfiChannelEvent conversion so SealedSigningData can carry it.
        let channel_sink = ChannelEventSink {
            sink,
            key_context: params.key_context.clone(),
        };
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
        let init_event = lobby_client
            .build_creation_event(&keys, &metadata)
            .await?;
        let (broadcast, sink) = self::remote_keygen::RemoteLobbyHandle::build_bridge();
        let handle = lobby_client
            .run(self.client.clone(), keys.clone(), Some(init_event), sink)
            .await?;
        Ok(self::remote_keygen::RemoteLobbyHandle::new(
            handle,
            keys,
            invite_link,
            broadcast,
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
        let (broadcast, sink) = self::remote_keygen::RemoteLobbyHandle::build_bridge();
        let handle = lobby_client
            .run(self.client.clone(), keys.clone(), None, sink)
            .await?;
        Ok(self::remote_keygen::RemoteLobbyHandle::new(
            handle,
            keys,
            invite_link,
            broadcast,
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
        Keys::parse(&self.0).expect("validated").public_key()
    }
}

// ============================================================================
// PublicKey - Opaque mirror of nostr_sdk::PublicKey
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

#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub enum FfiChannelEvent {
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
    SigningEvent {
        event: FfiSigningEvent,
        pending: bool,
    },
    Error {
        event_id: EventId,
        author: PublicKey,
        timestamp: u64,
        reason: String,
    },
}

/// Opaque bundle of round data needed to combine signatures standalone.
/// Built by `channel_event_to_ffi` when a `SigningEvent::RoundConfirmed`
/// lands. Holds the channel's `key_context` so all the channel-scope context
/// (`access_structure_ref`, threshold, etc.) is recoverable from a single
/// place.
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

#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub enum FfiSigningEvent {
    Request {
        event_id: EventId,
        author: PublicKey,
        sign_task: crate::frb_generated::RustAutoOpaque<WireSignTask>,
        signing_details: super::signing::SigningDetails,
        message: String,
        timestamp: u64,
    },
    Offer {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        share_indices: Vec<u32>,
        timestamp: u64,
    },
    /// Emitted exactly once per round when the settling timer expires with
    /// at least `threshold` offers observed. Dart uses `sealed` to drive the
    /// signing UI and combine shares; `subset_event_ids` / `subset_authors`
    /// let the UI render which offers made the cut.
    RoundConfirmed {
        request_id: EventId,
        session_id: SignSessionId,
        subset_event_ids: Vec<EventId>,
        subset_authors: Vec<PublicKey>,
        sealed: crate::frb_generated::RustAutoOpaque<SealedSigningData>,
        timestamp: u64,
    },
    /// Emitted when the settling timer expires with fewer than `threshold`
    /// offers. The round is still collecting; this is a provisional
    /// snapshot. May fire multiple times as new offers arrive and later
    /// quiet periods pass.
    RoundPending {
        request_id: EventId,
        observed: Vec<EventId>,
        threshold: u32,
        timestamp: u64,
    },
    Partial {
        event_id: EventId,
        author: PublicKey,
        request_id: EventId,
        /// Offer event ids whose binonces were combined to sign this
        /// partial. Mirrors the wire field; Dart renders these for audit.
        offer_subset: Vec<EventId>,
        /// Computed by the Rust tree from `offer_subset`'s binonces; denorm
        /// for UI convenience.
        session_id: SignSessionId,
        shares: frostsnap_core::coordinator::ParticipantSignatureShares,
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

/// Mirrors `frostsnap_nostr::ConnectionState`.
#[frb(mirror(ConnectionState), non_opaque)]
pub enum _ConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}

/// Convert a `ChannelEvent` from the channel client into the FFI variant.
/// Takes the channel's `key_context` so `SealedSigningData` can be built with
/// the channel-scope context that no longer travels on `SigningChain`.
fn channel_event_to_ffi(event: ChannelEvent, key_context: &KeyContext) -> FfiChannelEvent {
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
        ChannelEvent::MessageSent { message_id } => FfiChannelEvent::MessageSent {
            message_id: message_id.into(),
        },
        ChannelEvent::MessageSendFailed { message_id, reason } => {
            FfiChannelEvent::MessageSendFailed {
                message_id: message_id.into(),
                reason,
            }
        }
        ChannelEvent::ConnectionState(state) => FfiChannelEvent::ConnectionState(state),
        ChannelEvent::GroupMetadata { members } => FfiChannelEvent::GroupMetadata { members },
        ChannelEvent::Signing {
            event: signing,
            pending,
        } => {
            use frostsnap_nostr::signing::SigningEvent;
            FfiChannelEvent::SigningEvent {
                event: match signing {
                    SigningEvent::Request {
                        event_id,
                        author,
                        sign_task,
                        message,
                        timestamp,
                    } => {
                        use super::signing::WireSignTaskExt;
                        let signing_details = sign_task.signing_details();
                        FfiSigningEvent::Request {
                            event_id: event_id.into(),
                            author,
                            sign_task: crate::frb_generated::RustAutoOpaque::new(sign_task),
                            signing_details,
                            message,
                            timestamp,
                        }
                    }
                    SigningEvent::Offer {
                        event_id,
                        author,
                        request_id,
                        binonces,
                        timestamp,
                    } => FfiSigningEvent::Offer {
                        event_id: event_id.into(),
                        author,
                        request_id: request_id.into(),
                        share_indices: binonces
                            .iter()
                            .map(|b| {
                                u32::try_from(b.share_index).expect("share index should fit in u32")
                            })
                            .collect(),
                        timestamp,
                    },
                    SigningEvent::RoundConfirmed {
                        request_id,
                        subset,
                        session_id,
                        sign_task,
                        timestamp,
                    } => {
                        let binonces: Vec<ParticipantBinonces> = subset
                            .iter()
                            .flat_map(|e| e.binonces.iter().cloned())
                            .collect();
                        let sealed = crate::frb_generated::RustAutoOpaque::new(SealedSigningData {
                            request_id,
                            sign_task,
                            binonces,
                            key_context: key_context.clone(),
                        });
                        FfiSigningEvent::RoundConfirmed {
                            request_id: request_id.into(),
                            session_id,
                            subset_event_ids: subset.iter().map(|e| e.event_id.into()).collect(),
                            subset_authors: subset.iter().map(|e| e.author).collect(),
                            sealed,
                            timestamp,
                        }
                    }
                    SigningEvent::RoundPending {
                        request_id,
                        observed,
                        threshold,
                        timestamp,
                    } => FfiSigningEvent::RoundPending {
                        request_id: request_id.into(),
                        observed: observed.into_iter().map(|eid| eid.into()).collect(),
                        threshold: threshold as u32,
                        timestamp,
                    },
                    SigningEvent::Partial {
                        event_id,
                        author,
                        request_id,
                        offer_subset,
                        session_id,
                        signature_shares,
                        timestamp,
                    } => FfiSigningEvent::Partial {
                        event_id: event_id.into(),
                        author,
                        request_id: request_id.into(),
                        offer_subset: offer_subset.into_iter().map(|e| e.into()).collect(),
                        session_id,
                        shares: signature_shares,
                        timestamp,
                    },
                    SigningEvent::Cancel {
                        event_id,
                        author,
                        request_id,
                        timestamp,
                    } => FfiSigningEvent::Cancel {
                        event_id: event_id.into(),
                        author,
                        request_id: request_id.into(),
                        timestamp,
                    },
                    SigningEvent::Rejected {
                        event_id,
                        author,
                        timestamp,
                        reason,
                    } => FfiSigningEvent::Rejected {
                        event_id: event_id.into(),
                        author,
                        timestamp,
                        reason,
                    },
                },
                pending,
            }
        }
        ChannelEvent::Error {
            event_id,
            author,
            timestamp,
            reason,
        } => FfiChannelEvent::Error {
            event_id: event_id.into(),
            author,
            timestamp,
            reason,
        },
    }
}

/// `Sink<ChannelEvent>` impl that smuggles the channel's `KeyContext` into
/// the FfiChannelEvent conversion path so `SealedSigningData` can carry it.
#[derive(Clone)]
struct ChannelEventSink {
    sink: StreamSink<FfiChannelEvent>,
    key_context: KeyContext,
}

impl frostsnap_coordinator::Sink<ChannelEvent> for ChannelEventSink {
    fn send(&self, event: ChannelEvent) {
        let ffi_event = channel_event_to_ffi(event, &self.key_context);
        let _ = self.sink.add(ffi_event);
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
