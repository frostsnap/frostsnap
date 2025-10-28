use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::{backup_run::BackupState, persist::Persisted};
use frostsnap_core::{DeviceId, KeyId};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{event, Level};

use crate::frb_generated::StreamSink;

#[frb(opaque)]
pub struct BackupManager {
    backup_state: Persisted<BackupState>,
    backup_run_streams: BTreeMap<KeyId, StreamSink<BackupRun>>,
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl BackupManager {
    pub(crate) fn new(db: Arc<Mutex<rusqlite::Connection>>) -> anyhow::Result<Self> {
        let persisted_backup_state: Persisted<BackupState> = {
            let mut db_ = db.lock().unwrap();
            Persisted::new(&mut *db_, ())?
        };

        Ok(Self {
            db,
            backup_state: persisted_backup_state,
            backup_run_streams: Default::default(),
        })
    }

    pub fn start_backup_run(
        &mut self,
        access_structure: &super::coordinator::AccessStructure,
    ) -> Result<()> {
        let access_structure_ref = access_structure.access_structure_ref();
        {
            let mut db = self.db.lock().unwrap();
            self.backup_state.mutate2(&mut *db, |state, mutations| {
                let devices = access_structure.devices().collect();
                state.start_run(access_structure_ref, devices, mutations);
                Ok(())
            })?;
        }

        self.backup_stream_emit(access_structure_ref.key_id)?;
        Ok(())
    }

    pub fn mark_backup_complete(&mut self, key_id: KeyId, device_id: DeviceId) -> Result<()> {
        {
            let mut db = self.db.lock().unwrap();
            self.backup_state.mutate2(&mut *db, |state, mutations| {
                state.mark_backup_complete(key_id, device_id, mutations);
                Ok(())
            })?;
        }
        self.backup_stream_emit(key_id)?;
        Ok(())
    }

    #[frb(sync)]
    pub fn get_backup_run(
        &self,
        key_id: KeyId,
        access_structure: &super::coordinator::AccessStructure,
    ) -> BackupRun {
        let backup_run = self.backup_state.get_backup_run(key_id);
        let devices = backup_run
            .into_iter()
            .map(|(device_id, timestamp)| {
                let device_name = access_structure
                    .coordinator()
                    .get_device_name(device_id)
                    .unwrap_or_default();
                let share_index = access_structure.get_device_short_share_index(device_id);
                BackupDevice {
                    device_id,
                    device_name,
                    share_index,
                    timestamp,
                }
            })
            .collect();

        BackupRun { devices }
    }

    pub fn backup_stream(
        &mut self,
        key_id: KeyId,
        new_stream: StreamSink<BackupRun>,
    ) -> Result<()> {
        event!(
            Level::DEBUG,
            key_id = key_id.to_string(),
            "backup stream subscribed"
        );
        {
            if self.backup_run_streams.insert(key_id, new_stream).is_some() {
                event!(
                    Level::WARN,
                    "backup stream was replaced this is probably a bug"
                );
            }
        }
        self.backup_stream_emit(key_id)?;
        Ok(())
    }

    pub fn backup_stream_emit(&self, key_id: KeyId) -> Result<()> {
        self.backup_run_streams
            .get(&key_id)
            .ok_or(anyhow!("no backup stream found for key: {}", key_id))?
            .add(self.get_backup_run(key_id))
            .unwrap();
        Ok(())
    }

    #[frb(sync)]
    pub fn should_quick_backup_warn(&self, key_id: KeyId, device_id: DeviceId) -> bool {
        let too_fast_warning_period = 5 * 60;
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;

        let backup_run = self.backup_state.get_backup_run(key_id);

        let most_recent = backup_run
            .iter()
            .filter_map(|(dev_id, time)| time.map(|t| (*dev_id, t)))
            .max_by_key(|&(_, time)| time);

        match most_recent {
            Some((last_device_id, timestamp)) => {
                let elapsed = current_time - timestamp;
                elapsed < too_fast_warning_period && last_device_id != device_id
            }
            None => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackupDevice {
    pub device_id: DeviceId,
    pub device_name: String,
    pub share_index: Option<u8>,
    pub timestamp: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BackupRun {
    pub devices: Vec<BackupDevice>,
}

impl BackupRun {
    pub fn is_run_complete(&self) -> bool {
        self.devices
            .iter()
            .all(|device| device.timestamp.is_some())
    }
}
