use frostsnap_comms::{CoordinatorSendBody, CoordinatorSendMessage};
use frostsnap_core::{
    coordinator::{
        restoration::{self, RecoverShare},
        CoordinatorToUserMessage,
    },
    message::{CoordinatorRestoration, CoordinatorToDeviceMessage},
    DeviceId,
};
use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::{DeviceMode, Sink, UiProtocol};

// ⏱️ Devices plugged in together connect in rapid succession - wait to detect all before emitting state
const DEVICE_SETTLE_TIME_MS: u64 = 800;

struct DeviceInfo {
    id: DeviceId,
    mode: DeviceMode,
    shares: Vec<RecoverShare>,
}

pub struct WaitForSingleDevice {
    sent_request_to: HashSet<DeviceId>,
    devices: Vec<DeviceInfo>,
    sink: Box<dyn Sink<WaitForSingleDeviceState>>,
    abort: bool,
    last_change: Option<Instant>,
    last_emitted_state: Option<WaitForSingleDeviceState>,
    finished: bool,
}

impl WaitForSingleDevice {
    pub fn new(sink: impl Sink<WaitForSingleDeviceState>) -> Self {
        Self {
            sent_request_to: Default::default(),
            devices: Default::default(),
            sink: Box::new(sink),
            abort: false,
            last_change: None,
            last_emitted_state: None,
            finished: false,
        }
    }

    fn compute_state(&self) -> WaitForSingleDeviceState {
        match self.devices.len() {
            0 => WaitForSingleDeviceState::NoDevice,
            1 => {
                let device = &self.devices[0];
                match device.mode {
                    DeviceMode::Blank => WaitForSingleDeviceState::BlankDevice {
                        device_id: device.id,
                    },
                    _ => {
                        if let Some(share) = device.shares.first() {
                            WaitForSingleDeviceState::DeviceWithShare {
                                device_id: device.id,
                                share: share.clone(),
                            }
                        } else {
                            WaitForSingleDeviceState::WaitingForDevice {
                                device_id: device.id,
                            }
                        }
                    }
                }
            }
            _ => WaitForSingleDeviceState::TooManyDevices,
        }
    }

    pub fn emit_state(&mut self) {
        let state = self.compute_state();
        let found_device = matches!(
            state,
            WaitForSingleDeviceState::BlankDevice { .. }
                | WaitForSingleDeviceState::DeviceWithShare { .. }
        );
        if found_device && !self.finished {
            self.finished = true;
        }
        self.sink.send(state.clone());
        self.last_emitted_state = Some(state);
    }

    fn mark_changed(&mut self) {
        self.last_change = Some(Instant::now());
    }
}

impl UiProtocol for WaitForSingleDevice {
    fn cancel(&mut self) {
        self.abort = true;
    }

    fn is_complete(&self) -> Option<crate::Completion> {
        if self.abort {
            Some(crate::Completion::Abort {
                send_cancel_to_all_devices: false,
            })
        } else if self.finished {
            Some(crate::Completion::Success)
        } else {
            None
        }
    }

    fn disconnected(&mut self, id: DeviceId) {
        self.devices.retain(|device| device.id != id);
        self.sent_request_to.remove(&id);

        self.mark_changed();
    }

    fn poll(&mut self) -> Vec<CoordinatorSendMessage> {
        let mut out = vec![];

        for device in &self.devices {
            if device.mode != DeviceMode::Blank && !self.sent_request_to.contains(&device.id) {
                out.push(CoordinatorSendMessage::to(
                    device.id,
                    CoordinatorSendBody::Core(CoordinatorToDeviceMessage::Restoration(
                        CoordinatorRestoration::RequestHeldShares,
                    )),
                ));
                self.sent_request_to.insert(device.id);
            }
        }

        if let Some(last_change) = self.last_change {
            if last_change.elapsed() >= Duration::from_millis(DEVICE_SETTLE_TIME_MS) {
                let current_state = self.compute_state();
                if self.last_emitted_state.as_ref() != Some(&current_state) {
                    self.emit_state();
                }
                self.last_change = None;
            }
        }

        out
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn connected(&mut self, id: DeviceId, mode: DeviceMode) {
        self.devices.push(DeviceInfo {
            id,
            mode,
            shares: vec![],
        });
        self.mark_changed();
    }

    fn process_to_user_message(&mut self, message: CoordinatorToUserMessage) -> bool {
        if let CoordinatorToUserMessage::Restoration(
            restoration::ToUserRestoration::GotHeldShares { held_by, shares },
        ) = message
        {
            if let Some(device) = self.devices.iter_mut().find(|d| d.id == held_by) {
                device.shares = shares
                    .into_iter()
                    .map(|held_share| RecoverShare {
                        held_by,
                        held_share,
                    })
                    .collect();
                self.mark_changed();
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum WaitForSingleDeviceState {
    NoDevice,
    TooManyDevices,
    WaitingForDevice {
        device_id: DeviceId,
    },
    BlankDevice {
        device_id: DeviceId,
    },
    DeviceWithShare {
        device_id: DeviceId,
        share: RecoverShare,
    },
}
