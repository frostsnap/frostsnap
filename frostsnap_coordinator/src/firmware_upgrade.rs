use crate::{
    Completion, FirmwareUpgradeEligibility, FirmwareVersion, Sink, UiProtocol, ValidatedFirmwareBin,
};

use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorSendMessage, CoordinatorUpgradeMessage,
};
use frostsnap_core::DeviceId;
use std::collections::{BTreeSet, HashMap};

pub struct FirmwareUpgradeProtocol {
    state: FirmwareUpgradeConfirmState,
    sent_first_message: bool,
    firmware_bin: ValidatedFirmwareBin,
    devices: HashMap<DeviceId, FirmwareVersion>,
    /// Devices that are already on the target firmware auto-ack without a
    /// user prompt. We still need to wait for these acks before starting the
    /// flash (they signal the device is ready to forward bytes), but they
    /// don't belong in the user-facing confirmation count.
    passive_acks: BTreeSet<DeviceId>,
    sink: Box<dyn Sink<FirmwareUpgradeConfirmState>>,
}

impl FirmwareUpgradeProtocol {
    pub fn new(
        devices: HashMap<DeviceId, FirmwareVersion>,
        need_upgrade: BTreeSet<DeviceId>,
        firmware_bin: ValidatedFirmwareBin,
        sink: impl Sink<FirmwareUpgradeConfirmState> + 'static,
    ) -> Self {
        // Check if any device has incompatible firmware
        let abort_reason = devices.values().find_map(|fw| {
            match firmware_bin.firmware_version().check_upgrade_eligibility(&fw.digest) {
                FirmwareUpgradeEligibility::CannotUpgrade { reason } => {
                    Some(format!("One of the devices is incompatible with the upgrade. Unplug it to continue or try upgrading the app. Problem: {reason}"))
                }
                _ => None,
            }
        });

        Self {
            state: FirmwareUpgradeConfirmState {
                devices: devices.keys().copied().collect(),
                need_upgrade: need_upgrade.into_iter().collect(),
                confirmations: Default::default(),
                abort: abort_reason,
                upgrade_ready_to_start: false,
            },
            sent_first_message: false,
            firmware_bin,
            devices,
            passive_acks: BTreeSet::new(),
            sink: Box::new(sink),
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }
}

impl UiProtocol for FirmwareUpgradeProtocol {
    fn cancel(&mut self) {
        self.state.abort = Some("canceled".to_string());
        self.emit_state();
    }

    fn is_complete(&self) -> Option<Completion> {
        // Abort wins — a passive device could auto-ack and then disconnect on
        // the same tick, leaving `passive_acks` populated while `aborted` is
        // also set. Succeeding there would skip the cancel-to-all cleanup.
        if self.state.abort.is_some() {
            return Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            });
        }
        // All upgrade targets must have user-confirmed, AND every passive
        // (already-up-to-date) device must have auto-ack'd so we know it's
        // ready to forward bytes during the flash phase.
        let targets_confirmed = BTreeSet::from_iter(self.state.confirmations.iter())
            == BTreeSet::from_iter(self.state.need_upgrade.iter());
        let passive_ready = self
            .devices
            .keys()
            .filter(|id| !self.state.need_upgrade.contains(id))
            .all(|id| self.passive_acks.contains(id));
        if targets_confirmed && passive_ready {
            Some(Completion::Success)
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        if self.devices.contains_key(&id) {
            self.state.abort = Some("Device disconnected during upgrade".to_string());
            self.emit_state();
        }
    }

    fn process_comms_message(&mut self, from: DeviceId, message: CommsMisc) -> bool {
        if !self.devices.contains_key(&from) {
            return false;
        }
        if let CommsMisc::AckUpgradeMode = message {
            if self.state.need_upgrade.contains(&from) {
                if !self.state.confirmations.contains(&from) {
                    self.state.confirmations.push(from);
                    self.emit_state();
                }
            } else {
                // Passive ack: tracked internally so `is_complete` can gate
                // on it, but kept out of the user-facing confirmation count.
                self.passive_acks.insert(from);
            }
            true
        } else {
            false
        }
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        let mut to_devices = vec![];
        if !self.sent_first_message && self.state.abort.is_none() {
            let any_device_needs_legacy = self
                .devices
                .values()
                .any(|fw| !fw.features().upgrade_digest_no_sig);

            let upgrade_message = if any_device_needs_legacy {
                CoordinatorUpgradeMessage::PrepareUpgrade {
                    size: self.firmware_bin.size(),
                    firmware_digest: self.firmware_bin.digest_with_signature(),
                }
            } else {
                CoordinatorUpgradeMessage::PrepareUpgrade2 {
                    size: self.firmware_bin.size(),
                    firmware_digest: self.firmware_bin.digest(),
                }
            };

            to_devices.push(CoordinatorSendMessage {
                target_destinations: frostsnap_comms::Destination::All,
                message_body: CoordinatorSendBody::Upgrade(upgrade_message),
            });
            self.sent_first_message = true;
        }

        // we only want to emit te ready state after we've been polled so coordinator loop has a
        // chance to clean up this protocol.
        if matches!(self.is_complete(), Some(Completion::Success)) {
            self.state.upgrade_ready_to_start = true;
            self.emit_state()
        }

        to_devices
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[derive(Clone, Debug)]
pub struct FirmwareUpgradeConfirmState {
    pub confirmations: Vec<DeviceId>,
    pub devices: Vec<DeviceId>,
    pub need_upgrade: Vec<DeviceId>,
    pub abort: Option<String>,
    pub upgrade_ready_to_start: bool,
}
