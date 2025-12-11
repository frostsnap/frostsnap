use std::collections::BTreeMap;

use bdk_chain::{
    bitcoin::{Psbt, Txid},
    rusqlite_impl::migrate_schema,
};
use frostsnap_core::SignSessionId;
use rusqlite::named_params;

use crate::persist::{Persist, SqlPsbt, SqlSignSessionId, SqlTxid};

/// Sign session identified by [`SignSessionId`].
#[derive(Debug, Clone)]
pub struct PersistedPsbtBySsid {
    pub ssid: SignSessionId,
    pub txid: Txid,
    pub psbt: Psbt,
}

/// Sign session identified by [`Txid`].
#[derive(Debug, Clone)]
pub struct PersistedPsbtsByTxid {
    pub txid: Txid,
    pub psbt_by_ssid: BTreeMap<SignSessionId, Psbt>,
}

/// Migration logic shared between [`SsidPsbt`] and [`TxidPsbt`]
fn _migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
    const SCHEMA_NAME: &str = "frostsnap_psbt";
    const MIGRATIONS: &[&str] = &[
        // # Version 0
        "CREATE TABLE IF NOT EXISTS fs_psbt (
                id BLOB PRIMARY KEY NOT NULL,
                txid TEXT NOT NULL UNIQUE,
                psbt BLOB NOT NULL
            ) WITHOUT ROWID, STRICT",
        // # Version 1
        // TODO: Remove 'UNIQUE' from txid. Different ssids can now have the same txid.
    ];

    let db_tx = conn.transaction()?;
    migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
    db_tx.commit()?;
    Ok(())
}

impl Persist<rusqlite::Connection> for Option<PersistedPsbtBySsid> {
    type Update = Self;
    type LoadParams = SignSessionId;

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        _migrate(conn)
    }

    fn load(conn: &mut rusqlite::Connection, ssid: Self::LoadParams) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut stmt = conn.prepare_cached("SELECT txid, psbt FROM fs_psbt WHERE id=:id")?;
        let row_result = stmt.query_row(&[(":id", &SqlSignSessionId(ssid))], |row| {
            Ok((
                row.get::<_, SqlTxid>("txid")?,
                row.get::<_, SqlPsbt>("psbt")?,
            ))
        });
        Ok(match row_result {
            Ok((SqlTxid(txid), SqlPsbt(psbt))) => {
                Ok(Some(PersistedPsbtBySsid { ssid, txid, psbt }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }?)
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        let update = match update {
            Some(update) => update,
            None => return Ok(()),
        };

        let mut stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO fs_psbt(id, txid, psbt) VALUES(:id, :txid, :psbt)",
        )?;
        stmt.execute(named_params! {
            ":id": SqlSignSessionId(update.ssid),
            ":txid": SqlTxid(update.txid),
            ":psbt": SqlPsbt(update.psbt),
        })?;

        Ok(())
    }
}

impl Persist<rusqlite::Connection> for PersistedPsbtsByTxid {
    type Update = Self;
    type LoadParams = Txid;

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        _migrate(conn)
    }

    fn load(conn: &mut rusqlite::Connection, txid: Self::LoadParams) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut stmt = conn.prepare_cached("SELECT id, psbt FROM fs_psbt WHERE txid=:txid")?;
        let psbts = stmt
            .query_map(&[(":txid", &SqlTxid(txid))], |row| {
                let SqlSignSessionId(ssid) = row.get::<_, SqlSignSessionId>("id")?;
                let SqlPsbt(psbt) = row.get::<_, SqlPsbt>("psbt")?;
                Ok((ssid, psbt))
            })?
            .collect::<Result<BTreeMap<SignSessionId, Psbt>, rusqlite::Error>>()?;
        Ok(Self {
            txid,
            psbt_by_ssid: psbts,
        })
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        let mut stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO fs_psbt(id, txid, psbt) VALUES(:id, :txid, :psbt)",
        )?;

        // TODO: Maybe this should be done within a `rusqlite::Transaction`?
        for (ssid, psbt) in update.psbt_by_ssid {
            stmt.execute(named_params! {
                ":txid": SqlTxid(update.txid),
                ":id": SqlSignSessionId(ssid),
                ":psbt": SqlPsbt(psbt),
            })?;
        }

        Ok(())
    }
}
