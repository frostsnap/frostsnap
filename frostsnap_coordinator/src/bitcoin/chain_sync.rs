//! We keep chain at arms length from the rest of the code by only communicating through mpsc channels.
use anyhow::{anyhow, Result};
use async_std::net::TcpStream;
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{
    bitcoin::{self, Transaction},
    miniscript::{Descriptor, DescriptorPublicKey},
    spk_client::{self, FullScanResult},
    ConfirmationBlockTime,
};
use bdk_electrum_c::Emitter;
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
use tracing::{event, Level};

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
        let wallet_ = wallet.lock().unwrap();
        let lookahead = wallet_.lookahead();
        let (mut emitter, cmd_sender, mut update_recv) =
            Emitter::<KeychainId>::new(wallet_.chain_tip(), lookahead);
        emitter.insert_txs(wallet_.tx_cache());
        drop(wallet_);
        let target_server = Arc::new(std::sync::Mutex::new(TargetServer { url, conn: None }));
        let status_sink = Arc::new(std::sync::Mutex::<Box<dyn Sink<ChainStatus>>>::new(
            Box::new(()),
        ));
        let (start_conn_signal, start_conn) = oneshot::channel::<()>();

        {
            const PING_DELAY: Duration = Duration::from_secs(5);
            const RECONNECT_DELAY: Duration = Duration::from_millis(1000);
            let target_server = Arc::clone(&target_server);
            let status_sink = status_sink.clone();
            // Run thread which handles the electrum connection.
            std::thread::spawn(move || {
                // Only start connection after getting first `Message` request.
                let _ = block_on(start_conn);
                loop {
                    let mut target_server = target_server.lock().unwrap();
                    let url = target_server.url.clone();
                    let span = tracing::span!(Level::INFO, "connection", url = url);
                    let _enter = span.enter();
                    status_sink.lock().unwrap().send(ChainStatus {
                        electrum_url: url.clone(),
                        state: ChainStatusState::Connecting,
                    });

                    let conn = target_server.conn.take();
                    drop(target_server);
                    let conn_res = match conn {
                        Some(conn) => {
                            event!(Level::INFO, "Using newly establised connection");
                            Ok(conn)
                        }
                        None => {
                            event!(Level::INFO, "No existing connection. Connecting.");
                            connect(&url)
                        }
                    };

                    status_sink.lock().unwrap().send(ChainStatus {
                        electrum_url: url.clone(),
                        state: ChainStatusState::Connected,
                    });

                    match conn_res {
                        Ok(conn) => {
                            let close_res = block_on(match conn {
                                Conn::Tcp(conn) => Either::Left(emitter.run(PING_DELAY, conn)),
                                Conn::Ssl(conn) => Either::Right(emitter.run(PING_DELAY, conn)),
                            });
                            match close_res {
                                Ok(_) => event!(Level::INFO, "connection closed gracefully"),
                                Err(e) => event!(
                                    Level::WARN,
                                    error = e.to_string(),
                                    "connection closed with error"
                                ),
                            }
                        }
                        Err(err) => {
                            tracing::error!("Connection {} failed to open: {}", url.clone(), err);
                        }
                    };
                    status_sink.lock().unwrap().send(ChainStatus {
                        electrum_url: url.clone(),
                        state: ChainStatusState::Disconnected,
                    });
                    std::thread::sleep(RECONNECT_DELAY);
                }
            });
        }

        {
            let target_server = Arc::clone(&target_server);
            let req_recv = self.req_recv;
            let status_sink = status_sink.clone();
            let mut start_conn_signal = Some(start_conn_signal);

            // Run thread which responds to `Message` requests.
            std::thread::spawn(move || loop {
                let msg = req_recv.recv().expect("sender never disappears");
                // as soon as we receive the first request request for this network we tell the
                // connection handler to start.
                if let Some(signal) = start_conn_signal.take() {
                    let _ = signal.send(());
                }
                match msg {
                    Message::ChangeUrlReq(ReqAndResponse { request, response }) => {
                        let url = request;
                        match connect(&url) {
                            Ok(conn) => {
                                *target_server.lock().unwrap() = TargetServer {
                                    url,
                                    conn: Some(conn),
                                };
                                block_on(cmd_sender.close()).expect("conn handler thread failed");
                                response.send(Ok(()));
                            }
                            Err(e) => response.send(Err(e)),
                        }
                    }
                    Message::MonitorDescriptor(keychain, descriptor) => {
                        cmd_sender
                            .insert_descriptor(keychain, descriptor, lookahead)
                            .expect("must insert descriptor");
                    }
                    Message::BroadcastReq(ReqAndResponse { request, response }) => {
                        let cmd_sender = cmd_sender.clone();
                        std::thread::spawn(move || {
                            let res = block_on(cmd_sender.broadcast_tx(request));
                            tracing::info!("Tx broadcast response: {:?}", res);
                            response.send(res);
                        });
                    }
                    Message::SetStatusSink(sink) => {
                        *status_sink.lock().unwrap() = sink;
                    }
                    Message::Reconnect => {
                        block_on(cmd_sender.close()).expect("conn handler thread failed");
                    }
                }
            });
        }

        // Run thread which responds to wallet updates emitted from the Electrum chain source.
        std::thread::spawn(move || {
            block_on(async move {
                while let Some(update) = update_recv.next().await {
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
        });
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

fn connect(url: &str) -> Result<Conn> {
    let (is_ssl, socket_addr) = match url.split_once("://") {
        Some(("ssl", socket_addr)) => (true, socket_addr.to_owned()),
        Some(("tcp", socket_addr)) => (false, socket_addr.to_owned()),
        Some((unknown_scheme, _)) => {
            return Err(anyhow!("unknown url scheme '{unknown_scheme}'"));
        }
        None => (false, url.to_owned()),
    };
    tracing::info!("Connecting to {} ...", url);
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

    conn_res
}

enum Conn {
    Tcp(TcpStream),
    Ssl(TlsStream<TcpStream>),
}

struct TargetServer {
    url: String,
    conn: Option<Conn>,
}
