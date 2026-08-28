//! Wallet-side abstraction over the chain source.

use super::{
    chain_sync::{ChainClient, ChainStatus},
    wallet::KeychainId,
};
use crate::Sink;
use anyhow::Result;
use bdk_chain::bitcoin;
use std::collections::BTreeMap;

/// A source of chain data for a `CoordSuperWallet`.
pub trait ChainBackend: Send + 'static {
    /// Track `keychain` through `next_index` plus the backend's lookahead.
    fn monitor_keychain(&self, keychain: KeychainId, next_index: u32);
    /// Send a signed transaction to the network.
    fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid>;
    /// Fee rates for the given confirmation targets, in blocks.
    fn estimate_fee(&self, target_blocks: &[usize]) -> Result<BTreeMap<usize, bitcoin::FeeRate>>;
    /// Route connection status to `sink` for display.
    fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>);
    /// Drop the current connection and establish a new one.
    fn reconnect(&self);
}

impl ChainBackend for ChainClient {
    fn monitor_keychain(&self, keychain: KeychainId, next_index: u32) {
        ChainClient::monitor_keychain(self, keychain, next_index)
    }
    fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid> {
        ChainClient::broadcast(self, transaction)
    }
    fn estimate_fee(&self, target_blocks: &[usize]) -> Result<BTreeMap<usize, bitcoin::FeeRate>> {
        ChainClient::estimate_fee(self, target_blocks.iter().copied())
    }
    fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        ChainClient::set_status_sink(self, sink)
    }
    fn reconnect(&self) {
        ChainClient::reconnect(self)
    }
}
