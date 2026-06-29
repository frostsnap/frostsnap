//! We keep chain at arms length from the rest of the code by only communicating through mpsc channels.
use anyhow::{anyhow, Result};
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{
    bitcoin::{self, BlockHash},
    spk_client::{self},
    CheckPoint, ConfirmationBlockTime,
};
use bdk_electrum_streaming::{
    electrum_streaming_client::request, run_async, AsyncReceiver, AsyncState, Cache,
    DerivedSpkTracker, ReqCoord, Update,
};
use frostsnap_core::MasterAppkey;
use futures::{
    channel::{mpsc, oneshot},
    executor::{block_on, block_on_stream},
    select,
    stream::FuturesUnordered,
    FutureExt, StreamExt,
};
use futures::{pin_mut, select_biased};
use std::{
    collections::BTreeMap,
    fmt::Debug,
    ops::Deref,
    sync::{
        self,
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{event, Level};

use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;
use crate::Sink;

use super::{
    descriptor_for_account_keychain,
    handler_state::{ConnectedTo, Establish, HandlerState},
    status_tracker::ConnPhase,
    tofu::{
        connection::{Conn, TargetServerReq},
        trusted_certs::TrustedCertificates,
        verifier::UntrustedCertificate,
    },
    wallet::{CoordSuperWallet, KeychainId},
};

#[derive(Debug)]
pub struct ReqAndResponse<I, O> {
    request: I,
    response: oneshot::Sender<O>,
}

impl<I: Send, O: Send> ReqAndResponse<I, O> {
    pub fn new(request: I) -> (Self, oneshot::Receiver<O>) {
        let (response, response_recv) = oneshot::channel();
        (Self { request, response }, response_recv)
    }

    pub fn into_tuple(self) -> (I, oneshot::Sender<O>) {
        (self.request, self.response)
    }
}

pub const SUPPORTED_NETWORKS: [bitcoin::Network; 4] = {
    use bitcoin::Network::*;
    [Bitcoin, Signet, Testnet, Regtest]
};

pub type SyncResponse = spk_client::SyncResponse<ConfirmationBlockTime>;
pub type KeychainClient = bdk_electrum_streaming::AsyncClient<KeychainId>;
pub type KeychainClientReceiver = bdk_electrum_streaming::AsyncReceiver<KeychainId>;

/// The messages the client can send to the backend
pub enum Message {
    ChangeUrlReq(ReqAndResponse<TargetServerReq, Result<ConnectionResult>>),
    SetStatusSink(Box<dyn Sink<ChainStatus>>),
    /// Start the client loop (sent once when first sync request is made)
    StartClient,
    Reconnect,
    TrustCertificate {
        server_url: String,
        certificate_der: Vec<u8>,
    },
    /// Connect to a specific server (primary or backup)
    ConnectTo {
        use_backup: bool,
    },
}

/// Result of a connection attempt
#[derive(Debug, Clone)]
pub enum ConnectionResult {
    Success,
    CertificatePromptNeeded(UntrustedCertificate),
    Failed(String),
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::ChangeUrlReq(_) => write!(f, "Message::ChangeUrlReq"),
            Message::SetStatusSink(_) => write!(f, "Message::SetStatusSink"),
            Message::StartClient => write!(f, "Message::StartClient"),
            Message::Reconnect => write!(f, "Message::Reconnect"),
            Message::TrustCertificate { server_url, .. } => {
                write!(f, "Message::TrustCertificate({})", server_url)
            }
            Message::ConnectTo { use_backup } => {
                write!(f, "Message::ConnectTo(use_backup={})", use_backup)
            }
        }
    }
}

/// The settings-derived desired state for connecting: which servers and whether enabled.
/// This is the single source of truth shared (via a watch) from the api to the handler —
/// the handler queries it and reacts to its change signal; there is no replica.
#[derive(Clone, Debug)]
pub struct ElectrumConfig {
    pub enabled: ElectrumEnabled,
    pub primary: String,
    pub backup: String,
}

/// Opaque API to the chain
#[derive(Clone)]
pub struct ChainClient {
    req_sender: mpsc::UnboundedSender<Message>,
    client: KeychainClient,
    connection_requested: Arc<AtomicBool>,
    config_tx: watch::Sender<ElectrumConfig>,
}

impl ChainClient {
    pub fn new(
        genesis_hash: BlockHash,
        config: ElectrumConfig,
        trusted_certificates: Persisted<TrustedCertificates>,
        db: Arc<sync::Mutex<rusqlite::Connection>>,
    ) -> (Self, ConnectionHandler) {
        let (req_sender, req_recv) = mpsc::unbounded();
        let (client, client_recv) = KeychainClient::new();
        let (config_tx, config_rx) = watch::channel(config);
        let cache = Cache::default();
        (
            Self {
                req_sender,
                client: client.clone(),
                connection_requested: Arc::new(AtomicBool::new(false)),
                config_tx,
            },
            ConnectionHandler {
                req_recv,
                client_recv,
                cache,
                client,
                genesis_hash,
                trusted_certificates,
                db,
                config_rx,
            },
        )
    }

    pub fn check_and_set_electrum_server_url(
        &self,
        url: String,
        is_backup: bool,
    ) -> Result<ConnectionResult> {
        self.start_client();
        let (req, response) = ReqAndResponse::new(TargetServerReq { url, is_backup });
        self.req_sender
            .unbounded_send(Message::ChangeUrlReq(req))
            .unwrap();
        block_on(response)?
    }

    pub fn monitor_keychain(&self, keychain: KeychainId, next_index: u32) {
        self.start_client();
        let descriptor = descriptor_for_account_keychain(
            keychain,
            // this does not matter
            bitcoin::NetworkKind::Main,
        );
        self.client
            .track_descriptor(keychain, descriptor, next_index)
            .expect("must track keychain");
    }

    pub fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid> {
        self.start_client();
        let txid = transaction.compute_txid();
        event!(Level::DEBUG, "Broadcasting: {}", transaction.compute_txid());
        block_on(self.client.send_request(request::BroadcastTx(transaction)))
            .inspect_err(|err| {
                tracing::error!(
                    txid = txid.to_string(),
                    error = err.to_string(),
                    "Failed to broadcast tx"
                )
            })
            .inspect(|txid| tracing::info!(txid = txid.to_string(), "Successfully broadcasted tx"))
    }

    pub fn estimate_fee(
        &self,
        target_blocks: impl IntoIterator<Item = usize>,
    ) -> Result<BTreeMap<usize, bitcoin::FeeRate>> {
        self.start_client();
        use futures::FutureExt;
        block_on_stream(
            target_blocks
                .into_iter()
                .map(|number| {
                    self.client
                        .send_request(request::EstimateFee { number })
                        .map(move |request_result| {
                            request_result
                                .map(|resp| resp.fee_rate.map(|fee_rate| (number, fee_rate)))
                                .transpose()
                        })
                })
                .collect::<FuturesUnordered<_>>(),
        )
        .flatten()
        .collect()
    }

    pub fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        self.req_sender
            .unbounded_send(Message::SetStatusSink(sink))
            .unwrap();
    }

    pub fn reconnect(&self) {
        self.req_sender.unbounded_send(Message::Reconnect).unwrap();
    }

    pub fn trust_certificate(&self, server_url: String, certificate_der: Vec<u8>) {
        self.req_sender
            .unbounded_send(Message::TrustCertificate {
                server_url,
                certificate_der,
            })
            .unwrap();
    }

    fn start_client(&self) {
        if !self.connection_requested.swap(true, Ordering::Relaxed) {
            self.req_sender
                .unbounded_send(Message::StartClient)
                .unwrap();
        }
    }

    pub fn set_urls(&self, primary: String, backup: String) {
        self.config_tx.send_modify(|c| {
            c.primary = primary;
            c.backup = backup;
        });
    }

    /// Update a single server url (used after a `ChangeUrlReq` probe succeeds).
    pub fn set_electrum_url(&self, url: String, is_backup: bool) {
        self.config_tx.send_modify(|c| {
            if is_backup {
                c.backup = url;
            } else {
                c.primary = url;
            }
        });
    }

    pub fn set_enabled(&self, enabled: ElectrumEnabled) {
        self.config_tx.send_modify(|c| c.enabled = enabled);
    }

    pub fn connect_to(&self, use_backup: bool) {
        self.start_client();
        self.req_sender
            .unbounded_send(Message::ConnectTo { use_backup })
            .unwrap();
    }
}

pub const fn default_electrum_server(network: bitcoin::Network) -> &'static str {
    // a tooling bug means we need this
    #[allow(unreachable_patterns)]
    match network {
        bitcoin::Network::Bitcoin => "ssl://blockstream.info:700",
        // we're using the tcp:// version since ssl ain't working for some reason
        bitcoin::Network::Testnet => "tcp://electrum.blockstream.info:60001",
        bitcoin::Network::Testnet4 => "ssl://blackie.c3-soft.com:57010",
        bitcoin::Network::Regtest => "tcp://localhost:60401",
        // tcp:// because most public signet servers' SSL certs are rejected by rustls
        // (old X.509 versions), and mempool.space's signet electrum is currently unreliable.
        bitcoin::Network::Signet => "tcp://signet.musdomworks.com:50001",
        _ => panic!("Unknown network"),
    }
}

pub const fn default_backup_electrum_server(network: bitcoin::Network) -> &'static str {
    // a tooling bug means we need this
    #[allow(unreachable_patterns)]
    match network {
        bitcoin::Network::Bitcoin => "ssl://electrum.acinq.co:50002",
        bitcoin::Network::Testnet => "ssl://blockstream.info:993",
        bitcoin::Network::Testnet4 => "ssl://mempool.space:40002",
        // Standard fallback: backend is currently down, but it has a valid cert and is
        // expected to recover. The primary (musdom) is the signet server that works today.
        bitcoin::Network::Signet => "ssl://mempool.space:60602",
        bitcoin::Network::Regtest => "tcp://127.0.0.1:51001",
        _ => panic!("Unknown network"),
    }
}

pub struct ConnectionHandler {
    client: KeychainClient,
    client_recv: KeychainClientReceiver,
    req_recv: mpsc::UnboundedReceiver<Message>,
    cache: Cache,
    genesis_hash: BlockHash,
    trusted_certificates: Persisted<TrustedCertificates>,
    db: Arc<sync::Mutex<rusqlite::Connection>>,
    config_rx: watch::Receiver<ElectrumConfig>,
}

impl ConnectionHandler {
    pub fn run<SW, F>(mut self, super_wallet: SW, update_action: F)
    where
        SW: Deref<Target = sync::Mutex<CoordSuperWallet>> + Clone + Send + 'static,
        F: FnMut(MasterAppkey, Vec<crate::bitcoin::wallet::Transaction>) + Send + 'static,
    {
        let lookahead: u32;
        let chain_tip: CheckPoint;
        let network: bitcoin::Network;
        {
            let super_wallet = super_wallet.lock().expect("must lock");
            network = super_wallet.network;
            lookahead = super_wallet.lookahead();
            chain_tip = super_wallet.chain_tip();
            self.cache.txs.extend(super_wallet.tx_cache());
            self.cache.anchors.extend(super_wallet.anchor_cache());
        }

        tracing::info!("Running ConnectionHandler for {} network", network);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("cannot build tokio runtime");

        let (update_sender, update_recv) = mpsc::unbounded::<Update<KeychainId>>();

        let _wallet_updates_jh = rt.spawn_blocking({
            let super_wallet = super_wallet.clone();
            move || Self::handle_wallet_updates(super_wallet, update_recv, update_action)
        });

        let handler = HandlerState::new(
            self.genesis_hash,
            self.config_rx.clone(),
            self.trusted_certificates,
            self.db,
        );

        let electrum_state = AsyncState::<KeychainId>::new(
            ReqCoord::new(rand::random::<u32>()),
            self.cache,
            DerivedSpkTracker::new(lookahead),
            chain_tip,
        );

        let mut conn_loop = ConnLoop {
            handler,
            client: self.client,
            req_recv: self.req_recv,
            client_recv: self.client_recv,
            update_sender,
            electrum_state,
            config_rx: self.config_rx,
        };

        rt.block_on(conn_loop.drive());
    }

    fn handle_wallet_updates<SW, F>(
        super_wallet: SW,
        update_recv: mpsc::UnboundedReceiver<Update<KeychainId>>,
        mut action: F,
    ) where
        SW: Deref<Target = sync::Mutex<CoordSuperWallet>> + Clone + Send + 'static,
        F: FnMut(MasterAppkey, Vec<crate::bitcoin::wallet::Transaction>) + Send,
    {
        for update in block_on_stream(update_recv) {
            let master_appkeys = update
                .last_active_indices
                .keys()
                .map(|(k, _)| *k)
                .collect::<Vec<_>>();
            let mut wallet = super_wallet.lock().unwrap();
            let changed = match wallet.apply_update(update) {
                Ok(changed) => changed,
                Err(err) => {
                    tracing::error!("Failed to apply wallet update: {}", err);
                    continue;
                }
            };
            if changed {
                for master_appkey in master_appkeys {
                    let txs = wallet.list_transactions(master_appkey);
                    action(master_appkey, txs);
                }
            }
        }
    }
}

/// The next state for the connection driver. Each variant is handled by one method that
/// awaits exactly the events meaningful in that state and returns the next `Next`, so the
/// connection lifecycle is an explicit state machine rather than a loop with flags.
enum Next {
    /// Not started or disabled: park without opening a socket (lazy).
    Idle,
    /// Try to establish a connection (with primary→backup failover).
    Connect,
    /// Service a live connection.
    Service(Conn, ConnectedTo),
    /// Wait before the next connect attempt. `failover` rotates away from the failed server
    /// (`was_on_backup`) first, so we don't tight-loop on a server that can't serve us.
    Backoff { failover: bool, was_on_backup: bool },
    /// The control channel closed: stop the loop.
    Stop,
}

/// Owns the connection loop's working state for the lifetime of the runtime and drives it as a
/// state machine. `HandlerState` holds connection policy (which server, enabled, status,
/// certs); `ConnLoop` holds the live I/O plumbing plus the handler.
struct ConnLoop {
    handler: HandlerState,
    client: KeychainClient,
    req_recv: mpsc::UnboundedReceiver<Message>,
    client_recv: KeychainClientReceiver,
    update_sender: mpsc::UnboundedSender<Update<KeychainId>>,
    electrum_state: AsyncState<KeychainId>,
    config_rx: watch::Receiver<ElectrumConfig>,
}

impl ConnLoop {
    const PING_DELAY: Duration = Duration::from_secs(21);
    const PING_TIMEOUT: Duration = Duration::from_secs(3);

    async fn drive(&mut self) {
        let mut next = Next::Idle;
        loop {
            next = match next {
                Next::Idle => self.idle().await,
                Next::Connect => self.connect().await,
                Next::Service(conn, info) => self.service(conn, info).await,
                Next::Backoff {
                    failover,
                    was_on_backup,
                } => self.backoff(failover, was_on_backup).await,
                Next::Stop => return,
            };
        }
    }

    /// Park until we should connect, staying responsive to control messages and config
    /// changes. No socket is opened here, so connections stay lazy.
    async fn idle(&mut self) -> Next {
        if self.handler.should_connect() {
            return Next::Connect;
        }
        // Entering the parked state: this is the single funnel for "not connecting", so
        // asserting Idle here also recovers the status after a disable that interrupted an
        // in-flight connect (which left the phase on Connecting).
        self.handler.status.set_phase(ConnPhase::Idle);
        while !self.handler.should_connect() {
            select_biased! {
                msg_opt = self.req_recv.next() => match msg_opt {
                    Some(msg) => { self.handler.handle_msg(msg).await; }
                    None => return Next::Stop,
                },
                res = self.config_rx.changed().fuse() => match res {
                    Ok(()) => self.handler.on_config_changed(),
                    Err(_) => return Next::Stop,
                },
            }
        }
        Next::Connect
    }

    /// Establish a connection. The single place that decides whether to connect at all: when
    /// disabled it parks (`Idle`). The attempt (incl. primary→backup failover) is raced ONLY
    /// against the config watch — never `req_recv` — so a benign control message can't cancel
    /// an in-flight connect. A connect failure becomes a `Backoff` (not an in-line sleep).
    async fn connect(&mut self) -> Next {
        if !self.handler.should_connect() {
            return Next::Idle;
        }
        match self.handler.establish(&mut self.config_rx).await {
            Establish::Connected(conn, info) => Next::Service(conn, info),
            Establish::Retry => Next::Backoff {
                failover: false,
                was_on_backup: false,
            },
            Establish::TargetChanged => {
                // Re-emit status (urls may have changed); re-evaluate from the top — the new
                // target may be disabled, a different server, etc.
                self.handler.on_config_changed();
                Next::Idle
            }
            Establish::Closed => Next::Stop,
        }
    }

    /// Service a live connection until it ends, then tear it down (status + socket shutdown)
    /// before returning. The return value encodes *why* it ended and *what next*: a graceful
    /// stop or deliberate reconnect → `Connect`; a session failure (sync error / ping timeout)
    /// → `Backoff` with failover; a disable → `Idle`.
    async fn service(&mut self, mut conn: Conn, info: ConnectedTo) -> Next {
        let Self {
            handler,
            client,
            req_recv,
            client_recv,
            update_sender,
            electrum_state,
            config_rx,
        } = self;

        let next = {
            let conn_fut =
                Self::run_connection(&mut conn, electrum_state, client_recv, update_sender).fuse();
            let ping_fut = async {
                loop {
                    tokio::time::sleep(Self::PING_DELAY).await;

                    let req_fut = client.send_request(request::Ping).fuse();
                    let req_timeout_fut = tokio::time::sleep(Self::PING_TIMEOUT).fuse();
                    pin_mut!(req_fut);
                    pin_mut!(req_timeout_fut);
                    select! {
                        result = req_fut => {
                            if let Err(err) = result {
                                return err;
                            }
                            tracing::trace!("Received pong from server");
                        },
                        _ = req_timeout_fut => {
                            return anyhow!("Timeout waiting for pong");
                        },
                    }
                }
            }
            .fuse();
            pin_mut!(conn_fut);
            pin_mut!(ping_fut);

            loop {
                select_biased! {
                    msg_opt = req_recv.next() => {
                        let msg = match msg_opt {
                            Some(msg) => msg,
                            None => break Next::Stop,
                        };
                        tracing::info!(msg = msg.to_string(), "Handling message");
                        let should_reconnect = handler.handle_msg(msg).await;

                        if !handler.should_connect() {
                            tracing::info!("Electrum disabled");
                            break Next::Idle;
                        }
                        if should_reconnect {
                            tracing::info!("Breaking connection loop due to reconnect request");
                            break Next::Connect;
                        }
                    }
                    res = config_rx.changed().fuse() => {
                        if res.is_err() {
                            break Next::Stop;
                        }
                        // Always re-emit status (displayed urls may have changed), but only tear
                        // down the live connection if the change affects the server we're on — a
                        // change to the other slot (e.g. toggling the backup while on the
                        // primary) leaves it untouched.
                        handler.on_config_changed();
                        if handler.reconnect_needed(info.on_backup, &info.url) {
                            tracing::info!("Breaking connection loop: config change affects the current server");
                            break Next::Connect;
                        }
                    }
                    err = ping_fut => {
                        tracing::error!(error = err.to_string(), "Failed to keep connection alive");
                        break Next::Backoff { failover: true, was_on_backup: info.on_backup };
                    },
                    res = conn_fut => {
                        break match res {
                            Ok(()) => {
                                tracing::info!("Connection service stopped gracefully");
                                Next::Connect
                            }
                            Err(err) => {
                                // `{err:#}` keeps the cause chain on one line without anyhow's
                                // backtrace dump.
                                tracing::warn!(error = format!("{err:#}"), "Connection service failed");
                                Next::Backoff { failover: true, was_on_backup: info.on_backup }
                            }
                        };
                    },
                }
            }
        };

        // The control channel closed: stop without touching status (the app is going away).
        if matches!(next, Next::Stop) {
            return Next::Stop;
        }

        // Leaving the connection: assert the phase from the authoritative predicate (disabled
        // → Idle, else Disconnected) before the socket shutdown, so the UI never lingers on a
        // stale Connected while the socket closes. We don't infer it from `next`: a disable
        // arrives as a config change that maps to Next::Connect, the same as a same-server
        // reconnect, so `next` can't tell them apart.
        let leaving = if handler.should_connect() {
            ConnPhase::Disconnected
        } else {
            ConnPhase::Idle
        };
        handler.status.set_phase(leaving);
        let shutdown_result = match conn {
            Conn::Tcp((rh, wh)) => rh.unsplit(wh).shutdown().await,
            Conn::Ssl((rh, wh)) => rh.unsplit(wh).shutdown().await,
        };
        tracing::info!(result = ?shutdown_result, "Connection shutdown");
        next
    }

    /// Wait before the next connect attempt, interruptibly. On a session failure (`failover`),
    /// rotate to the other server first.
    async fn backoff(&mut self, failover: bool, was_on_backup: bool) -> Next {
        // Between attempts we are disconnected (will retry). The service teardown already
        // asserted this on the failover path; on the connect-failure path the phase is still
        // Connecting, so assert it here. Deduped, so the redundant case is silent.
        self.handler.status.set_phase(ConnPhase::Disconnected);
        if failover {
            self.handler.prefer_other_server(was_on_backup);
        }
        select_biased! {
            res = self.config_rx.changed().fuse() => match res {
                Ok(()) => self.handler.on_config_changed(),
                Err(_) => return Next::Stop,
            },
            _ = tokio::time::sleep(HandlerState::RECONNECT_DELAY).fuse() => {}
        }
        Next::Connect
    }

    /// Run the sync loop with an established connection. `Ok` is a graceful stop; `Err` is a
    /// session failure (e.g. the server passed the connectivity probe but rejected the real
    /// sync) — `service` turns that into a failover `Backoff`.
    async fn run_connection(
        conn: &mut Conn,
        state: &mut AsyncState<KeychainId>,
        client_recv: &mut AsyncReceiver<KeychainId>,
        update_sender: &mut mpsc::UnboundedSender<Update<KeychainId>>,
    ) -> Result<()> {
        let conn_result = match conn {
            Conn::Tcp((read_half, write_half)) => {
                run_async(
                    state,
                    update_sender,
                    client_recv,
                    read_half.compat(),
                    write_half.compat_write(),
                )
                .await
            }
            Conn::Ssl((read_half, write_half)) => {
                run_async(
                    state,
                    update_sender,
                    client_recv,
                    read_half.compat(),
                    write_half.compat_write(),
                )
                .await
            }
        };
        conn_result.map(|_| ())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ChainStatus {
    pub primary_url: String,
    pub backup_url: String,
    pub on_backup: bool,
    pub state: ChainStatusState,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChainStatusState {
    /// No connection has been attempted yet
    Idle,
    Connecting,
    Connected,
    Disconnected,
}
