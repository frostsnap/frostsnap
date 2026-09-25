pub use crate::api::firmware::{FirmwareUpgradeEligibility, FirmwareVersion};
use anyhow::Result;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::DeviceMode;
use frostsnap_core::{AccessStructureRef, DeviceId};

use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};

#[derive(Clone, Debug)]
pub enum DeviceListChangeKind {
    Added,
    Removed,
    Named,
    RecoveryMode,
    /// The device's genuine status or case colour changed.
    GenuineCheck,
}

/// The case colour from a verified certificate. A certificate on file does not make the device in
/// front of the user genuine; only [`GenuineStatus::Genuine`] does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseColor {
    Black,
    Orange,
    Silver,
    Blue,
    Red,
}

impl CaseColor {
    /// `None` for a colour this build has no name for: no colour beats the wrong one, since the
    /// user matches it against the device in their hand.
    #[frb(ignore)]
    pub fn from_comms(
        color: frostsnap_coordinator::frostsnap_comms::genuine_certificate::CaseColor,
    ) -> Option<Self> {
        use frostsnap_coordinator::frostsnap_comms::genuine_certificate::CaseColor as C;
        Some(match color {
            C::Black => CaseColor::Black,
            C::Orange => CaseColor::Orange,
            C::Silver => CaseColor::Silver,
            C::Blue => CaseColor::Blue,
            C::Red => CaseColor::Red,
            _ => return None,
        })
    }
}

/// See `docs/genuine-check-design.md`. A failed proof is never shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenuineStatus {
    Unattested {
        firmware_supports_check: bool,
    },
    Attested {
        firmware_supports_check: bool,
    },
    /// Proven for this connection.
    Genuine,
}

impl GenuineStatus {
    #[frb(ignore)]
    pub fn from_coordinator(
        status: frostsnap_coordinator::genuine_check::GenuineStatus,
    ) -> (Self, Option<CaseColor>) {
        use frostsnap_coordinator::genuine_check::GenuineStatus as S;
        match status {
            S::Unattested {
                firmware_supports_check,
            } => (
                GenuineStatus::Unattested {
                    firmware_supports_check,
                },
                None,
            ),
            S::Attested {
                certificate,
                firmware_supports_check,
            } => (
                GenuineStatus::Attested {
                    firmware_supports_check,
                },
                CaseColor::from_comms(certificate.case_color()),
            ),
            S::Genuine { certificate } => (
                GenuineStatus::Genuine,
                CaseColor::from_comms(certificate.case_color()),
            ),
        }
    }
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

/// Whether a device is in recovery mode, and what it still has outstanding. The two are not
/// independent — pending consolidations only exist in recovery — so they are one value rather
/// than a bool beside a list that could disagree with it.
#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryMode {
    Off,
    On {
        /// Wallets with DEFERRED consolidation work — the coordinator's
        /// `pending_physical_consolidations`, i.e. work left over from restorations that have
        /// already finished. Non-empty is the re-plug discovery the background auto-exit acts
        /// on, resolving each wallet's key separately.
        ///
        /// EMPTY IS NORMAL, and it does **not** mean the device has nothing outstanding: an
        /// app-driven restoration can still have consolidation in flight via
        /// `tmp_waiting_consolidate`. It means only that there is no *deferred* work for the
        /// background auto-exit to pick up, because the active flow owns whatever it has in
        /// progress and takes the device back out of recovery itself.
        pending: Vec<AccessStructureRef>,
    },
}

#[derive(Clone, Debug)]
pub struct ConnectedDevice {
    pub name: Option<String>,
    pub firmware: FirmwareVersion,
    pub latest_firmware: Option<FirmwareVersion>,
    pub id: DeviceId,
    pub recovery_mode: RecoveryMode,
    /// `None` until we learn it, or when the device claims a colour this build
    /// doesn't know.
    pub case_color: Option<CaseColor>,
    pub genuine: GenuineStatus,
}

impl ConnectedDevice {
    #[frb(sync)]
    pub fn ready(&self) -> bool {
        self.name.is_some() && self.firmware_is_up_to_date()
    }

    /// Whether an upgrade is available *and* possible.
    ///
    /// False both when the device is current and when nothing can be done —
    /// no firmware bundled in the app, or a device newer than the app knows
    /// about. Those are not the same as "up to date", but offering the user an
    /// upgrade that would be refused is worse than saying nothing, so they are
    /// the same answer here. Callers that mean "is this device current" want
    /// [`firmware_is_up_to_date`] instead.
    #[frb(sync)]
    pub fn needs_firmware_upgrade(&self) -> bool {
        matches!(
            self.firmware_upgrade_eligibility(),
            FirmwareUpgradeEligibility::CanUpgrade
        )
    }

    #[frb(ignore)]
    pub(crate) fn firmware_is_up_to_date(&self) -> bool {
        matches!(
            self.firmware_upgrade_eligibility(),
            FirmwareUpgradeEligibility::UpToDate
        )
    }

    #[frb(sync)]
    pub fn firmware_upgrade_eligibility(&self) -> FirmwareUpgradeEligibility {
        let Some(latest_firmware) = &self.latest_firmware else {
            return FirmwareUpgradeEligibility::CannotUpgrade {
                reason: "No firmware available in app".to_string(),
            };
        };

        latest_firmware.check_upgrade_eligibility(&self.firmware.digest)
    }

    #[frb(ignore)]
    pub(crate) fn device_mode(&self) -> DeviceMode {
        if self.name.is_none() {
            DeviceMode::Blank
        } else if matches!(self.recovery_mode, RecoveryMode::On { .. }) {
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
