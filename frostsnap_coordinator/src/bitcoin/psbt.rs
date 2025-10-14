use bdk_chain::{
    bitcoin::{Psbt, Txid},
    rusqlite_impl::migrate_schema,
};
use frostsnap_core::SignSessionId;
use rusqlite::{named_params, CachedStatement, ToSql};

use crate::persist::{Persist, SqlPsbt, SqlSignSessionId, SqlTxid};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadSignSessionPsbtParams {
    Ssid(SignSessionId),
    Txid(Txid),
}

#[derive(Debug, Clone)]
pub struct SignSessionPsbt {
    pub ssid: SignSessionId,
    pub txid: Txid,
    pub psbt: Psbt,
}

impl Persist<rusqlite::Connection> for Option<SignSessionPsbt> {
    type Update = Self;

    type LoadParams = LoadSignSessionPsbtParams;

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_psbt";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_psbt (
                id BLOB PRIMARY KEY NOT NULL,
                txid TEXT NOT NULL UNIQUE,
                psbt BLOB NOT NULL
            ) WITHOUT ROWID, STRICT",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;

        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, params: Self::LoadParams) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut stmt: CachedStatement<'_>;
        let stmt_param_key: &str;
        let stmt_param_val: Box<dyn ToSql>;
        match params {
            LoadSignSessionPsbtParams::Ssid(ssid) => {
                stmt = conn.prepare_cached("SELECT id, txid, psbt FROM fs_psbt WHERE id=:id")?;
                stmt_param_key = ":id";
                stmt_param_val = Box::new(SqlSignSessionId(ssid));
            }
            LoadSignSessionPsbtParams::Txid(txid) => {
                stmt =
                    conn.prepare_cached("SELECT id, txid, psbt FROM fs_psbt WHERE txid=:txid")?;
                stmt_param_key = ":txid";
                stmt_param_val = Box::new(SqlTxid(txid));
            }
        };

        let row_result = stmt.query_row(&[(stmt_param_key, &*stmt_param_val)], |row| {
            Ok((
                row.get::<_, SqlSignSessionId>("id")?,
                row.get::<_, SqlTxid>("txid")?,
                row.get::<_, SqlPsbt>("psbt")?,
            ))
        });
        Ok(match row_result {
            Ok((SqlSignSessionId(ssid), SqlTxid(txid), SqlPsbt(psbt))) => {
                Ok(Some(SignSessionPsbt { ssid, txid, psbt }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }?)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> anyhow::Result<()> {
        let update = match update {
            Some(update) => update,
            None => return Ok(()),
        };

        let mut add_stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO fs_psbt(id, txid, psbt) VALUES(:id, :txid, :psbt)",
        )?;
        add_stmt.execute(named_params! {
            ":id": SqlSignSessionId(update.ssid),
            ":txid": SqlTxid(update.txid),
            ":psbt": SqlPsbt(update.psbt),
        })?;

        Ok(())
    }
}
