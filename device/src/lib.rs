#![no_std]

use alloc::{collections::VecDeque, string::ToString};
use frostsnap_comms::{DeviceSendBody, DeviceSendMessage};
use frostsnap_core::DeviceId;
use ui::UserInteraction;

#[macro_use]
extern crate alloc;

pub mod device_config;
pub mod display_init;
pub mod ds;
pub mod efuse;
pub mod esp32_run;
pub mod factory;
pub mod firmware_size;
pub mod flash;
pub mod frosty_ui;
pub mod io;
pub mod ota;
pub mod panic;
pub mod partitions;
pub mod peripherals;
pub mod resources;
pub mod root_widget;
pub mod secure_boot;
pub mod stack_guard;
pub mod touch_calibration;
pub mod ui;
pub mod widget_tree;

#[derive(Debug, Clone)]
pub struct UpstreamConnection {
    state: UpstreamConnectionState,
    messages: VecDeque<DeviceSendMessage<DeviceSendBody>>,
    announcement: Option<DeviceSendMessage<DeviceSendBody>>,
    my_device_id: DeviceId,
}

impl UpstreamConnection {
    pub fn new(my_device_id: DeviceId) -> Self {
        Self {
            state: UpstreamConnectionState::PowerOn,
            messages: Default::default(),
            announcement: None,
            my_device_id,
        }
    }

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

    pub fn dequeue_message(&mut self) -> Option<DeviceSendMessage<DeviceSendBody>> {
        if self.state >= UpstreamConnectionState::Established {
            if let Some(announcement) = self.announcement.take() {
                return Some(announcement);
            }
        }

        if self.state == UpstreamConnectionState::EstablishedAndCoordAck {
            return self.messages.pop_front();
        }

        None
    }

    pub fn send_announcement(&mut self, announcement: DeviceSendBody) {
        self.announcement = Some(DeviceSendMessage {
            from: self.my_device_id,
            body: announcement,
        });
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

    fn send_debug(&mut self, message: impl ToString) {
        if self.state == UpstreamConnectionState::EstablishedAndCoordAck {
            self.send_to_coordinator([DeviceSendBody::Debug {
                message: message.to_string(),
            }]);
        }
    }

    pub fn has_messages_to_send(&self) -> bool {
        self.announcement.is_some() || !self.messages.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum UpstreamConnectionState {
    /// We have power from the upstream port
    PowerOn,
    /// Received magic bytes from upstream device
    Established,
    /// The coordinator has Ack'd us
    EstablishedAndCoordAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum DownstreamConnectionState {
    Disconnected,
    Connected,
    Established,
}

pub type Instant = fugit::Instant<u64, 1, 1_000_000>;
pub type Duration = fugit::Duration<u64, 1, 1_000_000>;

use micromath::F32Ext;
/// Converts a non‑calibrated point into a calibrated point.
/// The calibration adjusts the y‑coordinate based on the x and y values.
pub fn calibrate_point(
    point: embedded_graphics::prelude::Point,
) -> embedded_graphics::prelude::Point {
    let corrected_y = point.y + x_based_adjustment(point.x) + y_based_adjustment(point.y);
    embedded_graphics::prelude::Point::new(point.x, corrected_y)
}

// DO NOT TOUCH the calibration functions below!
fn x_based_adjustment(x: i32) -> i32 {
    let x = x as f32;
    let corrected = 1.3189e-14 * x.powi(7) - 2.1879e-12 * x.powi(6) - 7.6483e-10 * x.powi(5)
        + 3.2578e-8 * x.powi(4)
        + 6.4233e-5 * x.powi(3)
        - 1.2229e-2 * x.powi(2)
        + 0.8356 * x
        - 20.0;
    (-corrected) as i32
}

fn y_based_adjustment(y: i32) -> i32 {
    if y > 170 {
        return 0;
    }
    let y = y as f32;
    let corrected =
        -5.5439e-07 * y.powi(4) + 1.7576e-04 * y.powi(3) - 1.5104e-02 * y.powi(2) - 2.3443e-02 * y
            + 40.0;
    (-corrected) as i32
}
