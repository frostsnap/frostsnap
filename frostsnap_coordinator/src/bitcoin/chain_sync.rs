//! We keep chain at arms length from the rest of the code by only communicating through mpsc channels.
use anyhow::{anyhow, Result};
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{
    bitcoin::{self, BlockHash},
    spk_client::{self, FullScanResponse},
    CheckPoint, ConfirmationBlockTime,
};
use bdk_electrum_streaming::{
    electrum_streaming_client::{request, Request},
    run_async, AsyncReceiver, AsyncState, Cache, DerivedSpkTracker, ReqCoord, Update,
};
use frostsnap_core::MasterAppkey;
use futures::pin_mut;
use futures::{
    channel::{mpsc, oneshot},
    executor::{block_on, block_on_stream},
    select,
    stream::FuturesUnordered,
    FutureExt, StreamExt,
};
use rustls::pki_types;
use std::{
    collections::BTreeMap,
    fmt::Debug,
    ops::Deref,
    sync::{self, Arc},
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_rustls::{
    client::TlsStream,
    rustls::{self},
    TlsConnector,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{event, Level};

use crate::Sink;

use super::{
    descriptor_for_account_keychain,
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

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub const SUPPORTED_NETWORKS: [bitcoin::Network; 4] = {
    use bitcoin::Network::*;
    [Bitcoin, Signet, Testnet, Regtest]
};

pub type SyncResponse = spk_client::SyncResponse<ConfirmationBlockTime>;
pub type KeychainClient = bdk_electrum_streaming::AsyncClient<KeychainId>;
pub type KeychainClientReceiver = bdk_electrum_streaming::AsyncReceiver<KeychainId>;

/// The messages the client can send to the backend
pub enum Message {
    ChangeUrlReq(ReqAndResponse<TargetServerReq, Result<()>>),
    SetStatusSink(Box<dyn Sink<ChainStatus>>),
    Reconnect,
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::ChangeUrlReq(_) => write!(f, "Message::ChangeUrlReq"),
            Message::SetStatusSink(_) => write!(f, "Message::SetStatusSink"),
            Message::Reconnect => write!(f, "Message::Reconnect"),
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
    pub fn new(genesis_hash: BlockHash) -> (Self, ConnectionHandler) {
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
            },
        )
    }

    pub fn check_and_set_electrum_server_url(&self, url: String, is_backup: bool) -> Result<()> {
        let (req, response) = ReqAndResponse::new(TargetServerReq { url, is_backup });
        self.req_sender
            .unbounded_send(Message::ChangeUrlReq(req))
            .unwrap();
        block_on(response)??;
        Ok(())
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
        event!(
            Level::DEBUG,
            "WE ARE BROADCASTING {}",
            transaction.compute_txid()
        );
        block_on(self.client.send_request(request::BroadcastTx(transaction)))
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
                        .map(move |result| {
                            result
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
}

pub const fn default_electrum_server(network: bitcoin::Network) -> &'static str {
    match network {
        bitcoin::Network::Bitcoin => "tcp://electrum.frostsn.app:50001",
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
}

#[derive(Debug, Clone)]
pub struct UpdateIter {
    update_recv: Arc<futures::lock::Mutex<mpsc::UnboundedReceiver<FullScanResponse<KeychainId>>>>,
}

impl Iterator for UpdateIter {
    type Item = FullScanResponse<KeychainId>;

    fn next(&mut self) -> Option<Self::Item> {
        block_on(async { self.update_recv.lock().await.next().await })
    }
}

impl ConnectionHandler {
    // TODO: Do something with this.
    const _PING_DELAY: Duration = Duration::from_secs(5);
    const RECONNECT_DELAY: Duration = Duration::from_millis(1000);

    pub fn run<SW, F>(mut self, url: String, backup_url: String, super_wallet: SW, update_action: F)
    where
        SW: Deref<Target = sync::Mutex<CoordSuperWallet>> + Clone + Send + 'static,
        F: FnMut(MasterAppkey, Vec<crate::bitcoin::wallet::Transaction>) + Send + 'static,
    {
        tracing::debug!("Running ConnectionHandler");

        let lookahead: u32;
        let chain_tip: CheckPoint;
        {
            let super_wallet = super_wallet.lock().expect("must lock");
            lookahead = super_wallet.lookahead();
            chain_tip = super_wallet.chain_tip();
            self.cache.txs.extend(super_wallet.tx_cache());
            self.cache.anchors.extend(super_wallet.anchor_cache());
        }

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
                // Make sure we have a status sync before handling connection.
                while let Some(msg) = self.req_recv.next().await {
                    let is_status_sink = msg.is_status_sink();
                    Self::handle_msg(self.genesis_hash, msg, &mut sink_stage, &mut conn_stage, &self.client, false).await;
                    if is_status_sink {
                        break;
                    }
                }

                // Reconnection loop.
                loop {
                    if let Some(new_conn) = conn_stage.take_conn() {
                        conn_opt = Some(new_conn);
                    }
                    if let Some(new_sink) = sink_stage.take() {
                        sink = new_sink;
                    }

                    let conn_fut = Self::try_connect_and_run(
                        self.genesis_hash,
                        conn_stage.url.clone(),
                        conn_stage.backup_url.clone(),
                        &mut conn_opt,
                        &mut state,
                        &mut self.client_recv,
                        &mut update_sender,
                        &*sink,
                    )
                    .fuse();

                    {
                        pin_mut!(conn_fut);
                        loop {
                            select! {
                                _ = conn_fut => break,
                                msg_opt = self.req_recv.next() => match msg_opt {
                                    Some(msg) => {
                                        tracing::info!(msg = msg.to_string(), "Got message");
                                        Self::handle_msg(self.genesis_hash, msg, &mut sink_stage, &mut conn_stage, &self.client, true).await
                                    },
                                    None => return,
                                }
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

    async fn try_connect_and_run(
        genesis_hash: BlockHash,
        url: String,
        backup_url: String,
        conn_opt: &mut Option<Conn>,
        state: &mut AsyncState<KeychainId>,
        client_recv: &mut AsyncReceiver<KeychainId>,
        update_sender: &mut mpsc::UnboundedSender<Update<KeychainId>>,
        status_sink: &dyn Sink<ChainStatus>,
    ) {
        let conn = match conn_opt {
            Some(conn) => {
                status_sink.send(ChainStatus::new(&url, ChainStatusState::Connected));
                tracing::info!("Using previously established connection with {}.", url);
                conn
            }
            conn_opt => {
                for url in [url.as_str(), backup_url.as_str()] {
                    status_sink.send(ChainStatus::new(&url, ChainStatusState::Connecting));
                    tracing::info!("No existing connection. Connecting to {}.", url);

                    match Conn::with_timeout(genesis_hash, &url, CONNECT_TIMEOUT).await {
                        Ok(conn) => {
                            status_sink.send(ChainStatus::new(&url, ChainStatusState::Connected));
                            tracing::info!("Connection established with {}.", url);
                            *conn_opt = Some(conn);
                            break;
                        }
                        Err(err) => {
                            status_sink
                                .send(ChainStatus::new(&url, ChainStatusState::Disconnected));
                            tracing::error!(err = err.to_string(), url, "failed to connect",);
                        }
                    }
                }
                match conn_opt {
                    Some(conn) => conn,
                    None => {
                        tracing::error!(
                            reconnecting_in_secs = Self::RECONNECT_DELAY.as_secs_f32(),
                            "Failed to connect to all Electrum servers"
                        );
                        tokio::time::sleep(Self::RECONNECT_DELAY).await;
                        return;
                    }
                }
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
        client: &KeychainClient,
        with_stop: bool,
    ) {
        match msg {
            Message::ChangeUrlReq(ReqAndResponse { request, response }) => {
                tracing::info!(
                    msg = "ChangeUrlReq",
                    url = request.url,
                    is_backup = request.is_backup,
                );

                match Conn::with_timeout(genesis_hash, &request.url, CONNECT_TIMEOUT).await {
                    Ok(conn) => {
                        if request.is_backup {
                            conn_stage.backup_url = request.url.clone();
                            conn_stage.backup_conn = Some(conn);
                        } else {
                            conn_stage.url = request.url.clone();
                            conn_stage.conn = Some(conn);
                        }
                        let _ = response.send(Ok(()));
                    }
                    Err(err) => {
                        let _ = response.send(Err(err));
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
        }
        if with_stop {
            let _ = client.stop().await;
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

/// Check that the connection actually connects to an Electrum server and the server is on the right
/// network.
async fn check_conn<R, W>(rh: R, mut wh: W, genesis_hash: BlockHash) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use bdk_electrum_streaming::electrum_streaming_client as client;
    use client::request;
    use client::RawNotificationOrResponse;
    use futures::io::BufReader;

    let req_id = rand::random::<u32>();
    let req = client::RawRequest::from_request(req_id, request::Header { height: 0 });
    client::io::tokio_write(&mut wh, req).await?;

    let mut read_stream = client::io::ReadStreamer::new(BufReader::new(rh.compat()));
    let raw_incoming = read_stream
        .next()
        .await
        .ok_or(anyhow!("failed to get response from server"))??;

    let raw_resp = match raw_incoming {
        RawNotificationOrResponse::Notification(_) => {
            return Err(anyhow!("Received unexpected notification from server"))
        }
        RawNotificationOrResponse::Response(raw_resp) => raw_resp,
    };

    if raw_resp.id != req_id {
        return Err(anyhow!(
            "Response id {} does not match request id {}",
            raw_resp.id,
            req_id
        ));
    }

    let raw_val = raw_resp
        .result
        .map_err(|err| anyhow!("Server responded with error: {err}"))?;

    let resp: <request::Header as Request>::Response = client::serde_json::from_value(raw_val)?;

    if genesis_hash != resp.header.block_hash() {
        return Err(anyhow!("Electrum server is on a different network"));
    }

    Ok(())
}

type SplitConn<T> = (tokio::io::ReadHalf<T>, tokio::io::WriteHalf<T>);

enum Conn {
    Tcp(SplitConn<tokio::net::TcpStream>),
    Ssl(SplitConn<TlsStream<tokio::net::TcpStream>>),
}

impl Conn {
    async fn new(genesis_hash: BlockHash, url: &str) -> Result<Self> {
        let (is_ssl, socket_addr) = match url.split_once("://") {
            Some(("ssl", socket_addr)) => (true, socket_addr.to_owned()),
            Some(("tcp", socket_addr)) => (false, socket_addr.to_owned()),
            Some((unknown_scheme, _)) => {
                return Err(anyhow!("unknown url scheme '{unknown_scheme}'"));
            }
            None => (false, url.to_owned()),
        };
        tracing::info!(url, "Connecting");
        if is_ssl {
            let mut root_store = rustls::RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();
            let host = socket_addr
                .clone()
                .split_once(":")
                .map(|(host, _)| host.to_string())
                .unwrap_or(socket_addr.clone());
            let dnsname = pki_types::ServerName::try_from(host)?;

            let sock = tokio::net::TcpStream::connect(socket_addr).await?;

            let connector = TlsConnector::from(Arc::new(config));
            let stream = connector.connect(dnsname, sock).await?;
            let (mut rh, mut wh) = tokio::io::split(stream);
            check_conn(&mut rh, &mut wh, genesis_hash)
                .await
                .inspect_err(|e| tracing::error!(e = e.to_string()))?;
            anyhow::Ok(Conn::Ssl((rh, wh)))
        } else {
            let sock = tokio::net::TcpStream::connect(socket_addr).await?;
            let (mut rh, mut wh) = tokio::io::split(sock);
            check_conn(&mut rh, &mut wh, genesis_hash)
                .await
                .inspect_err(|e| tracing::error!(e = e.to_string()))?;
            anyhow::Ok(Conn::Tcp((rh, wh)))
        }
    }

    async fn with_timeout(genesis_hash: BlockHash, url: &str, timeout: Duration) -> Result<Self> {
        let connect_fut = Self::new(genesis_hash, url).fuse();
        pin_mut!(connect_fut);

        let timeout_fut = tokio::time::sleep(timeout).fuse();
        pin_mut!(timeout_fut);

        select! {
            conn_res = connect_fut => conn_res,
            _ = timeout_fut => Err(anyhow!("")),
        }
    }
}

struct TargetServer {
    url: String,
    backup_url: String,
    conn: Option<Conn>,
    backup_conn: Option<Conn>,
}

impl TargetServer {
    fn take_conn(&mut self) -> Option<Conn> {
        self.conn.take().or_else(|| self.backup_conn.take())
    }
}

#[derive(Debug, Clone)]
pub struct TargetServerReq {
    pub url: String,
    pub is_backup: bool,
}
