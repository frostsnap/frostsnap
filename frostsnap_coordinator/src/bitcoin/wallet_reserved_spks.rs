use std::collections::{BTreeSet, HashSet};

use bdk_chain::{
    bitcoin::{Script, ScriptBuf},
    rusqlite_impl::migrate_schema,
    Impl, Merge,
};
use rusqlite::named_params;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ChangeSet {
    // index, skp, reserved/unreserved
    spks: BTreeSet<(u64, ScriptBuf, bool)>,
}

impl Merge for ChangeSet {
    fn merge(&mut self, other: Self) {
        self.spks.merge(other.spks);
    }

    fn is_empty(&self) -> bool {
        self.spks.is_empty()
    }
}

/// Reserves spks.
#[derive(Debug, Clone)]
pub struct ReservedSpks {
    spks: HashSet<ScriptBuf>,
    /// So that our changesets can be fully monotone.
    next_seq: u64,
}

impl ReservedSpks {
    /// Create from changeset.
    pub fn from_changeset(changeset: ChangeSet) -> Self {
        let next_seq = changeset.spks.last().map_or(0, |(seq, _, _)| *seq + 1);
        let mut spks = HashSet::with_capacity(changeset.spks.len());
        for (_, spk, is_add) in changeset.spks {
            if is_add {
                spks.insert(spk)
            } else {
                spks.remove(&spk)
            };
        }
        Self { spks, next_seq }
    }

    pub fn contains(&self, spk: impl AsRef<Script>) -> bool {
        self.spks.contains(spk.as_ref())
    }

    pub fn reserve(&mut self, spk: ScriptBuf) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        if self.spks.insert(spk.clone()) {
            let seq = self.next_seq;
            self.next_seq += 1;
            changeset.spks.insert((seq, spk, true));
        }
        changeset
    }

    pub fn unreserve(&mut self, spk: impl AsRef<Script>) -> ChangeSet {
        let mut changeset = ChangeSet::default();
        if self.spks.remove(spk.as_ref()) {
            let seq = self.next_seq;
            self.next_seq += 1;
            changeset.spks.insert((seq, spk.as_ref().to_owned(), false));
        }
        changeset
    }
}

impl ChangeSet {
    /// Schema name for the changeset.
    pub const SCHEMA_NAME: &'static str = "fs_reserved_spks";
    /// Name for table that stores reserved spks.
    pub const RESERVED_SPKS_TABLE_NAME: &'static str = "fs_reserved_spks";

    pub fn schema_v0() -> String {
        format!(
            "CREATE TABLE {} ( \
            spk BLOB PRIMARY KEY NOT NULL, \
            seq INTEGER NOT NULL DEFAULT 0, \
            reserved INTEGER NOT NULL DEFAULT 0 CHECK (reserved in (0, 1))
            ) STRICT",
            Self::RESERVED_SPKS_TABLE_NAME,
        )
    }

    pub fn init_sqlite_tables(db_tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        migrate_schema(db_tx, Self::SCHEMA_NAME, &[&[&Self::schema_v0()]])
    }

    pub fn from_sqlite(db_tx: &rusqlite::Transaction) -> rusqlite::Result<Self> {
        let mut changeset = Self::default();
        let mut select_statement = db_tx.prepare(&format!(
            "SELECT spk, seq, reserved FROM {}",
            Self::RESERVED_SPKS_TABLE_NAME,
        ))?;
        let row_iter = select_statement.query_map([], |row| {
            Ok((
                row.get::<_, Impl<ScriptBuf>>("spk")?,
                row.get::<_, u64>("seq")?,
                row.get::<_, bool>("reserved")?,
            ))
        })?;
        for row in row_iter {
            let (Impl(spk), seq, res) = row?;
            // select spks that have been reserved
            if res {
                changeset.spks.insert((seq, spk, res));
            }
        }
        Ok(changeset)
    }

    pub fn persist_to_sqlite(&self, db_tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
        let mut upsert_statement = db_tx.prepare_cached(&format!(
            "INSERT INTO {}(spk, seq, reserved) VALUES(:spk, :seq, :reserved) \
                ON CONFLICT(spk) DO UPDATE SET seq=excluded.seq, reserved=excluded.reserved \
                WHERE excluded.seq >= {}.seq",
            Self::RESERVED_SPKS_TABLE_NAME,
            Self::RESERVED_SPKS_TABLE_NAME,
        ))?;
        for (seq, spk, reserved) in &self.spks {
            upsert_statement.execute(named_params! {
                ":spk": Impl(spk.clone()),
                ":seq": *seq,
                ":reserved": *reserved,
            })?;
        }
        Ok(())
    }
}
