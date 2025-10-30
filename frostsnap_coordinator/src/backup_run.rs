use crate::persist::{Persist, ToStringWrapper};
use bdk_chain::rusqlite_impl::migrate_schema;
use frostsnap_core::{AccessStructureId, AccessStructureRef, Gist, KeyId};
use rusqlite::params;
use std::collections::BTreeMap;
use tracing::{event, Level};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayBackupState {
    pub confirmed: bool,
    pub legacy_display_confirmed: bool,
}

#[derive(Default, Debug)]
pub struct BackupState {
    // Maps each access_structure_ref to a map of share_index -> complete boolean.
    runs: BTreeMap<AccessStructureRef, BTreeMap<u32, bool>>,
}

impl BackupState {
    fn apply_mutation(&mut self, mutation: &Mutation) -> bool {
        match mutation {
            Mutation::AddShareNeedsBackup {
                access_structure_ref,
                share_index,
            } => {
                let run = self.runs.entry(*access_structure_ref).or_default();
                run.insert(*share_index, false);
                true
            }
            Mutation::MarkShareComplete {
                access_structure_ref,
                share_index,
            } => {
                let run = self.runs.entry(*access_structure_ref).or_default();
                run.insert(*share_index, true);
                true
            }
            Mutation::ClearBackupState {
                access_structure_ref,
            } => self.runs.remove(access_structure_ref).is_some(),
        }
    }

    fn mutate(&mut self, mutation: Mutation, mutations: &mut Vec<Mutation>) {
        if self.apply_mutation(&mutation) {
            event!(Level::DEBUG, gist = mutation.gist(), "mutating");
            mutations.push(mutation);
        }
    }

    pub fn start_run(
        &mut self,
        access_structure_ref: AccessStructureRef,
        share_indices: Vec<u32>,
        mutations: &mut Vec<Mutation>,
    ) {
        for share_index in share_indices {
            self.mutate(
                Mutation::AddShareNeedsBackup {
                    access_structure_ref,
                    share_index,
                },
                mutations,
            );
        }
    }

    pub fn mark_backup_complete(
        &mut self,
        access_structure_ref: AccessStructureRef,
        share_index: u32,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(
            Mutation::MarkShareComplete {
                access_structure_ref,
                share_index,
            },
            mutations,
        );
    }

    /// We want the API to assume there's only one access structure for key for now so we have this
    /// hack. If/when we want to have backups for other access structures then we can do that and
    /// change the API here.
    fn guess_access_structure_ref_for_key(&self, key_id: KeyId) -> Option<AccessStructureRef> {
        let (access_structure_ref, _) = self
            .runs
            .range(AccessStructureRef::range_for_key(key_id))
            .next()?;
        Some(*access_structure_ref)
    }

    pub fn clear_backup_run(&mut self, key_id: KeyId, mutations: &mut Vec<Mutation>) {
        if let Some(access_structure_ref) = self.guess_access_structure_ref_for_key(key_id) {
            self.mutate(
                Mutation::ClearBackupState {
                    access_structure_ref,
                },
                mutations,
            );
        }
    }

    pub fn get_backup_run(&self, key_id: KeyId) -> BTreeMap<u32, bool> {
        let access_structure_ref = match self.guess_access_structure_ref_for_key(key_id) {
            Some(asref) => asref,
            None => return Default::default(),
        };
        self.runs
            .get(&access_structure_ref)
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub enum Mutation {
    AddShareNeedsBackup {
        access_structure_ref: AccessStructureRef,
        share_index: u32,
    },
    MarkShareComplete {
        access_structure_ref: AccessStructureRef,
        share_index: u32,
    },
    ClearBackupState {
        access_structure_ref: AccessStructureRef,
    },
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        match self {
            Mutation::AddShareNeedsBackup { .. } => "AddShareNeedsBackup",
            Mutation::MarkShareComplete { .. } => "MarkShareComplete",
            Mutation::ClearBackupState { .. } => "ClearBackupState",
        }
        .to_string()
    }
}

const SCHEMA_NAME: &str = "frostsnap_backup_state";
const MIGRATIONS: &[&str] = &[
    // Version 0
    "CREATE TABLE IF NOT EXISTS backup_runs ( \
                key_id TEXT NOT NULL, \
                access_structure_id TEXT NOT NULL, \
                device_id TEXT NOT NULL, \
                timestamp INTEGER, \
                PRIMARY KEY (key_id, access_structure_id, device_id) \
            )",
    // Version 1: Change from timestamp + device_id to complete boolean + share_index
    // Preserve the gist: create share indices 1..N where N is the number of devices
    // If all devices were complete, mark all shares complete; otherwise all incomplete
    "CREATE TABLE backup_runs_new ( \
                key_id TEXT NOT NULL, \
                access_structure_id TEXT NOT NULL, \
                share_index INTEGER NOT NULL, \
                complete BOOLEAN NOT NULL, \
                PRIMARY KEY (key_id, access_structure_id, share_index) \
            ); \
            WITH backup_summary AS ( \
                SELECT key_id, access_structure_id, \
                       COUNT(*) as device_count, \
                       MIN(CASE WHEN timestamp IS NOT NULL THEN 1 ELSE 0 END) as all_complete \
                FROM backup_runs \
                GROUP BY key_id, access_structure_id \
            ), \
            numbers(n) AS ( \
                SELECT 1 \
                UNION ALL \
                SELECT n + 1 FROM numbers WHERE n < 50 \
            ) \
            INSERT INTO backup_runs_new (key_id, access_structure_id, share_index, complete) \
            SELECT bs.key_id, bs.access_structure_id, n.n, bs.all_complete \
            FROM backup_summary bs \
            JOIN numbers n ON n.n <= bs.device_count; \
            DROP TABLE backup_runs; \
            ALTER TABLE backup_runs_new RENAME TO backup_runs;",
];

impl Persist<rusqlite::Connection> for BackupState {
    type Update = Vec<Mutation>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: ()) -> anyhow::Result<Self> {
        let mut stmt = conn.prepare(
            r#"
            SELECT key_id, access_structure_id, share_index, complete
            FROM backup_runs
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, ToStringWrapper<KeyId>>(0)?.0,
                row.get::<_, ToStringWrapper<AccessStructureId>>(1)?.0,
                row.get::<_, u32>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;

        let mut state = BackupState::default();

        for row in rows.into_iter() {
            let (key_id, access_structure_id, share_index, complete) = row?;
            let access_structure_ref = AccessStructureRef {
                key_id,
                access_structure_id,
            };
            state.apply_mutation(&Mutation::AddShareNeedsBackup {
                access_structure_ref,
                share_index,
            });
            if complete {
                state.apply_mutation(&Mutation::MarkShareComplete {
                    access_structure_ref,
                    share_index,
                });
            }
        }

        Ok(state)
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Vec<Mutation>,
    ) -> anyhow::Result<()> {
        let tx = conn.transaction()?;
        for mutation in update {
            match mutation {
                Mutation::AddShareNeedsBackup {
                    access_structure_ref,
                    share_index,
                } => {
                    tx.execute(
                        r#"
                        INSERT INTO backup_runs (key_id, access_structure_id, share_index, complete)
                        VALUES (?1, ?2, ?3, ?4)
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                            share_index,
                            false
                        ],
                    )?;
                }
                Mutation::MarkShareComplete {
                    access_structure_ref,
                    share_index,
                } => {
                    tx.execute(
                        r#"
                        INSERT OR REPLACE INTO backup_runs (key_id, access_structure_id, share_index, complete)
                        VALUES (?1, ?2, ?3, ?4)
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                            share_index,
                            true
                        ],
                    )?;
                }
                Mutation::ClearBackupState {
                    access_structure_ref,
                } => {
                    tx.execute(
                        r#"
                        DELETE FROM backup_runs
                        WHERE key_id=?1 AND access_structure_id=?2
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                        ],
                    )?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::Persist;
    use rusqlite::Connection;
    use std::str::FromStr;

    fn setup_version_0_db(conn: &mut Connection) {
        conn.execute(MIGRATIONS[0], []).unwrap();

        conn.execute_batch(
            "INSERT INTO backup_runs VALUES('8715c3b68a7d7f2c248b5f63e923d5aeafe7f29458bb825646e024c1816181f2','e74a55491d1dd0ba1f8ac57ef3e9ed58b1d1046aa0698009b4bec1be0e0d8bd2','027feb109a5690935a1e862f3a20be00ffe6a36dcfeed3d0eb2f7f056e5d543abb',NULL);\
             INSERT INTO backup_runs VALUES('8715c3b68a7d7f2c248b5f63e923d5aeafe7f29458bb825646e024c1816181f2','e74a55491d1dd0ba1f8ac57ef3e9ed58b1d1046aa0698009b4bec1be0e0d8bd2','02f3fab71a90989c57dbc72b4bbaf5dc82b82552bcd1c416738d081daa248214c7',NULL);\
             INSERT INTO backup_runs VALUES('8715c3b68a7d7f2c248b5f63e923d5aeafe7f29458bb825646e024c1816181f2','e74a55491d1dd0ba1f8ac57ef3e9ed58b1d1046aa0698009b4bec1be0e0d8bd2','0333e61a47f72480bf40ad938ddb6d6e0eb94608ddb4921305bee5d36ef0c2e5a2',NULL)"
        )
        .unwrap();
    }

    /// Tests migration from Version 0 (device_id + timestamp) to Version 1 (share_index + complete).
    ///
    /// ## Why this change was made
    ///
    /// The original schema tracked backup completion per DeviceId. However, this was incorrect
    /// because what actually needs to be backed up is the share at a particular ShareIndex, not
    /// the specific device holding it. If two devices hold the same share (same ShareIndex),
    /// backing up either one satisfies the backup requirement for that share.
    ///
    /// ## Difficulties in migration
    ///
    /// The main difficulty is that the old schema didn't store ShareIndex at all - it only had
    /// DeviceId. This means we cannot accurately map old device backups to specific share indices.
    ///
    /// To preserve the "gist" of existing backup state, the migration:
    /// 1. Counts how many devices were in each backup run
    /// 2. Creates share indices 1..N (where N is the device count)
    /// 3. Uses an all-or-nothing approach: if ALL old devices were complete (had timestamps),
    ///    mark all new shares as complete; otherwise mark all as incomplete
    ///
    /// This is a lossy migration but preserves the essential information: whether a backup run
    /// was fully completed or not.
    #[test]
    fn test_migration_from_v0_to_v1() {
        let key_id =
            KeyId::from_str("8715c3b68a7d7f2c248b5f63e923d5aeafe7f29458bb825646e024c1816181f2")
                .unwrap();

        // Section 1: 0 of 3 devices complete
        {
            let mut conn = Connection::open_in_memory().unwrap();
            setup_version_0_db(&mut conn);

            BackupState::migrate(&mut conn).unwrap();
            let state = BackupState::load(&mut conn, ()).unwrap();
            let backup_run = state.get_backup_run(key_id);

            assert_eq!(backup_run.len(), 3, "Should have 3 shares");
            assert_eq!(backup_run.get(&1), Some(&false));
            assert_eq!(backup_run.get(&2), Some(&false));
            assert_eq!(backup_run.get(&3), Some(&false));
        }

        // Section 2: 2 of 3 devices complete (partial completion)
        {
            let mut conn = Connection::open_in_memory().unwrap();
            setup_version_0_db(&mut conn);

            conn.execute(
                "UPDATE backup_runs SET timestamp = 1761805723 WHERE device_id = '027feb109a5690935a1e862f3a20be00ffe6a36dcfeed3d0eb2f7f056e5d543abb'",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE backup_runs SET timestamp = 1761805724 WHERE device_id = '02f3fab71a90989c57dbc72b4bbaf5dc82b82552bcd1c416738d081daa248214c7'",
                [],
            )
            .unwrap();

            BackupState::migrate(&mut conn).unwrap();
            let state = BackupState::load(&mut conn, ()).unwrap();
            let backup_run = state.get_backup_run(key_id);

            assert_eq!(backup_run.len(), 3, "Should have 3 shares");
            assert_eq!(backup_run.get(&1), Some(&false));
            assert_eq!(backup_run.get(&2), Some(&false));
            assert_eq!(backup_run.get(&3), Some(&false));
        }

        // Section 3: 3 of 3 devices complete (full completion)
        {
            let mut conn = Connection::open_in_memory().unwrap();
            setup_version_0_db(&mut conn);

            conn.execute(
                "UPDATE backup_runs SET timestamp = 1761805723 WHERE device_id = '027feb109a5690935a1e862f3a20be00ffe6a36dcfeed3d0eb2f7f056e5d543abb'",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE backup_runs SET timestamp = 1761805724 WHERE device_id = '02f3fab71a90989c57dbc72b4bbaf5dc82b82552bcd1c416738d081daa248214c7'",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE backup_runs SET timestamp = 1761805725 WHERE device_id = '0333e61a47f72480bf40ad938ddb6d6e0eb94608ddb4921305bee5d36ef0c2e5a2'",
                [],
            )
            .unwrap();

            BackupState::migrate(&mut conn).unwrap();
            let state = BackupState::load(&mut conn, ()).unwrap();
            let backup_run = state.get_backup_run(key_id);

            assert_eq!(backup_run.len(), 3, "Should have 3 shares");
            assert_eq!(backup_run.get(&1), Some(&true));
            assert_eq!(backup_run.get(&2), Some(&true));
            assert_eq!(backup_run.get(&3), Some(&true));
        }
    }
}
