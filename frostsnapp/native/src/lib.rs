// because flutter rust bridge is currently making code that triggers this
#![allow(clippy::unnecessary_literal_unwrap)]
mod api;
mod bridge_generated;
mod camera;
pub use camera::*;
mod coordinator;
pub use coordinator::*;
mod device_list;
use frostsnap_coordinator::{frostsnap_core::SymmetricKey, FirmwareBin};
mod ffi_serial_port;
mod logger;
mod sink_wrap;

#[cfg(feature = "no_build_firmware")]
pub const FIRMWARE: FirmwareBin = FirmwareBin::new(&[]);

#[cfg(not(feature = "no_build_firmware"))]
pub const FIRMWARE: FirmwareBin =
    FirmwareBin::new(include_bytes!(concat!(env!("OUT_DIR"), "/firmware.bin")));

/// meant to be replaced by something that's actually secure from the phone's secure element.
const TEMP_KEY: SymmetricKey = SymmetricKey([42u8; 32]);
