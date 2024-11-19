// because flutter rust bridge is currently making code that triggers this
#![allow(clippy::unnecessary_literal_unwrap)]
mod api;
mod bridge_generated;
mod camera;
pub use camera::*;
mod coordinator;
pub use coordinator::*;
mod device_list;
use frostsnap_coordinator::FirmwareBin;
mod ffi_serial_port;
mod logger;
mod sink_wrap;

#[cfg(not(bundle_firmware))]
pub const FIRMWARE: Option<FirmwareBin> = None;

#[cfg(bundle_firmware)]
pub const FIRMWARE: Option<FirmwareBin> = Some(FirmwareBin::new(include_bytes!(concat!(
    env!("OUT_DIR"),
    "/firmware.bin"
))));
