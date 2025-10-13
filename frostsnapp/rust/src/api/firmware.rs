use super::coordinator::Coordinator;
use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::Result;
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::firmware::*;
pub use frostsnap_coordinator::firmware_upgrade::FirmwareUpgradeConfirmState;
pub use frostsnap_coordinator::frostsnap_comms::Sha256Digest;
pub use frostsnap_coordinator::VersionNumber;
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

    #[frb(sync)]
    pub fn upgrade_firmware_version_name(&self) -> Option<String> {
        self.0.upgrade_firmware_version_name()
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
    pub abort: Option<String>,
    pub upgrade_ready_to_start: bool,
}

#[frb(mirror(VersionNumber))]
pub struct _VersionNumber {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

#[frb(mirror(FirmwareVersion))]
pub struct _FirmwareVersion {
    pub digest: Sha256Digest,
    pub version: Option<VersionNumber>,
}

#[frb(mirror(FirmwareUpgradeEligibility))]
pub enum _FirmwareUpgradeEligibility {
    UpToDate,
    CanUpgrade,
    CannotUpgrade { reason: String },
}

#[frb(external)]
impl VersionNumber {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}

#[frb(external)]
impl FirmwareVersion {
    #[frb(sync)]
    pub fn version_name(&self) -> String {}
}

#[frb(mirror(Sha256Digest))]
pub struct _Sha256Digest(pub [u8; 32]);

#[frb(external)]
impl Sha256Digest {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}
