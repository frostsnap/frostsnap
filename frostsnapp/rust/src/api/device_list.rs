use anyhow::Result;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::DeviceMode;
use frostsnap_core::DeviceId;

use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};

#[derive(Clone, Debug)]
pub enum DeviceListChangeKind {
    Added,
    Removed,
    Named,
    RecoveryMode,
}

#[derive(Clone, Debug)]
pub struct DeviceListChange {
    pub kind: DeviceListChangeKind,
    pub index: u32,
    pub device: ConnectedDevice,
}

#[derive(Clone, Debug)]
pub struct DeviceListUpdate {
    pub changes: Vec<DeviceListChange>,
    pub state: DeviceListState,
}

#[derive(Clone, Debug)]
pub struct DeviceListState {
    pub devices: Vec<ConnectedDevice>,
    pub state_id: u32,
}

#[derive(Clone, Debug)]
pub struct ConnectedDevice {
    pub name: Option<String>,
    // NOTE: digest should always be present in any device that is actually plugged in
    pub firmware_digest: String,
    pub latest_digest: Option<String>,
    pub id: DeviceId,
    pub recovery_mode: bool,
}

impl ConnectedDevice {
    #[frb(sync)]
    pub fn ready(&self) -> bool {
        self.name.is_some() && !self.needs_firmware_upgrade()
    }

    #[frb(sync)]
    pub fn needs_firmware_upgrade(&self) -> bool {
        // We still want to have this return true even when we don't have firmware in the app so we
        // know that the device needs a firmware upgrade (even if we can't give it to them).
        Some(self.firmware_digest.as_str()) != self.latest_digest.as_deref()
    }

    #[frb(ignore)]
    pub(crate) fn device_mode(&self) -> DeviceMode {
        if self.name.is_none() {
            DeviceMode::Blank
        } else if self.recovery_mode {
            DeviceMode::Recovery
        } else {
            DeviceMode::Ready
        }
    }
}

impl DeviceListState {
    #[frb(sync)]
    pub fn get_device(&self, id: DeviceId) -> Option<ConnectedDevice> {
        self.devices.iter().find(|device| device.id == id).cloned()
    }
}

impl super::coordinator::Coordinator {
    #[frb(sync)]
    pub fn device_at_index(&self, index: usize) -> Option<ConnectedDevice> {
        self.0.device_at_index(index)
    }

    #[frb(sync)]
    pub fn device_list_state(&self) -> DeviceListState {
        self.0.device_list_state()
    }

    pub fn sub_device_events(&self, sink: StreamSink<DeviceListUpdate>) -> Result<()> {
        self.0.sub_device_events(SinkWrap(sink));
        Ok(())
    }

    #[frb(sync)]
    pub fn get_connected_device(&self, id: DeviceId) -> Option<ConnectedDevice> {
        self.0.get_connected_device(id)
    }
}
