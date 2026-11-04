use crate::{
    Completion, DeviceFirmware, FirmwareUpgradeEligibility, Sink, UiProtocol, ValidatedFirmwareBin,
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
    devices: HashMap<DeviceId, DeviceFirmware>,
    /// Devices that are already on the target firmware auto-ack without a
    /// user prompt. We still need to wait for these acks before starting the
    /// flash (they signal the device is ready to forward bytes), but they
    /// don't belong in the user-facing confirmation count.
    passive_acks: BTreeSet<DeviceId>,
    sink: Box<dyn Sink<FirmwareUpgradeConfirmState>>,
}

impl FirmwareUpgradeProtocol {
    pub fn new(
        devices: HashMap<DeviceId, DeviceFirmware>,
        firmware_bin: ValidatedFirmwareBin,
        sink: impl Sink<FirmwareUpgradeConfirmState> + 'static,
    ) -> Self {
        // Check if any device has incompatible firmware
        let abort_reason = devices.values().find_map(|fw| {
            match firmware_bin.firmware_version().check_upgrade_eligibility(fw) {
                FirmwareUpgradeEligibility::CannotUpgrade { reason } => {
                    Some(format!("One of the devices is incompatible with the upgrade. Unplug it to continue or try upgrading the app. Problem: {reason}"))
                }
                FirmwareUpgradeEligibility::UpToDate | FirmwareUpgradeEligibility::CanUpgrade => {
                    None
                }
            }
        });

        // A device forwards passively exactly when the image is the one it runs
        // (`start_upgrade` on the device); every other device prompts to install.
        let need_upgrade = devices
            .iter()
            .filter(|(_, fw)| fw.digest != firmware_bin.digest())
            .map(|(id, _)| *id)
            .collect();

        Self {
            state: FirmwareUpgradeConfirmState {
                devices: devices.keys().copied().collect(),
                need_upgrade,
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
            let mut legacy = BTreeSet::new();
            let mut digest_only = BTreeSet::new();
            let mut names_version = BTreeSet::new();
            for (id, firmware) in &self.devices {
                let features = firmware.features();
                if !features.upgrade_digest_no_sig {
                    legacy.insert(*id);
                } else if features.upgrade_names_version {
                    names_version.insert(*id);
                } else {
                    digest_only.insert(*id);
                }
            }

            let size = self.firmware_bin.size();
            let firmware_digest = self.firmware_bin.digest();
            let messages = [
                (
                    legacy,
                    CoordinatorUpgradeMessage::PrepareUpgrade {
                        size,
                        firmware_digest: self.firmware_bin.digest_with_signature(),
                    },
                ),
                (
                    digest_only,
                    CoordinatorUpgradeMessage::PrepareUpgrade2 {
                        size,
                        firmware_digest,
                    },
                ),
                (
                    names_version,
                    CoordinatorUpgradeMessage::PrepareUpgrade3 {
                        size,
                        firmware_digest,
                        version: self.firmware_bin.version(),
                    },
                ),
            ];
            for (recipients, message) in messages {
                if !recipients.is_empty() {
                    to_devices.push(CoordinatorSendMessage {
                        target_destinations: frostsnap_comms::Destination::Particular(recipients),
                        message_body: CoordinatorSendBody::Upgrade(message),
                    });
                }
            }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{DeviceVersion, FirmwareBin, VersionNumber};
    use frostsnap_comms::firmware_reader::{APP_DESC_MAGIC, APP_DESC_OFFSET, ESP_MAGIC};
    use frostsnap_comms::Sha256Digest;
    use frostsnap_macros::hex;

    fn release_0_5_0_image() -> ValidatedFirmwareBin {
        const SEGMENT_LEN: usize = 0x100;
        let unpadded = APP_DESC_OFFSET + SEGMENT_LEN;
        let mut image = vec![0u8; (unpadded + 1 + 15) & !15];
        image[0] = ESP_MAGIC;
        image[1] = 1;
        image[28..32].copy_from_slice(&(SEGMENT_LEN as u32).to_le_bytes());
        image[APP_DESC_OFFSET..APP_DESC_OFFSET + 4].copy_from_slice(&APP_DESC_MAGIC.to_le_bytes());
        image[APP_DESC_OFFSET + 16..APP_DESC_OFFSET + 21].copy_from_slice(b"0.5.0");
        let bin = FirmwareBin::new(Box::leak(image.into_boxed_slice()));
        let bin = bin.validate().unwrap();
        assert_eq!(bin.version(), VersionNumber::new(0, 5, 0));
        bin
    }

    fn device(n: u8) -> DeviceId {
        DeviceId([n; 33])
    }

    fn ack(protocol: &mut FirmwareUpgradeProtocol, id: DeviceId) {
        assert!(protocol.process_comms_message(id, CommsMisc::AckUpgradeMode));
    }

    #[test]
    fn only_the_same_image_forwards_passively() {
        let bin = release_0_5_0_image();
        let same_image = device(1);
        let same_release_other_build = device(2);
        let older = device(3);
        let devices = HashMap::from([
            (
                same_image,
                DeviceFirmware {
                    digest: bin.digest(),
                    version: DeviceVersion::Reported {
                        version: bin.version(),
                        environment: bin.firmware_version().environment,
                    },
                },
            ),
            (
                same_release_other_build,
                DeviceFirmware {
                    digest: Sha256Digest([2; 32]),
                    version: DeviceVersion::Reported {
                        version: bin.version(),
                        environment: bin.firmware_version().environment,
                    },
                },
            ),
            (
                older,
                DeviceFirmware {
                    digest: Sha256Digest([3; 32]),
                    version: DeviceVersion::Unreported,
                },
            ),
        ]);

        let mut protocol = FirmwareUpgradeProtocol::new(devices, bin, ());
        assert_eq!(protocol.state.abort, None);
        assert_eq!(
            BTreeSet::from_iter(protocol.state.need_upgrade.iter().copied()),
            BTreeSet::from([same_release_other_build, older])
        );

        ack(&mut protocol, same_release_other_build);
        ack(&mut protocol, older);
        assert!(
            protocol.is_complete().is_none(),
            "the passive device hasn't acked"
        );
        assert_eq!(protocol.state.confirmations.len(), 2);

        ack(&mut protocol, same_image);
        assert_eq!(protocol.state.confirmations.len(), 2);
        assert!(matches!(protocol.is_complete(), Some(Completion::Success)));
    }

    #[test]
    fn each_device_is_sent_the_preparation_it_understands() {
        let bin = release_0_5_0_image();
        let v0_0_1 = device(1);
        let v0_4_0 = device(2);
        let reporting = device(3);
        let devices = HashMap::from([
            (
                v0_0_1,
                DeviceFirmware {
                    digest: Sha256Digest(hex!(
                        "8f45ae6b72c241a20798acbd3c6d3e54071cae73e335df1785f2d485a915da4c"
                    )),
                    version: DeviceVersion::Unreported,
                },
            ),
            (
                v0_4_0,
                DeviceFirmware {
                    digest: Sha256Digest(hex!(
                        "037c345c9db9866d30e219ae56de5efe3905a4050a82df362590d5017e709df0"
                    )),
                    version: DeviceVersion::Unreported,
                },
            ),
            (
                reporting,
                DeviceFirmware {
                    digest: Sha256Digest([3; 32]),
                    version: DeviceVersion::Reported {
                        version: bin.version(),
                        environment: bin.firmware_version().environment,
                    },
                },
            ),
        ]);

        let mut protocol = FirmwareUpgradeProtocol::new(devices, bin, ());
        let sent = protocol.poll();
        assert_eq!(sent.len(), 3);
        for message in sent {
            let frostsnap_comms::Destination::Particular(recipients) = message.target_destinations
            else {
                panic!("each preparation names its recipients");
            };
            let CoordinatorSendBody::Upgrade(upgrade) = message.message_body else {
                panic!("expected an upgrade message");
            };
            match upgrade {
                CoordinatorUpgradeMessage::PrepareUpgrade {
                    firmware_digest, ..
                } => {
                    assert_eq!(recipients, BTreeSet::from([v0_0_1]));
                    assert_eq!(firmware_digest, bin.digest_with_signature());
                }
                CoordinatorUpgradeMessage::PrepareUpgrade2 {
                    firmware_digest, ..
                } => {
                    assert_eq!(recipients, BTreeSet::from([v0_4_0]));
                    assert_eq!(firmware_digest, bin.digest());
                }
                CoordinatorUpgradeMessage::PrepareUpgrade3 {
                    firmware_digest,
                    version,
                    ..
                } => {
                    assert_eq!(recipients, BTreeSet::from([reporting]));
                    assert_eq!(firmware_digest, bin.digest());
                    assert_eq!(version, bin.version());
                }
                CoordinatorUpgradeMessage::EnterUpgradeMode => panic!("not yet"),
            }
        }
        assert!(protocol.poll().is_empty(), "prepared once");
    }
}
