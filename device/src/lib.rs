#![no_std]

#[macro_use]
extern crate alloc;

pub mod device_config;
pub mod esp32_run;
pub mod io;
#[cfg(feature = "frostypede")]
pub mod st7735;
pub mod state;
pub mod storage;
pub mod ui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Established,
}
