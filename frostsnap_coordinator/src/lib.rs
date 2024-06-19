pub mod backup;
pub mod firmware_upgrade;
pub mod keygen;
mod serial_port;
pub mod signing;
mod ui_protocol;
mod usb_serial_manager;

pub use frostsnap_comms;
pub use frostsnap_core;
pub use serial_port::*;
pub use ui_protocol::*;
pub use usb_serial_manager::*;
pub mod bitcoin;
pub use bdk_chain;
