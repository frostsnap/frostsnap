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
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{event, Level};

use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;
use crate::Sink;

use super::{
    descriptor_for_account_keychain,
    handler_state::HandlerState,
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
    SetUrls {
        primary: String,
        backup: String,
    },
    SetEnabled(ElectrumEnabled),
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
            Message::SetUrls { .. } => write!(f, "Message::SetUrls"),
            Message::SetEnabled(enabled) => write!(f, "Message::SetEnabled({:?})", enabled),
        }
    }
}

/// Opaque API to the chain
#[derive(Clone)]
pub struct ChainClient {
    req_sender: mpsc::UnboundedSender<Message>,
    client: KeychainClient,
    connection_requested: Arc<AtomicBool>,
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
                connection_requested: Arc::new(AtomicBool::new(false)),
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
        self.req_sender
            .unbounded_send(Message::SetUrls { primary, backup })
            .unwrap();
    }

    pub fn set_enabled(&self, enabled: ElectrumEnabled) {
        self.req_sender
            .unbounded_send(Message::SetEnabled(enabled))
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
        bitcoin::Network::Signet => "ssl://mempool.space:60602",
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

        let mut handler = HandlerState::new(
            self.genesis_hash,
            url,
            backup_url,
            self.trusted_certificates,
            self.db,
        );

        rt.block_on({
            let mut electrum_state = AsyncState::<KeychainId>::new(
                ReqCoord::new(rand::random::<u32>()),
                self.cache,
                DerivedSpkTracker::new(lookahead),
                chain_tip,
            );

            async move {
                loop {
                    // Wait until we should connect (started + enabled)
                    while !handler.should_connect() {
                        match self.req_recv.next().await {
                            Some(msg) => {
                                handler.handle_msg(msg).await;
                            }
                            None => return,
                        }
                    }

                    let mut conn = match handler.get_connection().await {
                        Some(conn) => conn,
                        None => {
                            tokio::time::sleep(HandlerState::RECONNECT_DELAY).await;
                            continue;
                        }
                    };

                    {
                        let conn_fut = Self::run_connection(
                            &mut conn,
                            &mut electrum_state,
                            &mut self.client_recv,
                            &mut update_sender,
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
                                        tracing::trace!("Received pong from server");
                                    },
                                    _ = req_timeout_fut => {
                                        return anyhow!("Timeout waiting for pong");
                                    },
                                }
                            }
                        }.fuse();
                        pin_mut!(conn_fut);
                        pin_mut!(ping_fut);

                        // Handle messages until connection fails or we get disabled
                        loop {
                            select_biased! {
                                msg_opt = self.req_recv.next() => {
                                    let msg = match msg_opt {
                                        Some(msg) => msg,
                                        None => return,
                                    };
                                    tracing::info!(msg = msg.to_string(), "Handling message");
                                    let should_reconnect = handler.handle_msg(msg).await;

                                    if !handler.should_connect() {
                                        tracing::info!("Electrum disabled");
                                        break;
                                    }

                                    if should_reconnect {
                                        tracing::info!("Breaking connection loop due to reconnect request");
                                        break;
                                    }
                                }
                                err = ping_fut => {
                                    tracing::error!(error = err.to_string(), "Failed to keep connection alive");
                                    break;
                                },
                                _ = conn_fut => {
                                    tracing::debug!("Connection service stopped");
                                    break;
                                },
                            }
                        }
                    }

                    // Shutdown the connection
                    handler.set_disconnected();
                    let shutdown_result = match conn {
                        Conn::Tcp((rh, wh)) => rh.unsplit(wh).shutdown().await,
                        Conn::Ssl((rh, wh)) => rh.unsplit(wh).shutdown().await,
                    };
                    tracing::info!(result = ?shutdown_result, "Connection shutdown");
                }
            }
        });
    }

    /// Run the sync loop with an established connection
    async fn run_connection(
        conn: &mut Conn,
        state: &mut AsyncState<KeychainId>,
        client_recv: &mut AsyncReceiver<KeychainId>,
        update_sender: &mut mpsc::UnboundedSender<Update<KeychainId>>,
    ) {
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
            Ok(_) => tracing::info!("Connection service stopped gracefully"),
            Err(err) => tracing::warn!(?err, "Connection service stopped"),
        };
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
    /// No connection has been attempted yet
    Idle,
    Connecting,
    Connected,
    Disconnected,
}
