use super::coordinator::Coordinator;
use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::firmware_upgrade::FirmwareUpgradeConfirmState;
use frostsnap_core::DeviceId;

impl Coordinator {
    pub fn start_firmware_upgrade(
        &self,
        sink: StreamSink<FirmwareUpgradeConfirmState>,
    ) -> Result<()> {
        self.0.begin_upgrade_firmware(SinkWrap(sink))?;
        Ok(())
    }

    #[frb(sync)]
    pub fn upgrade_firmware_digest(&self) -> Option<String> {
        self.0
            .upgrade_firmware_digest()
            .map(|digest| digest.to_string())
    }

    pub fn enter_firmware_upgrade_mode(&self, progress: StreamSink<f32>) -> Result<()> {
        self.0.enter_firmware_upgrade_mode(SinkWrap(progress))
    }
}

#[frb(mirror(FirmwareUpgradeConfirmState), unignore)]
pub struct _FirmwareUpgradeConfirmState {
    pub confirmations: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub need_upgrade: Vec<DeviceId>,
    pub abort: bool,
    pub upgrade_ready_to_start: bool,
}
