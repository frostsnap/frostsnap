//! The esp `FirmwareServices` implementation: OTA upgrades + genuine-challenge
//! signing. This is the device-only half the sim drops — the portable
//! `DeviceLoop` only sees the `FirmwareServices` trait.

use alloc::boxed::Box;
use esp_hal::{
    peripherals::TIMG0,
    rsa::Rsa,
    sha::Sha,
    timer::timg::{Timer as TimgTimer, Timer0},
    Blocking,
};
use frostsnap_comms::{
    genuine_certificate::Certificate, CoordinatorSendBody, CoordinatorUpgradeMessage,
    DeviceSendBody, Downstream, Sha256Digest, Upstream,
};
use frostsnap_embedded::{
    device_hal::{FirmwareAction, FirmwareServices},
    framed_serial::SerialPort,
    ui::UserInteraction,
};

use crate::ds::HardwareDs;
use crate::ota::{FirmwareUpgradeMode, OtaPartitions};
use crate::partitions::PartitionExt;

type EspTimer = TimgTimer<Timer0<TIMG0>, Blocking>;

pub struct EspFirmware<'a> {
    ota: OtaPartitions<'a>,
    sha256: Sha<'a>,
    rsa: Rsa<'a, Blocking>,
    ds: Option<HardwareDs<'a>>,
    certificate: Option<Certificate>,
    timer: &'a EspTimer,
    active_firmware_digest: Sha256Digest,
    upgrade: Option<FirmwareUpgradeMode<'a>>,
}

impl<'a> EspFirmware<'a> {
    pub fn new(
        ota: OtaPartitions<'a>,
        mut sha256: Sha<'a>,
        rsa: Rsa<'a, Blocking>,
        ds: Option<HardwareDs<'a>>,
        certificate: Option<Certificate>,
        timer: &'a EspTimer,
    ) -> Self {
        let active_partition = ota.active_partition();
        let (firmware_size, _firmware_and_signature_block_size) =
            active_partition.firmware_size().unwrap();
        let active_firmware_digest =
            active_partition.sha256_digest(&mut sha256, Some(firmware_size));

        Self {
            ota,
            sha256,
            rsa,
            ds,
            certificate,
            timer,
            active_firmware_digest,
            upgrade: None,
        }
    }
}

impl FirmwareServices for EspFirmware<'_> {
    fn firmware_digest(&self) -> Sha256Digest {
        self.active_firmware_digest
    }

    fn handle<U, D>(
        &mut self,
        msg: &CoordinatorSendBody,
        upstream: &mut U,
        downstream: Option<&mut D>,
        ui: &mut dyn UserInteraction,
    ) -> FirmwareAction
    where
        U: SerialPort<Upstream>,
        D: SerialPort<Downstream>,
    {
        match msg {
            CoordinatorSendBody::Upgrade(
                CoordinatorUpgradeMessage::PrepareUpgrade {
                    size,
                    firmware_digest,
                }
                | CoordinatorUpgradeMessage::PrepareUpgrade2 {
                    size,
                    firmware_digest,
                },
            ) => {
                self.upgrade = Some(self.ota.start_upgrade(
                    *size,
                    *firmware_digest,
                    self.active_firmware_digest,
                ));
                FirmwareAction::None
            }
            CoordinatorSendBody::Upgrade(CoordinatorUpgradeMessage::EnterUpgradeMode) => {
                let mut upgrade = self
                    .upgrade
                    .take()
                    .expect("upgrade cannot start because we were not warned about it");
                let downstream_raw = downstream.map(|d| d.raw());
                upgrade.enter_upgrade_mode(
                    upstream.raw(),
                    downstream_raw,
                    ui,
                    &mut self.sha256,
                    self.timer,
                    &mut self.rsa,
                );
                FirmwareAction::ResetRequested
            }
            CoordinatorSendBody::Challenge(challenge) => {
                if let (Some(hw_rsa), Some(cert)) = (self.ds.as_mut(), self.certificate.as_ref()) {
                    let signature = hw_rsa.sign(&challenge.0, &mut self.sha256);
                    FirmwareAction::Send(Box::new(DeviceSendBody::SignedChallenge {
                        signature: Box::new(signature),
                        certificate: Box::new(cert.clone()),
                    }))
                } else {
                    FirmwareAction::None
                }
            }
            _ => FirmwareAction::None,
        }
    }

    fn poll(&mut self, ui: &mut dyn UserInteraction) -> FirmwareAction {
        match self.upgrade.as_mut().and_then(|upgrade| upgrade.poll(ui)) {
            Some(body) => FirmwareAction::Send(Box::new(body)),
            None => FirmwareAction::None,
        }
    }

    fn confirm_upgrade(&mut self) {
        if let Some(upgrade) = self.upgrade.as_mut() {
            upgrade.upgrade_confirm();
        }
    }

    fn cancel(&mut self) {
        self.upgrade = None;
    }
}
