use crate::persist::{Persist, ToStringWrapper};
use bdk_chain::rusqlite_impl::migrate_schema;
use frostsnap_core::{AccessStructureId, AccessStructureRef, DeviceId, Gist, KeyId};
use rusqlite::params;
use std::collections::BTreeMap;
use tracing::{event, Level};

#[derive(Default, Debug)]
pub struct BackupState {
    // Maps each access_structure_ref to a map of device_id -> complete boolean.
    runs: BTreeMap<AccessStructureRef, BTreeMap<DeviceId, bool>>,
}

impl BackupState {
    fn apply_mutation(&mut self, mutation: &Mutation) -> bool {
        match mutation {
            Mutation::AddDeviceNeedsBackup {
                access_structure_ref,
                device_id,
            } => {
                let run = self.runs.entry(*access_structure_ref).or_default();
                run.insert(*device_id, false);
                true
            }
            Mutation::MarkDeviceComplete {
                access_structure_ref,
                device_id,
            } => {
                if let Some(run) = self.runs.get_mut(access_structure_ref) {
                    if let Some(entry) = run.get_mut(device_id) {
                        *entry = true;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
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
        devices: Vec<DeviceId>,
        mutations: &mut Vec<Mutation>,
    ) {
        for device_id in devices {
            self.mutate(
                Mutation::AddDeviceNeedsBackup {
                    access_structure_ref,
                    device_id,
                },
                mutations,
            );
        }
    }

    pub fn mark_backup_complete(
        &mut self,
        key_id: KeyId,
        device_id: DeviceId,
        mutations: &mut Vec<Mutation>,
    ) {
        if let Some(access_structure_ref) = self.guess_access_structure_ref_for_key(key_id) {
            self.mutate(
                Mutation::MarkDeviceComplete {
                    access_structure_ref,
                    device_id,
                },
                mutations,
            );
        }
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

    pub fn get_backup_run(&self, key_id: KeyId) -> BTreeMap<DeviceId, bool> {
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
    AddDeviceNeedsBackup {
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    },
    MarkDeviceComplete {
        access_structure_ref: AccessStructureRef,
        device_id: DeviceId,
    },
    ClearBackupState {
        access_structure_ref: AccessStructureRef,
    },
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        match self {
            Mutation::AddDeviceNeedsBackup { .. } => "AddDeviceNeedsBackup",
            Mutation::MarkDeviceComplete { .. } => "MarkDeviceComplete",
            Mutation::ClearBackupState { .. } => "ClearBackupState",
        }
        .to_string()
    }
}

impl Persist<rusqlite::Connection> for BackupState {
    type Update = Vec<Mutation>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
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
            // Version 1: Change timestamp to complete boolean
            "CREATE TABLE backup_runs_new ( \
                key_id TEXT NOT NULL, \
                access_structure_id TEXT NOT NULL, \
                device_id TEXT NOT NULL, \
                complete BOOLEAN NOT NULL, \
                PRIMARY KEY (key_id, access_structure_id, device_id) \
            ); \
            INSERT INTO backup_runs_new (key_id, access_structure_id, device_id, complete) \
            SELECT key_id, access_structure_id, device_id, \
                   CASE WHEN timestamp IS NOT NULL THEN 1 ELSE 0 END \
            FROM backup_runs; \
            DROP TABLE backup_runs; \
            ALTER TABLE backup_runs_new RENAME TO backup_runs;",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: ()) -> anyhow::Result<Self> {
        let mut stmt = conn.prepare(
            r#"
            SELECT key_id, access_structure_id, device_id, complete
            FROM backup_runs
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, ToStringWrapper<KeyId>>(0)?.0,
                row.get::<_, ToStringWrapper<AccessStructureId>>(1)?.0,
                row.get::<_, ToStringWrapper<DeviceId>>(2)?.0,
                row.get::<_, bool>(3)?,
            ))
        })?;

        let mut state = BackupState::default();

        for row in rows.into_iter() {
            let (key_id, access_structure_id, device_id, complete) = row?;
            let access_structure_ref = AccessStructureRef {
                key_id,
                access_structure_id,
            };
            state.apply_mutation(&Mutation::AddDeviceNeedsBackup {
                access_structure_ref,
                device_id,
            });
            if complete {
                state.apply_mutation(&Mutation::MarkDeviceComplete {
                    access_structure_ref,
                    device_id,
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
                Mutation::AddDeviceNeedsBackup {
                    access_structure_ref,
                    device_id,
                } => {
                    tx.execute(
                        r#"
                        INSERT INTO backup_runs (key_id, access_structure_id, device_id, complete)
                        VALUES (?1, ?2, ?3, ?4)
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                            ToStringWrapper(device_id),
                            false
                        ],
                    )?;
                }
                Mutation::MarkDeviceComplete {
                    access_structure_ref,
                    device_id,
                } => {
                    tx.execute(
                        r#"
                        UPDATE backup_runs
                        SET complete=?4
                        WHERE key_id=?1 AND access_structure_id=?2 AND device_id=?3
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                            ToStringWrapper(device_id),
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
