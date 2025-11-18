use anyhow::Result;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::display_backup::DisplayBackupState;
use frostsnap_core::{AccessStructureRef, DeviceId, KeyId, SymmetricKey};
use tracing::{event, Level};

use crate::frb_generated::StreamSink;
use crate::sink_wrap::SinkWrap;

#[frb(mirror(DisplayBackupState))]
pub struct _DisplayBackupState {
    pub confirmed: bool,
    pub close_dialog: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackupDevice {
    pub device_id: DeviceId,
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
    pub fn display_backup(
        &self,
        id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
        sink: StreamSink<DisplayBackupState>,
    ) -> Result<()> {
        use frostsnap_coordinator::display_backup::DisplayBackupProtocol;

        let backup_protocol = DisplayBackupProtocol::new(
            self.0.coordinator.lock().unwrap().MUTATE_NO_PERSIST(),
            id,
            access_structure_ref,
            encryption_key,
            SinkWrap(sink),
        )?;

        self.0.start_protocol(backup_protocol);

        Ok(())
    }

    pub fn mark_backup_complete(
        &self,
        access_structure_ref: AccessStructureRef,
        share_index: u32,
    ) -> Result<()> {
        let mut backup_state = self.0.backup_state.lock().unwrap();
        let mut db = self.0.db.lock().unwrap();

        backup_state.mutate2(&mut *db, |state, mutations| {
            state.mark_backup_complete(access_structure_ref, share_index, mutations);
            Ok(())
        })?;

        drop(db);
        drop(backup_state);

        self.0.backup_stream_emit(access_structure_ref.key_id)?;
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
