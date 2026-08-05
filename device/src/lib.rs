#![no_std]

#[macro_use]
extern crate alloc;

pub use frostsnap_embedded::DISPLAY_REFRESH_MS; // lifted; re-exported

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

pub mod device_config;
pub mod ds;
pub mod efuse;
pub use frostsnap_embedded::erase; // lifted; re-exported so crate::erase still resolves
pub mod esp32_run;
pub mod esp_ui; // esp Clock/TouchSource impls + EspDisplay/EspFrostyUi
pub mod factory;
pub mod firmware; // esp FirmwareServices impl (OTA + genuine challenge)
pub mod firmware_size;
pub mod flash;
pub use frostsnap_embedded::frosty_ui; // lifted; re-exported
pub mod io;
pub mod ota;
pub mod panic;
pub mod partitions;
pub mod peripherals;
pub mod resources;
pub use frostsnap_embedded::root_widget; // lifted; re-exported
pub mod screen_test;
pub mod secure_boot;
pub mod stack_guard;
pub mod touch_calibration;
pub mod uart_interrupt;
pub mod ui;
pub use frostsnap_embedded::widget_tree; // lifted; re-exported

// Lifted into frostsnap_embedded (esp-free). Re-exported here so the rest of
// `device` (esp32_run, frosty_ui, …) keeps referring to `crate::…` unchanged
// until those modules move too.
pub use frostsnap_embedded::{
    DownstreamConnectionState, Duration, Instant, UpstreamConnection, UpstreamConnectionState,
};
