use crate::frb_generated::{RustAutoOpaque, StreamSink};
use crate::sink_wrap::SinkWrap;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    message::EncodedSignature,
    AccessStructureId, AccessStructureRef, SignSessionId, SymmetricKey, WireSignTask,
};
use frostsnap_nostr::{
    ChannelClient, ChannelHandle, ChannelInitData, Client, Keys, NostrDatabaseExt, NostrLMDB,
    NostrProfile, SigningChain, ToBech32,
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
    pub(crate) as_id: AccessStructureId,
    pub(crate) signing_key: KeyContext,
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
    pub async fn fetch_profile(&self, pubkey: &PublicKey) -> Result<Option<FfiNostrProfile>> {
        // 📦 Check cache first
        if let Ok(Some(metadata)) = self.client.database().metadata(*pubkey).await {
            return Ok(Some(FfiNostrProfile::from_metadata(*pubkey, metadata)));
        }

        // 🌐 Fetch from relays
        match self
            .client
            .fetch_metadata(*pubkey, Duration::from_secs(5))
            .await
        {
            Ok(Some(metadata)) => Ok(Some(FfiNostrProfile::from_metadata(*pubkey, metadata))),
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
        let channel_client = ChannelClient::new(params.as_id, params.signing_key, params.init_data);
        let handle = match channel_client
            .run(self.client.clone(), SinkWrap(sink))
            .await
        {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "failed to connect to channel");
                return;
            }
        };

        self.channels.lock().unwrap().insert(params.as_id, handle);
    }

    /// Join a wallet from a nostr invite link. Fetches channel data from relays,
    /// adds the key to the coordinator, and returns the new wallet's KeyId.
    pub async fn join_from_link(
        &self,
        coord: &super::coordinator::Coordinator,
        channel_secret: ChannelSecret,
        encryption_key: SymmetricKey,
    ) -> Result<frostsnap_core::KeyId> {
        let init_data = frostsnap_nostr::fetch_channel_init(&self.client, &channel_secret)
            .await?
            .ok_or_else(|| anyhow!("no channel found for this link"))?;

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
        reply_to: Option<NostrEventId>,
    ) -> Result<NostrEventId> {
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
    ) -> Result<NostrEventId> {
        let keys = Keys::parse(&nsec)?;
        let sign_task = WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone());
        let handle = self.get_handle(access_structure_ref.access_structure_id)?;
        let event_id = handle
            .send_sign_request(&keys, sign_task, access_structure_ref, message)
            .await?;
        Ok(event_id.into())
    }

    /// Propose a test message for signing over the channel.
    pub async fn send_test_sign_request(
        &self,
        access_structure_ref: AccessStructureRef,
        nsec: String,
        test_message: String,
        message: String,
    ) -> Result<NostrEventId> {
        let keys = Keys::parse(&nsec)?;
        let sign_task = WireSignTask::Test {
            message: test_message,
        };
        let handle = self.get_handle(access_structure_ref.access_structure_id)?;
        let event_id = handle
            .send_sign_request(&keys, sign_task, access_structure_ref, message)
            .await?;
        Ok(event_id.into())
    }

    /// Send a signing offer with pre-allocated binonces over the channel.
    pub async fn send_sign_offer(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        reply_to: NostrEventId,
        binonces: ParticipantBinonces,
    ) -> Result<NostrEventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let event_id = handle
            .send_sign_offer(&keys, reply_to.into(), binonces)
            .await?;
        Ok(event_id.into())
    }

    /// Send signature shares over the channel.
    /// Dart is responsible for getting shares from the coordinator first.
    pub async fn send_sign_partial(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        request_id: NostrEventId,
        session_id: SignSessionId,
        shares: frostsnap_core::coordinator::ParticipantSignatureShares,
    ) -> Result<NostrEventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let event_id = handle
            .send_sign_partial(&keys, request_id.into(), session_id, shares)
            .await?;
        Ok(event_id.into())
    }

    /// Cancel a signing request. Only the original requester should call this.
    pub async fn send_sign_cancel(
        &self,
        access_structure_id: AccessStructureId,
        nsec: String,
        request_id: NostrEventId,
    ) -> Result<NostrEventId> {
        let keys = Keys::parse(&nsec)?;
        let handle = self.get_handle(access_structure_id)?;
        let event_id = handle.send_sign_cancel(&keys, request_id.into()).await?;
        Ok(event_id.into())
    }

    /// Disconnect from a channel.
    pub fn disconnect_channel(&self, access_structure_id: AccessStructureId) {
        self.channels.lock().unwrap().remove(&access_structure_id);
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

    #[frb(sync)]
    pub fn invite_link(&self) -> String {}
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
// NostrEventId - Non-opaque wrapper for EventId with proper Dart equality
// ============================================================================

/// A Nostr event ID (32 bytes). This is a non-opaque wrapper that provides
/// proper equality semantics in Dart for use as Map keys.
#[frb(
    non_opaque,
    non_hash,
    non_eq,
    dart_code = "
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
"
)]
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

/// A member of the channel group with their profile.
#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub struct FfiGroupMember {
    pub pubkey: PublicKey,
    pub profile: Option<FfiNostrProfile>,
}

impl From<GroupMember> for FfiGroupMember {
    fn from(m: GroupMember) -> Self {
        FfiGroupMember {
            pubkey: m.pubkey,
            profile: m.profile.map(|p| p.into()),
        }
    }
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
    ConnectionState(FfiConnectionState),
    GroupMetadata {
        members: Vec<FfiGroupMember>,
    },
    SigningEvent(FfiSigningEvent),
    Error {
        event_id: NostrEventId,
        author: PublicKey,
        timestamp: u64,
        reason: String,
    },
}

/// Opaque bundle of chain data needed to combine signatures standalone.
#[frb(opaque)]
#[derive(Debug, Clone)]
pub struct SealedSigningData(pub(crate) SigningChain);

impl SealedSigningData {
    #[frb(sync)]
    pub fn sign_task(&self) -> WireSignTask {
        self.0.sign_task.clone()
    }

    #[frb(sync)]
    pub fn access_structure_ref(&self) -> AccessStructureRef {
        self.0.access_structure_ref
    }

    #[frb(sync)]
    pub fn binonces(&self) -> Vec<ParticipantBinonces> {
        self.0.binonces.clone()
    }

    #[frb(sync)]
    pub fn sign_session_id(&self) -> SignSessionId {
        use frostsnap_core::message::GroupSignReq;
        GroupSignReq::from_binonces(
            self.0.sign_task.clone(),
            self.0.access_structure_ref.access_structure_id,
            &self.0.binonces,
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
            self.0.sign_task.clone(),
            &self.0.signing_key,
            &self.0.binonces,
            &share_refs,
        )?)
    }
}

#[frb(non_opaque)]
#[derive(Debug, Clone)]
pub enum FfiSigningEvent {
    Request {
        event_id: NostrEventId,
        author: PublicKey,
        sign_task: crate::frb_generated::RustAutoOpaque<WireSignTask>,
        signing_details: super::signing::SigningDetails,
        access_structure_ref: AccessStructureRef,
        message: String,
        timestamp: u64,
    },
    Offer {
        event_id: NostrEventId,
        author: PublicKey,
        request_id: NostrEventId,
        share_index: u32,
        sealed: Option<crate::frb_generated::RustAutoOpaque<SealedSigningData>>,
        timestamp: u64,
    },
    Partial {
        event_id: NostrEventId,
        author: PublicKey,
        request_id: NostrEventId,
        session_id: SignSessionId,
        shares: frostsnap_core::coordinator::ParticipantSignatureShares,
        timestamp: u64,
    },
    Cancel {
        event_id: NostrEventId,
        author: PublicKey,
        request_id: NostrEventId,
        timestamp: u64,
    },
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
            ChannelEvent::MessageSent { message_id } => FfiChannelEvent::MessageSent {
                message_id: message_id.into(),
            },
            ChannelEvent::MessageSendFailed { message_id, reason } => {
                FfiChannelEvent::MessageSendFailed {
                    message_id: message_id.into(),
                    reason,
                }
            }
            ChannelEvent::ConnectionState(state) => FfiChannelEvent::ConnectionState(match state {
                ConnectionState::Connecting => FfiConnectionState::Connecting,
                ConnectionState::Connected => FfiConnectionState::Connected,
                ConnectionState::Disconnected { reason } => {
                    FfiConnectionState::Disconnected { reason }
                }
            }),
            ChannelEvent::GroupMetadata { members } => FfiChannelEvent::GroupMetadata {
                members: members.into_iter().map(|m| m.into()).collect(),
            },
            ChannelEvent::Frostsnap(frostsnap_event) => {
                use frostsnap_nostr::events::{FrostsnapEvent, SigningEvent};
                match frostsnap_event {
                    FrostsnapEvent::Signing(signing) => {
                        FfiChannelEvent::SigningEvent(match signing {
                            SigningEvent::Request {
                                event_id,
                                author,
                                sign_task,
                                access_structure_ref,
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
                                    access_structure_ref,
                                    message,
                                    timestamp,
                                }
                            }
                            SigningEvent::Offer {
                                event_id,
                                author,
                                request_id,
                                binonces,
                                sealed,
                                timestamp,
                            } => FfiSigningEvent::Offer {
                                event_id: event_id.into(),
                                author,
                                request_id: request_id.into(),
                                share_index: u32::try_from(binonces.share_index)
                                    .expect("share index should fit in u32"),
                                sealed: sealed.map(|chain| {
                                    crate::frb_generated::RustAutoOpaque::new(SealedSigningData(
                                        chain,
                                    ))
                                }),
                                timestamp,
                            },
                            SigningEvent::Partial {
                                event_id,
                                author,
                                request_id,
                                session_id,
                                signature_shares,
                                timestamp,
                            } => FfiSigningEvent::Partial {
                                event_id: event_id.into(),
                                author,
                                request_id: request_id.into(),
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
                        })
                    }
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
}

/// Nostr profile metadata (NIP-01 kind 0).
#[frb(non_opaque)]
#[derive(Debug, Clone, Default)]
pub struct FfiNostrProfile {
    pub pubkey: Option<PublicKey>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub picture: Option<String>,
    pub banner: Option<String>,
    pub nip05: Option<String>,
    pub website: Option<String>,
}

impl FfiNostrProfile {
    pub(crate) fn from_metadata(pubkey: PublicKey, metadata: frostsnap_nostr::Metadata) -> Self {
        FfiNostrProfile {
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

impl From<NostrProfile> for FfiNostrProfile {
    fn from(p: NostrProfile) -> Self {
        FfiNostrProfile {
            pubkey: p.pubkey,
            name: p.name,
            display_name: p.display_name,
            about: p.about,
            picture: p.picture,
            banner: p.banner,
            nip05: p.nip05,
            website: p.website,
        }
    }
}

/// Default port for the local test relay.
pub const TEST_RELAY_PORT: u16 = 7447;

/// Get default relay URLs (public relays for profile/event discovery).
#[frb(sync)]
pub fn default_relay_urls() -> Vec<String> {
    vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
        "wss://relay.primal.net".to_string(),
    ]
}

/// Get relay URLs for development (includes local test relay).
#[frb(sync)]
pub fn dev_relay_urls() -> Vec<String> {
    vec![
        format!("ws://localhost:{}", TEST_RELAY_PORT),
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
    ]
}
