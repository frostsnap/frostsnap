pub mod api;
mod coordinator;
mod device_list;
mod frb_generated;
pub mod logger;
// pub mod old;
pub mod ffi_serial_port;
pub mod sink_wrap;

use frostsnap_coordinator::FirmwareBin;
use frostsnap_core::SymmetricKey;

#[cfg(not(bundle_firmware))]
pub const FIRMWARE: Option<FirmwareBin> = None;

#[cfg(bundle_firmware)]
pub const FIRMWARE: Option<FirmwareBin> = Some(FirmwareBin::new(include_bytes!(concat!(
    env!("OUT_DIR"),
    "/firmware.bin"
))));

#[allow(unused)]
/// meant to be replaced by something that's actually secure from the phone's secure element.
const TEMP_KEY: SymmetricKey = SymmetricKey([42u8; 32]);
