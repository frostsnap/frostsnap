pub mod keygen_run;
pub mod remote_keygen;

use crate::api::broadcast::{BehaviorBroadcast, BehaviorBroadcastSubscription, StartError};
use crate::frb_generated::{RustAutoOpaque, StreamSink};
use crate::nostr_settings_state::NostrSettingsState;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::persist::Persisted;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::{
    coordinator::{KeyContext, ParticipantBinonces, ParticipantSignatureShares},
    message::EncodedSignature,
    AccessStructureId, AccessStructureRef, KeyId, SignSessionId, SymmetricKey, WireSignTask,
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

/// FRB-translatable view of an access structure's local Nostr-coordination
/// settings. Carried directly by the per-access-structure broadcast so
/// fresh subscribers see current state without a separate sync read.
#[derive(Clone, Debug)]
pub struct AccessStructureSettings {
    pub key_id: KeyId,
    pub coordination_ui_enabled: bool,
}

impl AccessStructureSettings {
    fn default_for_ref(asref: AccessStructureRef) -> Self {
        Self {
            key_id: asref.key_id,
            coordination_ui_enabled: false,
        }
    }
}

/// Nostr identity + per-access-structure coordination settings.
/// Mirrors the `Settings` pattern (`api/settings.rs`): a `Persisted<State>`
/// with a `Mutation` enum, plus the FRB-side stream sinks. Lives behind a
/// `RustAutoOpaque` on `AppCtx`, so `&mut self` setters are fine.
#[frb(opaque)]
pub struct NostrSettings {
    settings: Persisted<NostrSettingsState>,
    db: Arc<Mutex<Connection>>,

    /// Sink for identity updates. Emits the local pubkey; Dart computes
    /// npub on demand via `PublicKey.toNpub()`.
    identity_sink: Option<StreamSink<Option<PublicKey>>>,

    /// One `BehaviorBroadcast` per access structure. Lazily created on
    /// first subscribe (or pre-populated from `load`); each carries the
    /// latest `AccessStructureSettings` so newly-mounted UI sees the
    /// current state immediately. Multi-subscriber on the Rust side, no
    /// shared subject on the Dart side.
    coordination_broadcasts:
        RwLock<HashMap<AccessStructureId, BehaviorBroadcast<AccessStructureSettings>>>,
}

impl NostrSettings {
    /// Create by loading existing state from SQLite.
    /// Called during app initialization.
    pub(crate) fn new(db: Arc<Mutex<Connection>>, data_dir: PathBuf) -> Result<Self> {
        let settings: Persisted<NostrSettingsState> = {
            let mut conn = db.lock().unwrap();
            Persisted::new(&mut *conn, ())?
        };

        // Pre-populate one broadcast per loaded access structure so a fresh
        // subscriber sees the current state without waiting for a change
        // event.
        let coordination_broadcasts = RwLock::new(HashMap::new());
        {
            let mut map = coordination_broadcasts.write().unwrap();
            for (asid, settings) in &settings.access_structure_settings {
                map.insert(
                    *asid,
                    BehaviorBroadcast::seeded(AccessStructureSettings {
                        key_id: settings.key_id,
                        coordination_ui_enabled: settings.coordination_ui_enabled,
                    }),
                );
            }
        }

        // Initialize LMDB at the configured data dir.
        get_or_init_nostr_lmdb(&data_dir);

        Ok(Self {
            settings,
            db,
            identity_sink: None,
            coordination_broadcasts,
        })
    }

    /// Subscribe to identity changes. Emits current value immediately.
    /// Stream values are the local nostr pubkey; Dart computes the
    /// bech32 `npub` on demand via `PublicKey.toNpub()`.
    pub fn sub_identity(&mut self, sink: StreamSink<Option<PublicKey>>) -> Result<()> {
        self.identity_sink.replace(sink);
        self.emit_identity();
        Ok(())
    }

    /// Subscribe to per-access-structure coordination settings updates.
    /// New subscribers immediately receive the current `AccessStructureSettings`
    /// (the BehaviorBroadcast cached value); subsequent emissions arrive
    /// every time `set_coordination_ui_enabled` is called for this asref.
    #[frb(sync)]
    pub fn sub_access_structure(
        &self,
        access_structure_ref: AccessStructureRef,
    ) -> AccessStructureSettingsBroadcastSubscription {
        let broadcast = self.broadcast_for(access_structure_ref);
        AccessStructureSettingsBroadcastSubscription(broadcast.subscribe())
    }

    /// Get-or-create the broadcast for an access structure, seeding the
    /// cached value with the latest persisted settings (or the default
    /// `coordination_ui_enabled = false` for an asref we've never seen).
    fn broadcast_for(
        &self,
        access_structure_ref: AccessStructureRef,
    ) -> BehaviorBroadcast<AccessStructureSettings> {
        let asid = access_structure_ref.access_structure_id;
        if let Some(b) = self.coordination_broadcasts.read().unwrap().get(&asid) {
            return b.clone();
        }
        let mut map = self.coordination_broadcasts.write().unwrap();
        // Re-check inside the write lock; another caller may have inserted.
        if let Some(b) = map.get(&asid) {
            return b.clone();
        }
        let initial = self
            .settings
            .access_structure_settings
            .get(&asid)
            .map(|s| AccessStructureSettings {
                key_id: s.key_id,
                coordination_ui_enabled: s.coordination_ui_enabled,
            })
            .unwrap_or_else(|| AccessStructureSettings::default_for_ref(access_structure_ref));
        let broadcast = BehaviorBroadcast::seeded(initial);
        map.insert(asid, broadcast.clone());
        broadcast
    }

    fn emit_identity(&self) {
        if let Some(sink) = &self.identity_sink {
            let _ = sink.add(self.settings.pubkey);
        }
    }

    fn emit_access_structure(&self, asref: AccessStructureRef) {
        let asid = asref.access_structure_id;
        let snapshot = self
            .settings
            .access_structure_settings
            .get(&asid)
            .map(|s| AccessStructureSettings {
                key_id: s.key_id,
                coordination_ui_enabled: s.coordination_ui_enabled,
            })
            .unwrap_or_else(|| AccessStructureSettings::default_for_ref(asref));
        self.broadcast_for(asref).add(&snapshot);
    }

    /// Get current identity synchronously.
    #[frb(sync)]
    pub fn current(&self) -> Option<PublicKey> {
        self.settings.pubkey
    }

    /// Set/import nsec. Persists and notifies subscribers.
    pub fn set_nsec(&mut self, nsec: String) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        self.settings
            .mutate2(&mut *conn, |st, update| st.set_nsec(Some(nsec), update))?;
        drop(conn);
        self.emit_identity();
        tracing::info!("Nostr identity configured");
        Ok(())
    }

    /// Remove the configured Nostr signing identity.
    /// Does not delete chat history or per-access-structure settings.
    pub fn clear_nsec(&mut self) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        self.settings
            .mutate2(&mut *conn, |st, update| st.set_nsec(None, update))?;
        drop(conn);
        self.emit_identity();
        tracing::info!("Nostr identity cleared");
        Ok(())
    }

    /// Generate new random identity.
    pub fn generate(&mut self) -> Result<String> {
        let keys = Keys::generate();
        let nsec = keys.secret_key().to_bech32()?;
        self.set_nsec(nsec.clone())?;
        Ok(nsec)
    }

    /// Get nsec for export/backup. Returns an error if no identity is set.
    #[frb(sync)]
    pub fn get_nsec(&self) -> Result<String> {
        self.settings
            .nsec
            .clone()
            .ok_or_else(|| anyhow!("no Nostr identity configured"))
    }

    /// Check if identity exists.
    #[frb(sync)]
    pub fn has_identity(&self) -> bool {
        self.settings.pubkey.is_some()
    }

    /// Whether the wallet behind this access structure should render its
    /// chat-first (remote-coordinated) UI shape.
    #[frb(sync)]
    pub fn is_coordination_ui_enabled(&self, access_structure_id: AccessStructureId) -> bool {
        self.settings
            .is_coordination_ui_enabled(access_structure_id)
    }

    pub fn set_coordination_ui_enabled(
        &mut self,
        access_structure_ref: AccessStructureRef,
        enabled: bool,
    ) -> Result<()> {
        let mut conn = self.db.lock().unwrap();
        self.settings.mutate2(&mut *conn, |st, update| {
            st.set_coordination_ui_enabled(access_structure_ref, enabled, update);
            Ok(())
        })?;
        drop(conn);
        self.emit_access_structure(access_structure_ref);
        Ok(())
    }
}

/// FRB-typed wrapper around the per-access-structure subscription so the
/// Dart side can call `start(StreamSink)` / `stop()` directly. Mirrors
/// the `LobbyStateBroadcastSubscription` pattern.
pub struct AccessStructureSettingsBroadcastSubscription(
    pub(crate) BehaviorBroadcastSubscription<AccessStructureSettings>,
);

impl AccessStructureSettingsBroadcastSubscription {
    #[frb(sync)]
    pub fn id(&self) -> u32 {
        self.0._id()
    }

    #[frb(sync)]
    pub fn is_running(&self) -> bool {
        self.0._is_running()
    }

    #[frb(sync)]
    pub fn start(
        &self,
        sink: StreamSink<AccessStructureSettings>,
    ) -> std::result::Result<(), StartError> {
        self.0._start(sink)
    }

    #[frb(sync)]
    pub fn stop(&self) -> bool {
        self.0._stop()
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
        let event_id = handle.send_message(content, reply_to, &keys).await?;
        Ok(event_id)
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
        Ok(event_id)
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
        Ok(event_id)
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
        let event_id = handle.send_sign_offer(&keys, request_id, binonces).await?;
        Ok(event_id)
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
        let event_id = handle
            .send_sign_partial(&keys, request_id, offer_subset, shares)
            .await?;
        Ok(event_id)
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
        let event_id = handle.send_sign_cancel(&keys, request_id).await?;
        Ok(event_id)
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
