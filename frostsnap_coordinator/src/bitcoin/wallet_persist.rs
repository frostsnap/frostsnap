use std::collections::btree_map;

use super::{
    wallet::{WalletIndexedTxGraph, WalletIndexedTxGraphChangeSet},
    wallet_reserved_spks::{self, ReservedSpks},
};
use crate::persist::Persist;
use anyhow::Result;
use bdk_chain::{
    bitcoin::BlockHash,
    local_chain::{self, LocalChain},
    ConfirmationBlockTime,
};

impl Persist<rusqlite::Connection> for ReservedSpks {
    type Update = wallet_reserved_spks::ChangeSet;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: Self::InitParams) -> Result<Self>
    where
        Self: Sized,
    {
        let db_tx = conn.transaction()?;
        wallet_reserved_spks::ChangeSet::init_sqlite_tables(&db_tx)?;
        let changeset = wallet_reserved_spks::ChangeSet::from_sqlite(&db_tx)?;
        let reserved_spks = Self::from_changeset(changeset);
        db_tx.commit()?;
        Ok(reserved_spks)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let db_tx = conn.transaction()?;
        update.persist_to_sqlite(&db_tx)?;
        db_tx.commit()?;
        Ok(())
    }
}

impl Persist<rusqlite::Connection> for WalletIndexedTxGraph {
    type Update = WalletIndexedTxGraphChangeSet;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: Self::InitParams) -> anyhow::Result<Self> {
        let db_tx = conn.transaction()?;

        // Migrations happen here.
        bdk_chain::tx_graph::ChangeSet::<ConfirmationBlockTime>::init_sqlite_tables(&db_tx)?;
        bdk_chain::indexer::keychain_txout::ChangeSet::init_sqlite_tables(&db_tx)?;

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
    type InitParams = BlockHash;
    type Update = local_chain::ChangeSet;

    fn initialize(conn: &mut rusqlite::Connection, block_hash: Self::InitParams) -> Result<Self> {
        let db_tx = conn.transaction()?;

        // Migrations happen here.
        bdk_chain::local_chain::ChangeSet::init_sqlite_tables(&db_tx)?;

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
