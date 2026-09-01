//! Persistence for [`bdk_electrum_streaming::Cache`].
//!
//! Only `subscriptions` and `headers` are persisted here — `tx_cache` is `#[serde(skip)]` on
//! `Cache` itself, because it's fully derivable from the wallet (see `ConnectionHandler::run`,
//! which seeds it from `super_wallet.tx_cache()` / `.anchor_cache()` on every startup).
use anyhow::Result;
use bdk_chain::{bitcoin, rusqlite_impl::migrate_schema};
use rusqlite::{params, OptionalExtension};

use crate::persist::Persist;

pub struct ElectrumCache {
    network: bitcoin::Network,
    pub cache: bdk_electrum_streaming::Cache,
}

impl ElectrumCache {
    pub fn new(network: bitcoin::Network, cache: bdk_electrum_streaming::Cache) -> Self {
        Self { network, cache }
    }
}

const SCHEMA_NAME: &str = "frostsnap_electrum_cache";
const MIGRATIONS: &[&str] = &[
    // Version 0 - initial schema
    "CREATE TABLE IF NOT EXISTS fs_electrum_cache (
        network TEXT PRIMARY KEY,
        cache_blob BLOB NOT NULL
    )",
];

impl Persist<rusqlite::Connection> for ElectrumCache {
    type Update = ();
    type LoadParams = bitcoin::Network;

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, network: Self::LoadParams) -> Result<Self> {
        let blob: Option<Vec<u8>> = conn
            .query_row(
                "SELECT cache_blob FROM fs_electrum_cache WHERE network = ?1",
                params![network.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let cache = blob
            .and_then(|blob| {
                bincode::serde::decode_from_slice(&blob, bincode::config::standard())
                    .inspect_err(|err| {
                        tracing::warn!(
                            error = err.to_string(),
                            "Failed to decode persisted electrum cache, starting fresh"
                        )
                    })
                    .ok()
                    .map(|(cache, _)| cache)
            })
            .unwrap_or_default();
        Ok(Self { network, cache })
    }

    fn persist_update(&self, conn: &mut rusqlite::Connection, _update: Self::Update) -> Result<()> {
        let blob = bincode::serde::encode_to_vec(&self.cache, bincode::config::standard())?;
        conn.execute(
            "INSERT OR REPLACE INTO fs_electrum_cache (network, cache_blob) VALUES (?1, ?2)",
            params![self.network.to_string(), blob],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_chain::bitcoin::{
        absolute::LockTime,
        block::{Header, Version},
        hashes::Hash,
        BlockHash, CompactTarget, Transaction, TxMerkleNode,
    };
    use std::sync::Arc;

    fn dummy_header() -> Header {
        Header {
            version: Version::ONE,
            prev_blockhash: BlockHash::all_zeros(),
            merkle_root: TxMerkleNode::all_zeros(),
            time: 0,
            bits: CompactTarget::from_consensus(0),
            nonce: 0,
        }
    }

    fn dummy_tx() -> Transaction {
        Transaction {
            version: bdk_chain::bitcoin::transaction::Version::ONE,
            lock_time: LockTime::ZERO,
            input: Vec::new(),
            output: Vec::new(),
        }
    }

    /// `headers` (persisted) must survive a round trip; `tx_cache` (derived from wallet data,
    /// never persisted) must not, so a fresh load never carries stale wallet data.
    #[test]
    fn persist_and_load_round_trips_headers_but_not_tx_cache() -> Result<()> {
        let mut conn = rusqlite::Connection::open_in_memory()?;
        ElectrumCache::migrate(&mut conn)?;

        let mut cache = bdk_electrum_streaming::Cache::default();
        let header = dummy_header();
        cache.headers.insert(header.block_hash(), header);
        let tx = dummy_tx();
        cache.tx_cache.txs.insert(tx.compute_txid(), Arc::new(tx));

        ElectrumCache::new(bitcoin::Network::Signet, cache).persist_update(&mut conn, ())?;

        let loaded = ElectrumCache::load(&mut conn, bitcoin::Network::Signet)?;
        assert_eq!(loaded.cache.headers.len(), 1, "headers must be persisted");
        assert!(
            loaded.cache.tx_cache.txs.is_empty(),
            "tx_cache must never be persisted"
        );

        Ok(())
    }
}
