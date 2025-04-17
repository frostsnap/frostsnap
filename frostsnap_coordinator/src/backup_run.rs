use crate::persist::{Persist, ToStringWrapper};
use frostsnap_core::{AccessStructureId, AccessStructureRef, DeviceId, Gist, KeyId};
use rusqlite::params;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{event, Level};

#[derive(Default, Debug)]
pub struct BackupState {
    // Maps each access_structure_ref to a map of device_id -> timestamp.
    runs: BTreeMap<AccessStructureRef, BTreeMap<DeviceId, Option<u32>>>,
}

impl BackupState {
    fn apply_mutation(&mut self, mutation: &Mutation) -> bool {
        match mutation {
            Mutation::AddDeviceNeedsBackup {
                access_structure_ref,
                device_id,
            } => {
                let run = self.runs.entry(*access_structure_ref).or_default();
                run.insert(*device_id, None);
                true
            }
            Mutation::MarkDeviceComplete {
                access_structure_ref,
                device_id,
                timestamp,
            } => {
                if let Some(run) = self.runs.get_mut(access_structure_ref) {
                    if let Some(entry) = run.get_mut(device_id) {
                        *entry = Some(*timestamp);
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
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as u32,
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

    pub fn get_backup_run(&self, key_id: KeyId) -> BTreeMap<DeviceId, Option<u32>> {
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
        timestamp: u32,
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
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: ()) -> anyhow::Result<Self> {
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS backup_runs (
                key_id TEXT NOT NULL,
                access_structure_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                timestamp INTEGER,
                PRIMARY KEY (key_id, access_structure_id, device_id)
            )
            "#,
            [],
        )?;

        let mut stmt = conn.prepare(
            r#"
            SELECT key_id, access_structure_id, device_id, timestamp
            FROM backup_runs
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, ToStringWrapper<KeyId>>(0)?.0,
                row.get::<_, ToStringWrapper<AccessStructureId>>(1)?.0,
                row.get::<_, ToStringWrapper<DeviceId>>(2)?.0,
                row.get::<_, Option<u32>>(3)?,
            ))
        })?;

        let mut state = BackupState::default();

        for row in rows.into_iter() {
            let (key_id, access_structure_id, device_id, timestamp) = row?;
            let access_structure_ref = AccessStructureRef {
                key_id,
                access_structure_id,
            };
            state.apply_mutation(&Mutation::AddDeviceNeedsBackup {
                access_structure_ref,
                device_id,
            });
            if let Some(timestamp) = timestamp {
                state.apply_mutation(&Mutation::MarkDeviceComplete {
                    access_structure_ref,
                    device_id,
                    timestamp,
                });
            }
        }

        Ok(state)
    }

    fn persist_update(
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
                        INSERT INTO backup_runs (key_id, access_structure_id, device_id, timestamp)
                        VALUES (?1, ?2, ?3, ?4)
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                            ToStringWrapper(device_id),
                            None::<u32>
                        ],
                    )?;
                }
                Mutation::MarkDeviceComplete {
                    access_structure_ref,
                    device_id,
                    timestamp,
                } => {
                    tx.execute(
                        r#"
                        UPDATE backup_runs
                        SET timestamp=?4
                        WHERE key_id=?1 AND access_structure_id=?2 AND device_id=?3
                        "#,
                        params![
                            ToStringWrapper(access_structure_ref.key_id),
                            ToStringWrapper(access_structure_ref.access_structure_id),
                            ToStringWrapper(device_id),
                            Some(timestamp)
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
