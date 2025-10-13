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
            match firmware_bin.check_upgrade_eligibility(&fw.digest) {
                FirmwareUpgradeEligibility::CannotUpgrade { reason } => {
                    Some(format!("One of the devices is incompatible with the upgrade. Unplug it to continue. Problem: {reason}"))
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
        if BTreeSet::from_iter(self.state.confirmations.iter())
            == BTreeSet::from_iter(self.state.devices.iter())
        {
            Some(Completion::Success)
        } else if self.state.abort.is_some() {
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
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
            if !self.state.confirmations.contains(&from) {
                self.state.confirmations.push(from);
                self.emit_state()
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
