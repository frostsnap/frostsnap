pub mod check_share;
pub mod display_backup;
pub mod firmware_upgrade;
pub mod keygen;
mod serial_port;
pub mod signing;
mod ui_protocol;
mod usb_serial_manager;
pub mod verify_address;

pub use frostsnap_comms;
pub use frostsnap_core;
pub use serial_port::*;
mod settings;
pub use settings::*;
pub use ui_protocol::*;
pub use usb_serial_manager::*;
pub mod bitcoin;
pub use bdk_chain;
pub mod frostsnap_persist;
pub mod persist;

pub trait Sink<M>: Send {
    fn send(&self, state: M);
    fn close(&self);
}

impl<M> Sink<M> for () {
    fn send(&self, _: M) {}
    fn close(&self) {}
}
