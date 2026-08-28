//! Per-network chain source as the app holds it: Electrum or (desktop-only) compact filters.

use anyhow::{anyhow, Result};
use frostsnap_coordinator::bitcoin::backend::ChainBackend;
use frostsnap_coordinator::bitcoin::chain_sync::{ChainClient, ChainStatus};
use frostsnap_coordinator::bitcoin::wallet::KeychainId;
use frostsnap_coordinator::Sink;
use std::collections::BTreeMap;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use frostsnap_coordinator::bitcoin::compact_filters::node::FilterClient;

#[derive(Clone)]
pub enum ChainApi {
    Electrum(ChainClient),
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    CompactFilters(FilterClient),
}

impl ChainApi {
    /// The Electrum client, or an error naming what is running.
    pub fn electrum(&self) -> Result<&ChainClient> {
        match self {
            ChainApi::Electrum(client) => Ok(client),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ChainApi::CompactFilters(_) => Err(anyhow!(
                "this network is using compact block filters, which have no Electrum server to set"
            )),
        }
    }
}

impl ChainBackend for ChainApi {
    fn monitor_keychain(&self, keychain: KeychainId, next_index: u32) {
        match self {
            ChainApi::Electrum(client) => client.monitor_keychain(keychain, next_index),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ChainApi::CompactFilters(client) => client.monitor_keychain(keychain, next_index),
        }
    }

    fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid> {
        match self {
            ChainApi::Electrum(client) => client.broadcast(transaction),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ChainApi::CompactFilters(client) => client.broadcast(transaction),
        }
    }

    fn estimate_fee(&self, target_blocks: &[usize]) -> Result<BTreeMap<usize, bitcoin::FeeRate>> {
        match self {
            ChainApi::Electrum(client) => client.estimate_fee(target_blocks.iter().copied()),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ChainApi::CompactFilters(client) => client.estimate_fee(target_blocks),
        }
    }

    fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        match self {
            ChainApi::Electrum(client) => client.set_status_sink(sink),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ChainApi::CompactFilters(client) => client.set_status_sink(sink),
        }
    }

    fn reconnect(&self) {
        match self {
            ChainApi::Electrum(client) => client.reconnect(),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            ChainApi::CompactFilters(client) => client.reconnect(),
        }
    }
}
