//! We keep chain at arms length from the rest of the code by only communicating through mpsc channels.
use anyhow::{Context, Result};
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{
    bitcoin::{self, Transaction},
    spk_client, ConfirmationBlockTime,
};
use bdk_electrum::{electrum_client, BdkElectrumClient};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use tracing::{event, Level};

use crate::Sink;

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
    SyncReq(ReqAndResponse<SyncRequest, Result<SyncResponse>>),
    ChangeUrlReq(ReqAndResponse<String, Result<()>>),
    BroadcastReq(ReqAndResponse<Transaction, Result<()>>),
    SetStatusSink(Box<dyn Sink<ChainStatus>>),
}

/// Opaque API to the chain
#[derive(Clone)]
pub struct ChainClient {
    sync_request_sender: SyncSender<Message>,
}

impl ChainClient {
    pub fn sync(&self, sync_request: SyncRequest) -> Result<SyncResponse> {
        let (req, response) = ReqAndResponse::new(sync_request);
        self.sync_request_sender
            .send(Message::SyncReq(req))
            .unwrap();
        response.recv()?
    }

    pub fn check_and_set_electrum_server_url(&self, url: String) -> Result<()> {
        let (req, response) = ReqAndResponse::new(url);
        self.sync_request_sender
            .send(Message::ChangeUrlReq(req))
            .unwrap();
        response.recv()?
    }

    pub fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<()> {
        let (req, response) = ReqAndResponse::new(transaction);
        self.sync_request_sender
            .send(Message::BroadcastReq(req))
            .unwrap();
        response.recv()?
    }

    pub fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        self.sync_request_sender
            .send(Message::SetStatusSink(sink))
            .unwrap();
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

pub struct ElectrumConnection {
    incoming_requests: Receiver<Message>,
    url: String,
    network: bitcoin::Network,
    validate_domain: bool,
    state: ConnectionState,
    status_sink: Box<dyn Sink<ChainStatus>>,
}

enum ConnectionState {
    Disconnected,
    Connected(Box<BdkElectrumClient<electrum_client::Client>>),
}

/// This had to be a macro so that we can precisely mutably borrow certain fields.
macro_rules! try_connect {
    ($self:expr) => {
        try_connect(
            &mut $self.state,
            $self.validate_domain,
            $self.url.clone(),
            $self.status_sink.as_ref(),
        )
    };
}

impl ElectrumConnection {
    pub fn new(
        network: bitcoin::Network,
        url: String,
        validate_domain: bool,
    ) -> (Self, ChainClient) {
        let (sync_request_sender, incoming_requests) = sync_channel(1);
        (
            Self {
                incoming_requests,
                url,
                network,
                validate_domain,
                state: ConnectionState::Disconnected,
                status_sink: Box::new(()),
            },
            ChainClient {
                sync_request_sender,
            },
        )
    }

    pub fn spawn(mut self) {
        event!(
            Level::DEBUG,
            network = self.network.to_string(),
            "starting thread bitcoin network",
        );
        std::thread::spawn(move || loop {
            self.blocking_poll();
        });
    }

    fn set_connection_state(&mut self, state: ConnectionState) {
        self.state = state;
        self.emit_status();
    }

    fn emit_status(&self) {
        self.status_sink.send(ChainStatus {
            electrum_url: self.url.clone(),
            state: match &self.state {
                ConnectionState::Disconnected => ChainStatusState::Disconnected,
                ConnectionState::Connected(_) => ChainStatusState::Connected,
            },
        });
    }

    pub fn blocking_poll(&mut self) {
        match self
            .incoming_requests
            .recv()
            .expect("sender never disappears")
        {
            Message::SyncReq(req_and_response) => {
                let (req, resp) = req_and_response.into_tuple();
                let client = try_connect!(self);
                match client {
                    Ok(client) => {
                        self.status_sink.send(ChainStatus {
                            electrum_url: self.url.clone(),
                            state: ChainStatusState::Syncing,
                        });

                        match client.sync(req, 10, true) {
                            Ok(sync_result) => {
                                resp.send(Ok(sync_result));
                                self.emit_status();
                            }
                            Err(e) => {
                                self.set_connection_state(ConnectionState::Disconnected);
                                resp.send(Err(e.into()));
                            }
                        }
                    }
                    Err(e) => {
                        resp.send(Err(e));
                    }
                }
            }
            Message::ChangeUrlReq(req_and_response) => {
                let (url, resp) = req_and_response.into_tuple();
                let mut state = ConnectionState::Disconnected;
                let res = try_connect(
                    &mut state,
                    self.validate_domain,
                    url.clone(),
                    // ignore status during connection attempt
                    &(),
                )
                .map(|_| ());
                if res.is_ok() {
                    // replace out own connection with the new one
                    self.set_connection_state(state);
                    self.url = url;
                }

                resp.send(res);
            }
            Message::BroadcastReq(req_and_response) => {
                let (tx, resp) = req_and_response.into_tuple();
                match try_connect!(self) {
                    Ok(client) => {
                        event!(
                            Level::INFO,
                            txid = tx.compute_txid().to_string(),
                            "broadcasting transaction"
                        );

                        let res = client
                            .transaction_broadcast(&tx)
                            .map(|_| ())
                            .map_err(|e| e.into());
                        resp.send(res);
                    }
                    Err(e) => {
                        resp.send(Err(e));
                    }
                }
            }
            Message::SetStatusSink(sink) => {
                self.status_sink = sink;
                self.emit_status();
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
    Syncing,
    Disconnected,
    Connecting,
}

fn try_connect<'a>(
    state: &'a mut ConnectionState,
    validate_domain: bool,
    url: String,
    status_sink: &dyn Sink<ChainStatus>,
) -> Result<&'a mut BdkElectrumClient<electrum_client::Client>> {
    match state {
        ConnectionState::Disconnected => {
            event!(Level::INFO, url = url, "connecting to electrum server");
            let config = electrum_client::Config::builder()
                .validate_domain(validate_domain)
                .timeout(Some(4))
                .build();

            status_sink.send(ChainStatus {
                electrum_url: url.clone(),
                state: ChainStatusState::Connecting,
            });

            let electrum_client = electrum_client::Client::from_config(&url, config)
                .and_then(|client| {
                    // without pinging the thing on startup many errors are not checked for some reason
                    electrum_client::ElectrumApi::ping(&client)?;
                    Ok(client)
                })
                .inspect_err(|e| {
                    event!(
                        Level::ERROR,
                        error = e.to_string(),
                        url = url,
                        "failed to connect to electrum server"
                    );
                    status_sink.send(ChainStatus {
                        electrum_url: url.clone(),
                        state: ChainStatusState::Disconnected,
                    });
                })
                .context(format!("initializing electrum client to {}", url))?;

            let bdk_electrum_client = BdkElectrumClient::new(electrum_client);
            event!(Level::DEBUG, url = url, "connected to electrum server");
            status_sink.send(ChainStatus {
                electrum_url: url.clone(),
                state: ChainStatusState::Connected,
            });
            *state = ConnectionState::Connected(Box::new(bdk_electrum_client));
            match state {
                ConnectionState::Connected(bdk_electrum_client) => Ok(bdk_electrum_client),
                _ => unreachable!("we just connected"),
            }
        }
        ConnectionState::Connected(bdk_electrum_client) => Ok(bdk_electrum_client),
    }
}
