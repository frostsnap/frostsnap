use crate::persist::Persist;
use frostsnap_core::{DeviceId, KeyId};
use rusqlite::params;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, bincode::Encode, bincode::Decode, Default)]
pub struct BackupRun {
    pub devices: Vec<(DeviceId, Option<u32>)>,
}

impl BackupRun {
    pub fn new(devices: Vec<DeviceId>) -> Self {
        Self {
            devices: devices.into_iter().map(|d| (d, None)).collect(),
        }
    }

    pub fn mark_device_complete(&mut self, device_id: DeviceId) {
        if let Some(entry) = self.devices.iter_mut().find(|(d, _)| *d == device_id) {
            entry.1 = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as u32,
            );
        }
    }

    pub fn is_run_complete(&self) -> bool {
        self.devices
            .iter()
            .all(|(_, timestamp)| timestamp.is_some())
    }
}

#[derive(Default, Debug)]
pub struct BackupState {
    pub runs: BTreeMap<KeyId, BackupRun>,
}

#[derive(Debug, Clone)]
pub enum BackupMutation {
    StartBackup {
        key_id: KeyId,
        devices: Vec<DeviceId>,
    },
    MarkDeviceComplete {
        key_id: KeyId,
        device_id: DeviceId,
        timestamp: u32,
    },
}

impl Persist<rusqlite::Connection> for BackupState {
    type Update = Vec<BackupMutation>;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: ()) -> anyhow::Result<Self> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS backup_runs (
                key_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                timestamp INTEGER,
                PRIMARY KEY (key_id, device_id)
            )",
            [],
        )?;

        let mut stmt = conn.prepare(
            "SELECT key_id, device_id, timestamp 
             FROM backup_runs 
             ORDER BY key_id",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,      // key_id
                row.get::<_, String>(1)?,      // device_id
                row.get::<_, Option<u32>>(2)?, // timestamp
            ))
        })?;

        // group by key_id and build BackupRun for each group
        let runs = rows
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .fold(
                BTreeMap::new(),
                |mut map: BTreeMap<_, Vec<_>>, (key_id, device_id, timestamp)| {
                    map.entry(key_id).or_default().push((device_id, timestamp));
                    map
                },
            )
            .into_iter()
            .map(|(key_id, devices)| {
                let devices = devices
                    .into_iter()
                    .map(|(device_id, timestamp)| Ok((device_id.parse()?, timestamp)))
                    .collect::<Result<Vec<_>, anyhow::Error>>()?;

                Ok((key_id.parse()?, BackupRun { devices }))
            })
            .collect::<Result<BTreeMap<_, _>, anyhow::Error>>()?;

        Ok(BackupState { runs })
    }

    fn persist_update(
        conn: &mut rusqlite::Connection,
        update: Vec<BackupMutation>,
    ) -> anyhow::Result<()> {
        for mutation in update {
            match mutation {
                BackupMutation::StartBackup { key_id, devices } => {
                    // Delete any previous backup run
                    conn.execute(
                        "DELETE FROM backup_runs WHERE key_id = ?1",
                        params![key_id.to_string()],
                    )?;

                    for device_id in devices {
                        conn.execute(
                            "INSERT OR REPLACE INTO backup_runs (key_id, device_id, timestamp) 
                             VALUES (?1, ?2, ?3)",
                            params![key_id.to_string(), device_id.to_string(), None::<u32>],
                        )?;
                    }
                }
                BackupMutation::MarkDeviceComplete {
                    key_id,
                    device_id,
                    timestamp,
                } => {
                    conn.execute(
                        "INSERT OR REPLACE INTO backup_runs (key_id, device_id, timestamp) 
                         VALUES (?1, ?2, ?3)",
                        params![key_id.to_string(), device_id.to_string(), Some(timestamp)],
                    )?;
                }
            }
        }
        Ok(())
    }
}
