use std::collections::btree_map;

use super::wallet::{WalletIndexedTxGraph, WalletIndexedTxGraphChangeSet};
use crate::persist::Persist;
use anyhow::Result;
use bdk_chain::{
    bitcoin::BlockHash,
    local_chain::{self, LocalChain},
    ConfirmationBlockTime,
};

impl Persist<rusqlite::Connection> for WalletIndexedTxGraph {
    type Update = WalletIndexedTxGraphChangeSet;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        let db_tx = conn.transaction()?;

        bdk_chain::tx_graph::ChangeSet::<ConfirmationBlockTime>::init_sqlite_tables(&db_tx)?;
        bdk_chain::indexer::keychain_txout::ChangeSet::init_sqlite_tables(&db_tx)?;

        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: Self::LoadParams) -> anyhow::Result<Self> {
        let db_tx = conn.transaction()?;
        let mut indexed_tx_graph = Self::default();
        indexed_tx_graph.apply_changeset(WalletIndexedTxGraphChangeSet {
            tx_graph: bdk_chain::tx_graph::ChangeSet::from_sqlite(&db_tx)?,
            indexer: bdk_chain::indexer::keychain_txout::ChangeSet::from_sqlite(&db_tx)?,
        });
        db_tx.commit()?;
        Ok(indexed_tx_graph)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let db_tx = conn.transaction()?;

        update.tx_graph.persist_to_sqlite(&db_tx)?;
        update.indexer.persist_to_sqlite(&db_tx)?;

        db_tx.commit()?;
        Ok(())
    }
}

impl Persist<rusqlite::Connection> for local_chain::LocalChain {
    type LoadParams = BlockHash;
    type Update = local_chain::ChangeSet;

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        let db_tx = conn.transaction()?;
        bdk_chain::local_chain::ChangeSet::init_sqlite_tables(&db_tx)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, block_hash: Self::LoadParams) -> Result<Self> {
        let db_tx = conn.transaction()?;

        let mut changeset = bdk_chain::local_chain::ChangeSet::from_sqlite(&db_tx)?;
        if let btree_map::Entry::Vacant(entry) = changeset.blocks.entry(0) {
            entry.insert(Some(block_hash));
            changeset.persist_to_sqlite(&db_tx)?;
        }

        db_tx.commit()?;
        Ok(LocalChain::from_changeset(changeset).expect("must have genesis block"))
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let db_tx = conn.transaction()?;

        update.persist_to_sqlite(&db_tx)?;

        db_tx.commit()?;
        Ok(())
    }
}
