#![no_std]

use alloc::{string::ToString, vec::Vec};
use frostsnap_comms::{DeviceSendBody, DeviceSendMessage};
use frostsnap_core::DeviceId;
use ui::UserInteraction;

#[macro_use]
extern crate alloc;

pub mod device_config;
pub mod esp32_run;
#[cfg(feature = "v2")]
pub mod graphics;
pub mod io;
pub mod key_generator;
pub mod ota;
pub mod panic;
pub mod storage;
pub mod ui;

#[derive(Debug, Clone)]
pub struct UpstreamConnection {
    state: UpstreamConnectionState,
    messages: Vec<DeviceSendMessage>,
    my_device_id: DeviceId,
}

impl UpstreamConnection {
    pub fn set_state(&mut self, state: UpstreamConnectionState, ui: &mut impl UserInteraction) {
        ui.set_upstream_connection_state(state);
        match state {
            UpstreamConnectionState::PowerOn => {
                self.messages.clear();
            }
            UpstreamConnectionState::Established => {}
            UpstreamConnectionState::EstablishedAndCoordAck => {}
        }
        self.state = state;
    }

    pub fn get_state(&self) -> UpstreamConnectionState {
        self.state
    }

    pub fn take_messages(&mut self) -> impl Iterator<Item = DeviceSendMessage> + '_ {
        self.messages.drain(..)
    }

    pub fn send_to_coordinator(
        &mut self,
        iter: impl IntoIterator<Item = impl Into<DeviceSendBody>>,
    ) {
        self.messages
            .extend(iter.into_iter().map(|body| DeviceSendMessage {
                from: self.my_device_id,
                body: body.into(),
            }));
    }

    pub fn forward_to_coordinator(&mut self, message: DeviceSendMessage) {
        self.messages.push(message);
    }

    fn send_debug(&mut self, message: impl ToString) {
        if self.state == UpstreamConnectionState::EstablishedAndCoordAck {
            self.send_to_coordinator([DeviceSendBody::Debug {
                message: message.to_string(),
            }]);
        }
    }
}

impl UpstreamConnection {
    pub fn new(my_device_id: DeviceId) -> Self {
        Self {
            state: UpstreamConnectionState::PowerOn,
            messages: Default::default(),
            my_device_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpstreamConnectionState {
    /// We have power from the upstream port
    PowerOn,
    /// Received magic bytes from upstream device
    Established,
    /// The coordinator has Ack'd us
    EstablishedAndCoordAck,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DownstreamConnectionState {
    Disconnected,
    Connected,
    Established,
}

pub type Instant = fugit::Instant<u64, 1, 1_000_000>;
pub type Duration = fugit::Duration<u64, 1, 1_000_000>;
