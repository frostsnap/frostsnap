//! We keep chain at arms length from the rest of the code by only communicating through mpsc channels.
use anyhow::Result;
use async_std::net::TcpStream;
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{
    bitcoin::{self, Transaction},
    miniscript::{Descriptor, DescriptorPublicKey},
    spk_client::{self, FullScanResult},
    ConfirmationBlockTime,
};
use bdk_electrum_c::{CmdSender, Emitter};
use frostsnap_core::MasterAppkey;
use futures::{
    channel::{mpsc, oneshot},
    executor::block_on,
    future::Either,
    StreamExt,
};
use futures_rustls::{
    client::TlsStream, pki_types::ServerName, rustls::RootCertStore, TlsConnector,
};
use std::{
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    time::Duration,
};

use crate::Sink;

use super::{
    descriptor_for_account_keychain,
    wallet::{FrostsnapWallet, KeychainId},
};

pub struct ReqAndResponse<I, O> {
    request: I,
    response: OneshotSender<O>,
}

impl<I: Send, O: Send> ReqAndResponse<I, O> {
    pub fn new(request: I) -> (Self, Receiver<O>) {
        let (sender, receiver) = sync_channel(1);
        (
            Self {
                request,
                response: OneshotSender { inner: sender },
            },
            receiver,
        )
    }

    pub fn into_tuple(self) -> (I, OneshotSender<O>) {
        (self.request, self.response)
    }
}

pub struct OneshotSender<O> {
    inner: SyncSender<O>,
}

impl<O: Send> OneshotSender<O> {
    pub fn send(self, value: O) {
        let _ = self.inner.send(value);
    }
}

pub const SUPPORTED_NETWORKS: [bitcoin::Network; 4] = {
    use bitcoin::Network::*;
    [Bitcoin, Signet, Testnet, Regtest]
};

pub type SyncResponse = spk_client::SyncResult<ConfirmationBlockTime>;

/// The messages the client can send to the backend
pub enum Message {
    ChangeUrlReq(ReqAndResponse<String, Result<()>>),
    MonitorDescriptor(KeychainId, Descriptor<DescriptorPublicKey>),
    BroadcastReq(ReqAndResponse<Transaction, Result<()>>),
    SetStatusSink(Box<dyn Sink<ChainStatus>>),
    Reconnect,
}

/// Opaque API to the chain
#[derive(Clone)]
pub struct ChainClient {
    req_sender: SyncSender<Message>,
}

impl ChainClient {
    pub fn new() -> (Self, ConnectionHandler) {
        let (req_sender, req_recv) = sync_channel(1);
        (Self { req_sender }, ConnectionHandler { req_recv })
    }

    pub fn check_and_set_electrum_server_url(&self, url: String) -> Result<()> {
        let (req, response) = ReqAndResponse::new(url);
        self.req_sender.send(Message::ChangeUrlReq(req)).unwrap();
        response.recv()?
    }

    pub fn monitor_keychain(&self, keychain: KeychainId) {
        let descriptor = descriptor_for_account_keychain(
            keychain,
            // this does not matter
            bitcoin::NetworkKind::Main,
        );
        self.req_sender
            .send(Message::MonitorDescriptor(keychain, descriptor))
            .unwrap();
    }

    pub fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<()> {
        let (req, response) = ReqAndResponse::new(transaction);
        self.req_sender.send(Message::BroadcastReq(req)).unwrap();
        response.recv()?
    }

    pub fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        self.req_sender.send(Message::SetStatusSink(sink)).unwrap();
    }

    pub fn reconnect(&self) {
        self.req_sender.send(Message::Reconnect).unwrap();
    }
}

pub const fn default_electrum_server(network: bitcoin::Network) -> &'static str {
    match network {
        bitcoin::Network::Bitcoin => "ssl://electrum.blockstream.info:50002",
        // we're using the tcp:// version since ssl ain't working for some reason
        bitcoin::Network::Testnet => "tcp://electrum.blockstream.info:60001",
        bitcoin::Network::Regtest => "tcp://localhost:60401",
        bitcoin::Network::Signet => "tcp://signet-electrumx.wakiyamap.dev:50001",
        _ => panic!("Unknown network"),
    }
}

pub struct ConnectionHandler {
    req_recv: Receiver<Message>,
}

#[derive(Debug, Clone)]
pub struct UpdateIter {
    update_recv: Arc<futures::lock::Mutex<mpsc::UnboundedReceiver<FullScanResult<KeychainId>>>>,
}

impl Iterator for UpdateIter {
    type Item = FullScanResult<KeychainId>;

    fn next(&mut self) -> Option<Self::Item> {
        block_on(async { self.update_recv.lock().await.next().await })
    }
}

impl ConnectionHandler {
    pub fn run(
        self,
        url: String,
        wallet: Arc<std::sync::Mutex<FrostsnapWallet>>,
        mut update_action: impl FnMut(MasterAppkey, Vec<crate::bitcoin::wallet::Transaction>)
            + Send
            + 'static,
    ) {
        let (emitter, cmd_sender, mut update_recv) = {
            let wallet = wallet.lock().unwrap();
            let (mut emitter, cmd_sender, update_recv) =
                Emitter::<KeychainId>::new(wallet.chain_tip(), wallet.lookahead());
            emitter.insert_txs(wallet.tx_cache());
            (emitter, cmd_sender, update_recv)
        };
        let url = Arc::new(std::sync::Mutex::new(url));
        let status_sink = Arc::new(std::sync::Mutex::<Box<dyn Sink<ChainStatus>>>::new(
            Box::new(()),
        ));
        let (start_conn_signal, start_conn) = oneshot::channel::<()>();
        std::thread::spawn({
            let url = url.clone();
            let status_sink = status_sink.clone();
            move || Self::handle_connection(start_conn, url, status_sink, emitter)
        });
        std::thread::spawn({
            let url = url.clone();
            let req_recv = self.req_recv;
            let status_sink = status_sink.clone();
            move || Self::handle_requests(start_conn_signal, url, status_sink, req_recv, cmd_sender)
        });
        std::thread::spawn({
            let url = url.clone();
            let status_sink = status_sink.clone();
            move || {
                block_on(async move {
                    while let Some(update) = update_recv.next().await {
                        status_sink.lock().unwrap().send(ChainStatus {
                            electrum_url: url.lock().unwrap().clone(),
                            state: ChainStatusState::Connected,
                        });
                        let master_appkeys = update
                            .last_active_indices
                            .keys()
                            .map(|(k, _)| *k)
                            .collect::<Vec<_>>();
                        let mut wallet = wallet.lock().unwrap();
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
                                update_action(master_appkey, txs);
                            }
                        }
                    }
                })
            }
        });
    }

    fn handle_connection(
        start_conn: oneshot::Receiver<()>,
        url: Arc<std::sync::Mutex<String>>,
        status_sink: Arc<std::sync::Mutex<Box<dyn Sink<ChainStatus>>>>,
        mut emitter: Emitter<KeychainId>,
    ) {
        const PING_DELAY: Duration = Duration::from_secs(5);
        const RECONNECT_DELAY: Duration = Duration::from_millis(1000);
        enum Conn {
            Tcp(TcpStream),
            Ssl(TlsStream<TcpStream>),
        }
        let _ = block_on(start_conn);
        loop {
            let url = url.lock().unwrap().clone();
            let (is_ssl, socket_addr) = match url.split_once("://") {
                Some(("ssl", socket_addr)) => (true, socket_addr.to_owned()),
                Some(("tcp", socket_addr)) => (false, socket_addr.to_owned()),
                Some((unknown_scheme, _)) => {
                    // TODO: exponential backoff.
                    tracing::error!("Unknown URI scheme '{}'", unknown_scheme);
                    continue;
                }
                None => (false, url.clone()),
            };
            tracing::info!("Connecting to {} ...", url);
            status_sink.lock().unwrap().send(ChainStatus {
                electrum_url: url.clone(),
                state: ChainStatusState::Connecting,
            });
            let conn_res = block_on(async {
                if is_ssl {
                    let mut root_store = RootCertStore::empty();
                    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                    let config = futures_rustls::rustls::ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                    let connector = TlsConnector::from(Arc::new(config));
                    let host = socket_addr
                        .clone()
                        .split_once(":")
                        .map(|(host, _)| host.to_string())
                        .unwrap_or(socket_addr.clone());
                    let dnsname = ServerName::try_from(host)?;
                    let conn = async_std::net::TcpStream::connect(socket_addr).await?;
                    let conn = connector.connect(dnsname, conn).await?;
                    anyhow::Ok(Conn::Ssl(conn))
                } else {
                    let conn = async_std::net::TcpStream::connect(socket_addr).await?;
                    anyhow::Ok(Conn::Tcp(conn))
                }
            });
            match conn_res {
                Ok(conn) => {
                    tracing::info!("Connected to {}.", url);
                    let close_res = block_on(match conn {
                        Conn::Tcp(conn) => Either::Left(emitter.run(PING_DELAY, conn)),
                        Conn::Ssl(conn) => Either::Right(emitter.run(PING_DELAY, conn)),
                    });
                    tracing::warn!("Connection {} closed: {:?}", url, close_res);
                }
                Err(err) => {
                    tracing::error!("Connection {} failed to open: {}", url, err);
                }
            };
            status_sink.lock().unwrap().send(ChainStatus {
                electrum_url: url,
                state: ChainStatusState::Disconnected,
            });
            std::thread::sleep(RECONNECT_DELAY);
        }
    }

    fn handle_requests(
        start_conn_signal: oneshot::Sender<()>,
        url: Arc<std::sync::Mutex<String>>,
        status_sink: Arc<std::sync::Mutex<Box<dyn Sink<ChainStatus>>>>,
        req_recv: Receiver<Message>,
        cmd_sender: CmdSender<KeychainId>,
    ) {
        let mut start_conn_signal = Some(start_conn_signal);
        loop {
            let msg = req_recv.recv().expect("sender never disappears");
            match msg {
                Message::ChangeUrlReq(ReqAndResponse { request, response }) => {
                    // TODO: Send connection status back somewhere else.
                    *url.lock().unwrap() = request;
                    block_on(cmd_sender.close()).expect("conn handler thread failed");
                    response.send(Ok(()));
                }
                Message::MonitorDescriptor(keychain, descriptor) => {
                    // TODO: 10 is just an arbitary number. We need to persist last-derived-index
                    // somewhere to be safe.
                    cmd_sender
                        .insert_descriptor(keychain, descriptor, 10)
                        .expect("must insert descriptor");
                }
                Message::BroadcastReq(ReqAndResponse { request, response }) => {
                    // TODO: Change `broadcast_tx` to not wait for response, but send event when
                    // broadcasted.
                    let cmd_sender = cmd_sender.clone();
                    std::thread::spawn(move || {
                        let res = block_on(cmd_sender.broadcast_tx(request));
                        tracing::info!("Tx broadcast response: {:?}", res);
                    });
                    response.send(Ok(()));
                }
                Message::SetStatusSink(sink) => {
                    *status_sink.lock().unwrap() = sink;
                }
                Message::Reconnect => {
                    block_on(cmd_sender.close()).expect("conn handler thread failed");
                }
            }
            if let Some(signal) = start_conn_signal.take() {
                let _ = signal.send(());
            }
        }
    }
}

#[derive(Clone)]
pub struct ChainStatus {
    pub electrum_url: String,
    pub state: ChainStatusState,
}

#[derive(Clone, Copy)]
pub enum ChainStatusState {
    Connected,
    Disconnected,
    Connecting,
}
