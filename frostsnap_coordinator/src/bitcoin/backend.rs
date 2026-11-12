//! The chain source a wallet follows.

use super::{
    chain_sync::{ChainClient, ChainStatus},
    wallet::KeychainId,
};
use crate::Sink;
use anyhow::Result;
use bdk_chain::bitcoin;
use std::collections::BTreeMap;

/// The chain source a `CoordSuperWallet` follows.
#[derive(Clone)]
pub enum ChainBackend {
    Electrum(ChainClient),
}

impl From<ChainClient> for ChainBackend {
    fn from(client: ChainClient) -> Self {
        ChainBackend::Electrum(client)
    }
}

impl ChainBackend {
    /// Track `keychain` through `next_index` plus the source's lookahead.
    pub fn monitor_keychain(&self, keychain: KeychainId, next_index: u32) {
        match self {
            ChainBackend::Electrum(client) => client.monitor_keychain(keychain, next_index),
        }
    }

    /// Send a signed transaction to the network.
    pub fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid> {
        match self {
            ChainBackend::Electrum(client) => client.broadcast(transaction),
        }
    }

    /// Fee rates for the given confirmation targets, in blocks.
    pub fn estimate_fee(
        &self,
        target_blocks: impl IntoIterator<Item = usize>,
    ) -> Result<BTreeMap<usize, bitcoin::FeeRate>> {
        match self {
            ChainBackend::Electrum(client) => client.estimate_fee(target_blocks),
        }
    }

    /// Route connection status to `sink` for display.
    pub fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        match self {
            ChainBackend::Electrum(client) => client.set_status_sink(sink),
        }
    }

    /// Drop the current connection and establish a new one.
    pub fn reconnect(&self) {
        match self {
            ChainBackend::Electrum(client) => client.reconnect(),
        }
    }

    /// The Electrum client, for the settings only Electrum has.
    pub fn electrum(&self) -> Result<&ChainClient> {
        match self {
            ChainBackend::Electrum(client) => Ok(client),
        }
    }
}
