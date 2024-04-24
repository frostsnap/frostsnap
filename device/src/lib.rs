#![no_std]

#[macro_use]
extern crate alloc;

pub mod device_config;
pub mod esp32_run;
pub mod io;
pub mod panic;
#[cfg(feature = "frostypede")]
pub mod st7735;
#[cfg(feature = "v2")]
pub mod st7789;
pub mod storage;
pub mod ui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Established,
}
