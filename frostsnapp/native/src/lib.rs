// because flutter rust bridge is currently making code that triggers this
#![allow(clippy::unnecessary_literal_unwrap)]
mod api;
mod bridge_generated;
mod camera;
pub use camera::*;
mod coordinator;
mod persist_core;
pub use coordinator::*;
mod chain_sync;
mod device_list;
use frostsnap_coordinator::FirmwareBin;
mod ffi_serial_port;
pub mod wallet;

#[cfg(feature = "no_build_firmware")]
pub const FIRMWARE: FirmwareBin = FirmwareBin::new(&[]);

#[cfg(not(feature = "no_build_firmware"))]
pub const FIRMWARE: FirmwareBin =
    FirmwareBin::new(include_bytes!(concat!(env!("OUT_DIR"), "/firmware.bin")));
