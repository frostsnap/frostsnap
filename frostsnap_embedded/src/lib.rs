#![no_std]
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[macro_use]
extern crate alloc;

mod ab_write;
#[cfg(test)]
pub mod test;
pub use ab_write::*;
mod nor_flash_log;
pub use nor_flash_log::*;
mod partition;
pub use partition::*;
mod nonce_slots;
pub use nonce_slots::*;
pub mod flash_header;
pub use flash_header::*;
mod secrets;
pub use secrets::*;
pub mod flash_log;
pub use flash_log::*;

// The lifted device run-loop + UI (esp-free). Gated behind `ui` because it pulls
// the widget stack; storage consumers can still use the crate without it.
#[cfg(feature = "ui")]
mod connection;
#[cfg(feature = "ui")]
pub use connection::*;
/// Display refresh frequency in milliseconds (25ms = 40 FPS).
pub const DISPLAY_REFRESH_MS: u64 = 25;

#[cfg(feature = "ui")]
pub mod device_hal;
#[cfg(feature = "ui")]
pub mod device_loop;
#[cfg(feature = "ui")]
pub mod erase;
#[cfg(feature = "ui")]
pub use device_hal::*;
#[cfg(feature = "ui")]
pub use device_loop::*;
#[cfg(feature = "ui")]
pub mod framed_serial;
#[cfg(feature = "ui")]
pub mod frosty_ui;
#[cfg(feature = "ui")]
pub mod root_widget;
#[cfg(feature = "ui")]
pub mod touch_handler;
#[cfg(feature = "ui")]
pub mod ui;
#[cfg(feature = "ui")]
pub mod widget_tree;
