use anyhow::Result;
use frostsnap_core::{DeviceId, KeyId};
use tracing::{event, Level};

use crate::frb_generated::StreamSink;

#[derive(Debug, Clone, PartialEq)]
pub struct BackupDevice {
    pub device_id: DeviceId,
    pub device_name: String,
    pub share_index: u32,
    pub complete: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BackupRun {
    pub devices: Vec<BackupDevice>,
}

impl BackupRun {
    pub fn is_run_complete(&self) -> bool {
        self.devices
            .iter()
            .all(|device| device.complete == Some(true))
    }
}

impl crate::api::coordinator::Coordinator {
    pub fn mark_backup_complete(&self, key_id: KeyId, device_id: DeviceId) -> Result<()> {
        let mut backup_state = self.0.backup_state.lock().unwrap();
        let mut db = self.0.db.lock().unwrap();

        backup_state.mutate2(&mut *db, |state, mutations| {
            state.mark_backup_complete(key_id, device_id, mutations);
            Ok(())
        })?;

        drop(db);
        drop(backup_state);

        self.0.backup_stream_emit(key_id)?;
        Ok(())
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn get_backup_run(&self, key_id: KeyId) -> BackupRun {
        self.0.build_backup_run(key_id)
    }

    pub fn backup_stream(&self, key_id: KeyId, stream: StreamSink<BackupRun>) -> Result<()> {
        event!(
            Level::DEBUG,
            key_id = key_id.to_string(),
            "backup stream subscribed"
        );

        if self
            .0
            .backup_run_streams
            .lock()
            .unwrap()
            .insert(key_id, stream)
            .is_some()
        {
            event!(
                Level::WARN,
                "backup stream was replaced this is probably a bug"
            );
        }

        self.0.backup_stream_emit(key_id)?;
        Ok(())
    }
}
