#![no_std]

#[cfg(all(feature = "chip-esp32c3", feature = "chip-esp32s3"))]
compile_error!("Enable exactly one chip feature: chip-esp32c3 or chip-esp32s3");
#[cfg(not(any(feature = "chip-esp32c3", feature = "chip-esp32s3")))]
compile_error!("A chip feature must be enabled: chip-esp32c3 or chip-esp32s3");

#[cfg(feature = "chip-esp32c3")]
use alloc::{collections::VecDeque, string::ToString};
#[cfg(feature = "chip-esp32c3")]
use frostsnap_comms::{DeviceSendBody, DeviceSendMessage, WireDeviceSendBody};
#[cfg(feature = "chip-esp32c3")]
use frostsnap_core::DeviceId;
#[cfg(feature = "chip-esp32c3")]
use ui::UserInteraction;

#[macro_use]
extern crate alloc;

/// Display refresh frequency in milliseconds (25ms = 40 FPS)
pub const DISPLAY_REFRESH_MS: u64 = 25;

/// Log macro for debug logging
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug_log")]
        frostsnap_widgets::debug::log(alloc::format!($($arg)*))
    };
}

/// Log and immediately redraw UI so log is visible
#[macro_export]
macro_rules! log_and_redraw {
    ($ui:expr, $($arg:tt)*) => {{
        log!($($arg)*);
        #[cfg(feature = "debug_log")]
        $ui.force_redraw();
    }};
}

#[cfg(feature = "chip-esp32c3")]
pub mod device_config;
#[cfg(feature = "chip-esp32c3")]
pub mod ds;
#[cfg(feature = "chip-esp32c3")]
pub mod efuse;
#[cfg(feature = "chip-esp32c3")]
pub mod erase;
#[cfg(feature = "chip-esp32c3")]
pub mod esp32_run;
#[cfg(feature = "chip-esp32c3")]
pub mod factory;
#[cfg(feature = "chip-esp32c3")]
pub mod firmware_size;
#[cfg(feature = "chip-esp32c3")]
pub mod flash;
#[cfg(feature = "chip-esp32c3")]
pub mod frosty_ui;
#[cfg(feature = "chip-esp32c3")]
pub mod io;
#[cfg(feature = "chip-esp32c3")]
pub mod ota;
pub mod panic;
#[cfg(feature = "chip-esp32c3")]
pub mod partitions;
#[cfg(feature = "chip-esp32c3")]
pub mod peripherals;
#[cfg(feature = "chip-esp32s3")]
#[path = "peripherals_s3.rs"]
pub mod peripherals;
#[cfg(feature = "chip-esp32c3")]
pub mod resources;
#[cfg(feature = "chip-esp32c3")]
pub mod root_widget;
#[cfg(feature = "chip-esp32c3")]
pub mod screen_test;
#[cfg(feature = "chip-esp32c3")]
pub mod secure_boot;
#[cfg(all(feature = "stack_guard", feature = "chip-esp32c3"))]
pub mod stack_guard;
pub mod touch_calibration;
pub mod touch_handler;
#[cfg(feature = "chip-esp32c3")]
pub mod uart_interrupt;
#[cfg(feature = "chip-esp32c3")]
pub mod ui;
#[cfg(feature = "chip-esp32c3")]
pub mod widget_tree;

#[cfg(feature = "chip-esp32c3")]
#[derive(Debug, Clone)]
pub struct UpstreamConnection {
    state: UpstreamConnectionState,
    messages: VecDeque<DeviceSendMessage<WireDeviceSendBody>>,
    announcement: Option<DeviceSendMessage<DeviceSendBody>>,
    my_device_id: DeviceId,
}

#[cfg(feature = "chip-esp32c3")]
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
                // HACK: We want to clear messages when resetting the connection
                // upstream but keep the downstream announcements otherwise we
                // would have to trigger something downstream for them to resend
                // it.
                self.messages.retain(|msg| msg.from != self.my_device_id);
            }
            UpstreamConnectionState::Established => {}
            UpstreamConnectionState::EstablishedAndCoordAck => {}
        }
        self.state = state;
    }

    pub fn get_state(&self) -> UpstreamConnectionState {
        self.state
    }

    pub fn dequeue_message(&mut self) -> Option<DeviceSendMessage<WireDeviceSendBody>> {
        if self.state >= UpstreamConnectionState::Established {
            if let Some(announcement) = self.announcement.take() {
                return Some(announcement.into());
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
        self.messages.extend(iter.into_iter().map(|body| {
            DeviceSendMessage {
                from: self.my_device_id,
                body: body.into(),
            }
            .into()
        }));
    }

    pub fn forward_to_coordinator(&mut self, message: DeviceSendMessage<WireDeviceSendBody>) {
        self.messages.push_back(message);
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

#[cfg(feature = "chip-esp32c3")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum UpstreamConnectionState {
    /// We have power from the upstream port
    PowerOn,
    /// Received magic bytes from upstream device
    Established,
    /// The coordinator has Ack'd us
    EstablishedAndCoordAck,
}

#[cfg(feature = "chip-esp32c3")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum DownstreamConnectionState {
    Disconnected,
    Connected,
    Established,
}

pub use esp_hal::time::{Duration, Instant};
