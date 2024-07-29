use super::wallet::{WalletIndexedTxGraph, WalletIndexedTxGraphChangeSet};
use crate::persist::{Persist, SqlBitcoinTransaction, SqlBlockHash, SqlDescriptorId, SqlTxid};
use anyhow::{anyhow, Result};
use bdk_chain::{
    bitcoin::BlockHash, indexer::keychain_txout, local_chain, BlockId, ConfirmationBlockTime,
};
use rusqlite::params;

impl Persist<rusqlite::Connection> for WalletIndexedTxGraph {
    type Update = WalletIndexedTxGraphChangeSet;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: Self::InitParams) -> anyhow::Result<Self> {
        let mut txs = WalletIndexedTxGraph::default();

        // index
        {
            let mut changeset = keychain_txout::ChangeSet::default();
            conn.execute(
                "CREATE TABLE IF NOT EXISTS bdk_keychain (
                    descriptor_id TEXT PRIMARY KEY,
                    last_revealed INTEGER NOT NULL
                )",
                [],
            )?;

            let mut stmt = conn.prepare("SELECT descriptor_id, last_revealed FROM bdk_keychain")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, SqlDescriptorId>(0)?.0, row.get::<_, u32>(1)?))
            })?;

            for row in rows {
                let (descriptor_id, last_revealed) = row?;
                changeset.last_revealed.insert(descriptor_id, last_revealed);
            }
            txs.index.apply_changeset(changeset);
        }

        // transactions
        {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS bdk_transactions (
                txid TEXT PRIMARY KEY,
                bitcoin_tx BLOB,
                last_seen INTEGER
            )",
                [],
            )?;

            let mut stmt =
                conn.prepare("SELECT txid, bitcoin_tx,last_seen FROM bdk_transactions")?;
            let row_iter = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, SqlTxid>(0)?.0,
                    row.get::<_, Option<SqlBitcoinTransaction>>(1)?,
                    row.get::<_, Option<u64>>(2)?,
                ))
            })?;

            for tx in row_iter {
                let (txid, tx, last_seen) = tx?;
                if let Some(tx) = tx {
                    let _ = txs.insert_tx(tx.0);
                }
                if let Some(last_seen) = last_seen {
                    let _ = txs.insert_seen_at(txid, last_seen);
                }
            }
        }

        // anchors
        {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS bdk_anchors (
                 txid TEXT NOT NULL,
                 height INTEGER NOT NULL,
                 blockhash TEXT NOT NULL,
                 timestamp INTEGER NOT NULL,
                 PRIMARY KEY (txid, height, blockhash)
            )",
                [],
            )?;

            let mut stmt =
                conn.prepare("SELECT txid,height,blockhash,height,timestamp FROM bdk_anchors")?;

            let row_iter = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, SqlTxid>(0)?.0,
                    row.get::<_, u32>(1)?,
                    row.get::<_, SqlBlockHash>(2)?.0,
                    row.get::<_, u64>(4)?,
                ))
            })?;

            for row in row_iter {
                let (txid, height, hash, confirmation_time) = row?;
                let _ = txs.insert_anchor(
                    txid,
                    ConfirmationBlockTime {
                        block_id: BlockId { height, hash },
                        confirmation_time,
                    },
                );
            }
        }

        Ok(txs)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        for tx in &update.graph.txs {
            conn.execute(
                "INSERT INTO bdk_transactions (txid, bitcoin_tx) VALUES (?1, ?2)
                 ON CONFLICT(txid) DO UPDATE SET bitcoin_tx=excluded.bitcoin_tx",
                params![
                    SqlTxid(tx.compute_txid()),
                    SqlBitcoinTransaction(tx.clone())
                ],
            )?;
        }

        for (txid, last_seen) in &update.graph.last_seen {
            conn.execute(
                "INSERT INTO bdk_transactions (txid, last_seen) VALUES (?1, ?2)
                   ON CONFLICT(txid) DO UPDATE SET
                     last_seen = COALESCE(MAX(excluded.last_seen, bdk_transactions.last_seen), excluded.last_seen);",
                // the COALESCE is needed because for some reason MAX(7, NULL) returns NULL
                params![SqlTxid(*txid), last_seen],
            )?;
        }

        for (anchor, txid) in &update.graph.anchors {
            conn.execute(
                "INSERT OR REPLACE INTO bdk_anchors (txid, height, blockhash, timestamp) VALUES (?1, ?2, ?3, ?4)",
                params![SqlTxid(*txid), anchor.block_id.height, SqlBlockHash(anchor.block_id.hash), anchor.confirmation_time])?;
        }

        for (descriptor_id, last_revealed) in &update.indexer.last_revealed {
            conn.execute(
                "INSERT INTO bdk_keychain (descriptor_id, last_revealed) VALUES (?1, ?2)
                 ON CONFLICT(descriptor_id) DO UPDATE SET
                 last_revealed = COALESCE(MAX(excluded.last_revealed, bdk_keychain.last_revealed), excluded.last_revealed)",
                // the COALESCE is needed because for some reason MAX(7, NULL) returns NULL
                params![SqlDescriptorId(*descriptor_id), last_revealed],
            )?;
        }
        Ok(())
    }
}

impl Persist<rusqlite::Connection> for local_chain::LocalChain {
    type InitParams = BlockHash;
    type Update = local_chain::ChangeSet;

    fn initialize(conn: &mut rusqlite::Connection, block_hash: Self::InitParams) -> Result<Self> {
        use anyhow::Context;
        let mut changeset = local_chain::ChangeSet::default();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS bdk_local_chain (
                height INTEGER PRIMARY KEY,
                hash TEXT
            )",
            [],
        )
        .context("creating bdk_local_chain table")?;

        // insert genesis block for the
        conn.execute(
            "INSERT OR IGNORE INTO bdk_local_chain (height, hash) VALUES (0, ?1)",
            params![SqlBlockHash(block_hash)],
        )
        .context("inserting genesis block")?;

        let got_gen_block_hash = conn
            .query_row(
                "SELECT hash FROM bdk_local_chain WHERE height = 0",
                [],
                |row| Ok(row.get::<_, SqlBlockHash>(0)?.0),
            )
            .context("querying genesis block")?;

        if got_gen_block_hash != block_hash {
            return Err(anyhow!("the database was initialized with a genesis block of {got_gen_block_hash} but we tried to use it for {block_hash}"));
        }

        let mut stmt = conn
            .prepare("SELECT height,hash FROM bdk_local_chain")
            .context("preparing blocks query")?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, SqlBlockHash>(1)?.0))
            })
            .context("loading blocks")?;

        for row in rows {
            let (height, hash) = row.context("loading block")?;
            changeset.insert(height, Some(hash));
        }

        Ok(Self::from_changeset(changeset)?)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        let tx = conn.transaction()?;
        for (height, hash) in update {
            tx.execute(
                "INSERT OR REPLACE INTO bdk_local_chain (height, hash) VALUES (?1, ?2)",
                params![height, hash.map(SqlBlockHash)],
            )?;
        }
        tx.commit()?;

        Ok(())
    }
}
