use crate::{Completion, FirmwareBin, Sink, UiProtocol};

use frostsnap_comms::{
    CommsMisc, CoordinatorSendBody, CoordinatorSendMessage, CoordinatorUpgradeMessage,
};
use frostsnap_core::DeviceId;
use std::collections::BTreeSet;

pub struct FirmwareUpgradeProtocol {
    state: FirmwareUpgradeConfirmState,
    sent_first_message: bool,
    firmware_bin: FirmwareBin,
    sink: Box<dyn Sink<FirmwareUpgradeConfirmState>>,
}

impl FirmwareUpgradeProtocol {
    pub fn new(
        devices: BTreeSet<DeviceId>,
        need_upgrade: BTreeSet<DeviceId>,
        firmware_bin: FirmwareBin,
        sink: impl Sink<FirmwareUpgradeConfirmState> + 'static,
    ) -> Self {
        Self {
            state: FirmwareUpgradeConfirmState {
                devices: devices.into_iter().collect(),
                need_upgrade: need_upgrade.into_iter().collect(),
                confirmations: Default::default(),
                abort: false,
                upgrade_ready_to_start: false,
            },
            sent_first_message: false,
            firmware_bin,
            sink: Box::new(sink),
        }
    }

    pub fn emit_state(&self) {
        self.sink.send(self.state.clone());
    }
}

impl UiProtocol for FirmwareUpgradeProtocol {
    fn cancel(&mut self) {
        self.state.abort = true;
        self.emit_state();
    }

    fn is_complete(&self) -> Option<Completion> {
        if BTreeSet::from_iter(self.state.confirmations.iter())
            == BTreeSet::from_iter(self.state.devices.iter())
        {
            Some(Completion::Success)
        } else if self.state.abort {
            Some(Completion::Abort {
                send_cancel_to_all_devices: true,
            })
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        if self.state.devices.contains(&id) {
            self.state.abort = true;
            self.emit_state();
            self.sink.close();
        }
    }

    fn process_comms_message(&mut self, from: DeviceId, message: CommsMisc) -> bool {
        if !self.state.devices.contains(&from) {
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
        if !self.sent_first_message {
            to_devices.push(CoordinatorSendMessage {
                target_destinations: frostsnap_comms::Destination::All,
                message_body: CoordinatorSendBody::Upgrade(
                    CoordinatorUpgradeMessage::PrepareUpgrade {
                        size: self.firmware_bin.size(),
                        firmware_digest: self.firmware_bin.cached_digest(),
                    },
                ),
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
    pub abort: bool,
    pub upgrade_ready_to_start: bool,
}
