use std::collections::btree_map;

use super::recovery_scan::{RecoveryScanState, RecoveryScanUpdate};
use super::wallet::{WalletIndexedTxGraph, WalletIndexedTxGraphChangeSet};
use crate::persist::{Persist, SqlMasterAppkey, SqlTxid};
use anyhow::Result;
use bdk_chain::{
    bitcoin::BlockHash,
    local_chain::{self, LocalChain},
    rusqlite_impl::migrate_schema,
    ConfirmationBlockTime,
};
use frostsnap_core::tweak::BitcoinBip32Path;
use rusqlite::named_params;

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
        // 50 scripts past the reveal frontier are derived, indexed against, and watched by the
        // chain source (bdk's default is 25): slop for indices the frontier doesn't know about -
        // restored wallets, other devices on the key, externally built PSBTs paying our own far
        // indices. Anything further past the frontier is outside the discovery contract.
        let mut indexed_tx_graph = Self::new(
            bdk_chain::indexer::keychain_txout::KeychainTxOutIndex::new(50, false),
        );
        indexed_tx_graph.apply_changeset(WalletIndexedTxGraphChangeSet {
            tx_graph: bdk_chain::tx_graph::ChangeSet::from_sqlite(&db_tx)?,
            indexer: bdk_chain::indexer::keychain_txout::ChangeSet::from_sqlite(&db_tx)?,
        });
        db_tx.commit()?;
        Ok(indexed_tx_graph)
    }

    fn persist_update(&self, conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let db_tx = conn.transaction()?;

        update.tx_graph.persist_to_sqlite(&db_tx)?;
        update.indexer.persist_to_sqlite(&db_tx)?;

        db_tx.commit()?;
        Ok(())
    }
}

impl Persist<rusqlite::Connection> for RecoveryScanState {
    type Update = RecoveryScanUpdate;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_recovery_scan";
        const MIGRATIONS: &[&str] = &["CREATE TABLE fs_recovery_scan_compared ( \
                master_appkey TEXT NOT NULL, \
                account_kind INTEGER NOT NULL, \
                account_index INTEGER NOT NULL, \
                keychain INTEGER NOT NULL, \
                compared_index INTEGER NOT NULL, \
                PRIMARY KEY (master_appkey, account_kind, account_index, keychain) \
            ) WITHOUT ROWID, STRICT; \
            CREATE TABLE fs_recovery_scan_outstanding ( \
                master_appkey TEXT NOT NULL, \
                txid TEXT NOT NULL, \
                PRIMARY KEY (master_appkey, txid) \
            ) WITHOUT ROWID, STRICT;"];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: Self::LoadParams) -> Result<Self> {
        let mut state = RecoveryScanState::default();

        let mut compared_stmt = conn.prepare_cached(
            "SELECT master_appkey, account_kind, account_index, keychain, compared_index \
             FROM fs_recovery_scan_compared",
        )?;
        let rows = compared_stmt.query_map([], |row| {
            Ok((
                row.get::<_, SqlMasterAppkey>("master_appkey")?,
                row.get::<_, u32>("account_kind")?,
                row.get::<_, u32>("account_index")?,
                row.get::<_, u32>("keychain")?,
                row.get::<_, u32>("compared_index")?,
            ))
        })?;
        for row in rows {
            let (SqlMasterAppkey(master_appkey), kind, index, keychain, compared_index) = row?;
            let path = BitcoinBip32Path::from_u32_slice(&[kind, index, keychain, 0])
                .ok_or_else(|| anyhow::anyhow!("invalid keychain row in recovery scan table"))?;
            state
                .compared
                .insert((master_appkey, path.account_keychain), compared_index);
        }

        let mut outstanding_stmt =
            conn.prepare_cached("SELECT master_appkey, txid FROM fs_recovery_scan_outstanding")?;
        let rows = outstanding_stmt.query_map([], |row| {
            Ok((
                row.get::<_, SqlMasterAppkey>("master_appkey")?,
                row.get::<_, SqlTxid>("txid")?,
            ))
        })?;
        for row in rows {
            let (SqlMasterAppkey(master_appkey), SqlTxid(txid)) = row?;
            state
                .outstanding
                .entry(master_appkey)
                .or_default()
                .insert(txid);
        }

        Ok(state)
    }

    fn persist_update(&self, conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let db_tx = conn.transaction()?;

        db_tx.execute(
            "DELETE FROM fs_recovery_scan_compared WHERE master_appkey=:master_appkey",
            named_params! { ":master_appkey": SqlMasterAppkey(update.master_appkey) },
        )?;
        db_tx.execute(
            "DELETE FROM fs_recovery_scan_outstanding WHERE master_appkey=:master_appkey",
            named_params! { ":master_appkey": SqlMasterAppkey(update.master_appkey) },
        )?;

        for (account_keychain, compared_index) in &update.compared {
            let mut segments = account_keychain.path_segments_from_bitcoin_appkey();
            let (kind, index, keychain) = (
                segments.next().expect("has kind"),
                segments.next().expect("has account index"),
                segments.next().expect("has keychain"),
            );
            db_tx.execute(
                "INSERT INTO fs_recovery_scan_compared \
                 (master_appkey, account_kind, account_index, keychain, compared_index) \
                 VALUES (:master_appkey, :account_kind, :account_index, :keychain, :compared_index)",
                named_params! {
                    ":master_appkey": SqlMasterAppkey(update.master_appkey),
                    ":account_kind": kind,
                    ":account_index": index,
                    ":keychain": keychain,
                    ":compared_index": compared_index,
                },
            )?;
        }
        for txid in &update.outstanding {
            db_tx.execute(
                "INSERT INTO fs_recovery_scan_outstanding (master_appkey, txid) \
                 VALUES (:master_appkey, :txid)",
                named_params! {
                    ":master_appkey": SqlMasterAppkey(update.master_appkey),
                    ":txid": SqlTxid(*txid),
                },
            )?;
        }

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

    fn persist_update(&self, conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let db_tx = conn.transaction()?;

        update.persist_to_sqlite(&db_tx)?;

        db_tx.commit()?;
        Ok(())
    }
}
