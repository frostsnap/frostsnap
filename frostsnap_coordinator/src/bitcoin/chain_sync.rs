//! We keep chain at arms length from the rest of the code by only communicating through mpsc channels.
use anyhow::{anyhow, Result};
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{
    bitcoin::{self, BlockHash},
    spk_client::{self},
    CheckPoint, ConfirmationBlockTime,
};
use bdk_electrum_streaming::{
    electrum_streaming_client::request,
    run_async, AsyncReceiver, AsyncState, Cache, DerivedSpkTracker, ReqCoord, Update,
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
    sync::{self, Arc},
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{event, Level};

use crate::Sink;

use super::{
    descriptor_for_account_keychain,
    tofu::{
        connection::{Conn, TargetServer, TargetServerReq},
        verifier::{TofuError, UntrustedCertificate},
        trusted_certs::TrustedCertificates,
    },
    wallet::{CoordSuperWallet, KeychainId},
};
use crate::persist::Persisted;

/// Improve error messages for known error patterns
fn improve_error_message(error: &str) -> String {
    if error.contains("UnsupportedCertVersion") {
        return "The server's certificate uses an unsupported version (not X.509 v3). This server may be using outdated certificate technology.".to_string();
    }
    
    if error.contains("certificate not valid for name") && error.contains("certificate is only valid for") {
        // This case is already handled well by rustls, just pass through
        return error.to_string();
    }
    
    // For any other errors, return as-is
    error.to_string()
}

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
    Reconnect,
    TrustCertificate { server_url: String, certificate_der: Vec<u8> },
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
            Message::Reconnect => write!(f, "Message::Reconnect"),
            Message::TrustCertificate { server_url, .. } => write!(f, "Message::TrustCertificate({})", server_url),
        }
    }
}

impl Message {
    pub fn is_status_sink(&self) -> bool {
        matches!(self, Message::SetStatusSink(_))
    }
}

/// Opaque API to the chain
#[derive(Clone)]
pub struct ChainClient {
    req_sender: mpsc::UnboundedSender<Message>,
    client: KeychainClient,
}

impl ChainClient {
    pub fn new(
        genesis_hash: BlockHash, 
        trusted_certificates: Persisted<TrustedCertificates>,
        db: Arc<sync::Mutex<rusqlite::Connection>>,
    ) -> (Self, ConnectionHandler) {
        let (req_sender, req_recv) = mpsc::unbounded();
        let (client, client_recv) = KeychainClient::new();
        let cache = Cache::default();
        (
            Self {
                req_sender,
                client: client.clone(),
            },
            ConnectionHandler {
                req_recv,
                client_recv,
                cache,
                client,
                genesis_hash,
                trusted_certificates,
                db,
            },
        )
    }

    pub fn check_and_set_electrum_server_url(&self, url: String, is_backup: bool) -> Result<ConnectionResult> {
        let (req, response) = ReqAndResponse::new(TargetServerReq { url, is_backup });
        self.req_sender
            .unbounded_send(Message::ChangeUrlReq(req))
            .unwrap();
        block_on(response)?
    }

    pub fn monitor_keychain(&self, keychain: KeychainId, next_index: u32) {
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
            .unbounded_send(Message::TrustCertificate { server_url, certificate_der })
            .unwrap();
    }
}

pub const fn default_electrum_server(network: bitcoin::Network) -> &'static str {
    match network {
        bitcoin::Network::Bitcoin => "ssl://electrum.frostsn.app:50002",
        // we're using the tcp:// version since ssl ain't working for some reason
        bitcoin::Network::Testnet => "tcp://electrum.blockstream.info:60001",
        bitcoin::Network::Testnet4 => "ssl://blackie.c3-soft.com:57010",
        bitcoin::Network::Regtest => "tcp://localhost:60401",
        bitcoin::Network::Signet => "tcp://electrum.frostsn.app:60001",
        _ => panic!("Unknown network"),
    }
}

pub const fn default_backup_electrum_server(network: bitcoin::Network) -> &'static str {
    match network {
        bitcoin::Network::Bitcoin => "ssl://blockstream.info:700",
        bitcoin::Network::Testnet => "ssl://blockstream.info:993",
        bitcoin::Network::Testnet4 => "ssl://mempool.space:40002",
        bitcoin::Network::Signet => "tcp://signet-electrumx.wakiyamap.dev:50001",
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
}

impl ConnectionHandler {
    const PING_DELAY: Duration = Duration::from_secs(21);
    const PING_TIMEOUT: Duration = Duration::from_secs(3);
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    const RECONNECT_DELAY: Duration = Duration::from_secs(2);

    pub fn run<SW, F>(mut self, url: String, backup_url: String, super_wallet: SW, update_action: F)
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

        let (mut update_sender, update_recv) = mpsc::unbounded::<Update<KeychainId>>();

        let _wallet_updates_jh = rt.spawn_blocking({
            let super_wallet = super_wallet.clone();
            move || Self::handle_wallet_updates(super_wallet, update_recv, update_action)
        });

        rt.block_on({
            let mut state = AsyncState::<KeychainId>::new(
                ReqCoord::new(rand::random::<u32>()),
                self.cache,
                DerivedSpkTracker::new(lookahead),
                chain_tip,
            );

            let mut conn_stage = TargetServer { url, backup_url, conn: None, backup_conn: None };
            // The current connection (if any).
            let mut conn_opt = Option::<Conn>::None;

            let mut sink_stage = Option::<Box<dyn Sink<ChainStatus>>>::None;
            let mut sink: Box<dyn Sink<ChainStatus>> = Box::new(());

            async move {
                // Reconnection loop.
                loop {
                    if let Some(new_conn) = conn_stage.take_conn() {
                        conn_opt = Some(new_conn);
                    }
                    if let Some(new_sink) = sink_stage.take() {
                        sink = new_sink;
                    }

                    // Try to establish connection if needed (only borrows trusted_certificates briefly)
                    let has_connection = Self::try_connect(
                        self.genesis_hash,
                        &conn_stage.url,
                        &conn_stage.backup_url,
                        &mut conn_opt,
                        &*sink,
                        &mut self.trusted_certificates,
                        self.db.clone(),
                    ).await;

                    if !has_connection {
                        // No connection available, wait before retrying
                        tokio::time::sleep(Self::RECONNECT_DELAY).await;
                        continue;
                    }

                    {
                        // Now run the connection (doesn't need trusted_certificates)
                        let url = conn_stage.url.clone();
                        let conn_fut = Self::run_connection(
                            &url,
                            &mut conn_opt,
                            &mut state,
                            &mut self.client_recv,
                            &mut update_sender,
                            &*sink,
                        )
                        .fuse();
                        let ping_fut = async {
                            loop {
                                tokio::time::sleep(Self::PING_DELAY).await;

                                let req_fut = self.client.send_request(request::Ping).fuse();
                                let req_timeout_fut = tokio::time::sleep(Self::PING_TIMEOUT).fuse();
                                pin_mut!(req_fut);
                                pin_mut!(req_timeout_fut);
                                select! {
                                    result = req_fut => {
                                        if let Err(err) = result {
                                            return err;
                                        }
                                        tracing::info!("Received pong from server");
                                    },
                                    _ = req_timeout_fut => {
                                        return anyhow!("Timeout waiting for pong");
                                    },
                                }
                            }
                        }.fuse();
                        pin_mut!(conn_fut);
                        pin_mut!(ping_fut);
                        
                        // Keep handling messages until connection fails
                        loop {
                            select_biased! {
                                msg_opt = self.req_recv.next() => {
                                    let msg = match msg_opt {
                                        Some(msg) => msg,
                                        None => return,
                                    };
                                    tracing::info!(msg = msg.to_string(), "Handling message");
                                    // Now we can handle the message directly since trusted_certificates is not borrowed
                                    Self::handle_msg(self.genesis_hash, msg, &mut sink_stage, &mut conn_stage, &mut self.trusted_certificates, &self.db).await;
                                    
                                    // Check if a new connection was staged (server change)
                                    if let Some((_, new_url)) = conn_stage.staged_connection() {
                                        tracing::info!(
                                            current_url = %url,
                                            new_url = %new_url,
                                            "New connection staged, restarting connection loop to switch servers"
                                        );
                                        break;
                                    }
                                    // Otherwise continue the inner loop to handle more messages
                                }
                                err = ping_fut => {
                                    tracing::error!(error = err.to_string(), "Failed to keep connection alive");
                                    break; // Exit inner loop on ping failure
                                },
                                _ = conn_fut => {
                                    tracing::debug!("Connection service stopped");
                                    break; // Exit inner loop when connection stops
                                },
                            }
                        }
                    }

                    if let Some(old_conn) = conn_opt.take() {
                        let shutdown_result = match old_conn {
                            Conn::Tcp((rh, wh)) => rh.unsplit(wh).shutdown().await,
                            Conn::Ssl((rh, wh)) => rh.unsplit(wh).shutdown().await,
                        };
                        tracing::info!(result = ?shutdown_result, "Connection shutdown");
                    }
                }
            }
        });
    }

    /// Try to establish a connection if one doesn't exist
    /// Returns true if a connection is available (either existing or newly established)
    async fn try_connect(
        genesis_hash: BlockHash,
        url: &str,
        backup_url: &str,
        conn_opt: &mut Option<Conn>,
        status_sink: &dyn Sink<ChainStatus>,
        trusted_certificates: &mut Persisted<TrustedCertificates>,
        db: Arc<sync::Mutex<rusqlite::Connection>>,
    ) -> bool {
        if conn_opt.is_some() {
            status_sink.send(ChainStatus::new(url, ChainStatusState::Connected));
            tracing::info!("Using previously established connection with {}.", url);
            return true;
        }

        for url in [url, backup_url] {
            status_sink.send(ChainStatus::new(url, ChainStatusState::Connecting));
            tracing::info!("No existing connection. Connecting to {}.", url);

            match Conn::new(genesis_hash, url, Self::CONNECT_TIMEOUT, trusted_certificates, db.clone()).await {
                Ok(conn) => {
                    status_sink.send(ChainStatus::new(url, ChainStatusState::Connected));
                    tracing::info!("Connection established with {}.", url);
                    *conn_opt = Some(conn);
                    return true;
                }
                Err(err) => {
                    status_sink.send(ChainStatus::new(url, ChainStatusState::Disconnected));
                    tracing::error!(err = err.to_string(), url, "failed to connect",);
                }
            }
        }
        
        tracing::error!(
            reconnecting_in_secs = Self::RECONNECT_DELAY.as_secs_f32(),
            "Failed to connect to all Electrum servers"
        );
        false
    }

    /// Run the sync loop with an established connection
    async fn run_connection(
        url: &str,
        conn_opt: &mut Option<Conn>,
        state: &mut AsyncState<KeychainId>,
        client_recv: &mut AsyncReceiver<KeychainId>,
        update_sender: &mut mpsc::UnboundedSender<Update<KeychainId>>,
        status_sink: &dyn Sink<ChainStatus>,
    ) {
        let conn = match conn_opt {
            Some(conn) => conn,
            None => {
                tracing::error!("run_connection called without an established connection");
                return;
            }
        };
        
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
        // TODO: This is not necessarily a closed connection.
        match conn_result {
            Ok(_) => tracing::info!(url, "Connection service stopped gracefully"),
            Err(err) => tracing::warn!(url, ?err, "Connection service stopped"),
        };
        status_sink.send(ChainStatus::new(&url, ChainStatusState::Disconnected));
        tokio::time::sleep(Self::RECONNECT_DELAY).await;
    }

    /// Handle a single message.
    ///
    /// Note that this requires a tokio runtime with networking as we need to handle
    /// connect/reconnect logic.
    ///
    /// * `sink_stage` stages changes to the `ChainStatus` sink which updates the Flutter UI about
    ///   connection status.
    /// * `conn_stage` stages changes to the connection.
    async fn handle_msg(
        genesis_hash: BlockHash,
        msg: Message,
        sink_stage: &mut Option<Box<dyn Sink<ChainStatus>>>,
        conn_stage: &mut TargetServer,
        trusted_certificates: &mut Persisted<TrustedCertificates>,
        db: &Arc<sync::Mutex<rusqlite::Connection>>,
    ) {
        match msg {
            Message::ChangeUrlReq(ReqAndResponse { request, response }) => {
                tracing::info!(
                    msg = "ChangeUrlReq",
                    url = request.url,
                    is_backup = request.is_backup,
                );

                match Conn::new(genesis_hash, &request.url, Self::CONNECT_TIMEOUT, trusted_certificates, db.clone()).await {
                    Ok(conn) => {
                        if request.is_backup {
                            conn_stage.backup_url = request.url.clone();
                            conn_stage.backup_conn = Some(conn);
                        } else {
                            conn_stage.url = request.url.clone();
                            conn_stage.conn = Some(conn);
                        }
                        let _ = response.send(Ok(ConnectionResult::Success));
                    }
                    Err(err) => {
                        match err {
                            TofuError::NotTrusted(cert) => {
                                tracing::info!("Certificate not trusted for {}: {}", request.url, cert.fingerprint);
                                let _ = response.send(Ok(ConnectionResult::CertificatePromptNeeded(cert)));
                            }
                            TofuError::Other(e) => {
                                tracing::error!("Failed to connect to {}: {}", request.url, e);
                                let error_msg = improve_error_message(&e.to_string());
                                let _ = response.send(Ok(ConnectionResult::Failed(error_msg)));
                            }
                        }
                    }
                };
            }
            Message::SetStatusSink(sink) => {
                tracing::info!(msg = "SetStatusSink");
                *sink_stage = Some(sink);
            }
            Message::Reconnect => {
                tracing::info!(msg = "Reconnect");
            }
            Message::TrustCertificate { server_url, certificate_der } => {
                tracing::info!(msg = "TrustCertificate", server_url = server_url);
                let cert = certificate_der.into();
                
                // Extract hostname from URL (remove protocol and port)
                let hostname = match server_url.split_once("://") {
                    Some((_, addr)) => {
                        // Remove port if present
                        addr.split_once(':')
                            .map(|(host, _)| host)
                            .unwrap_or(addr)
                            .to_string()
                    },
                    None => {
                        // No protocol, remove port if present
                        server_url.split_once(':')
                            .map(|(host, _)| host)
                            .unwrap_or(&server_url)
                            .to_string()
                    },
                };
                
                tracing::info!("Storing certificate for hostname: {}", hostname);
                
                // Use Persisted's mutation methods to update and persist
                let mut db_guard = db.lock().unwrap();
                if let Err(e) = trusted_certificates.mutate2(&mut *db_guard, |trusted_certs, update| {
                    trusted_certs.add_certificate(cert, hostname, update);
                    Ok(())
                }) {
                    tracing::error!("Failed to trust certificate: {:?}", e);
                }
            }
        }
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

#[derive(Clone)]
pub struct ChainStatus {
    pub electrum_url: String,
    pub state: ChainStatusState,
}

impl ChainStatus {
    pub fn new(url: &str, state: ChainStatusState) -> Self {
        Self {
            electrum_url: url.to_string(),
            state,
        }
    }
}

#[derive(Clone, Copy)]
pub enum ChainStatusState {
    Connected,
    Disconnected,
    Connecting,
}

