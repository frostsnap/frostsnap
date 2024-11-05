#![no_std]

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpstreamConnection {
    pub is_device: bool,
    pub state: UpstreamConnectionState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpstreamConnectionState {
    /// We're always at least connected since how did we get power unless we were connected!
    Connected,
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
