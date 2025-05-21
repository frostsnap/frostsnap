pub mod backup_run;
pub mod display_backup;
pub mod enter_physical_backup;
pub mod firmware_upgrade;
pub mod keygen;
mod serial_port;
pub mod signing;
mod ui_protocol;
mod usb_serial_manager;
pub mod verify_address;
pub mod wait_for_recovery_share;
mod wait_for_to_user_message;
pub use wait_for_to_user_message::*;

pub use frostsnap_comms;
pub use frostsnap_core;
pub use serial_port::*;
pub mod settings;
pub use ui_protocol::*;
pub use usb_serial_manager::*;
pub mod bitcoin;
pub use bdk_chain;
pub mod frostsnap_persist;
pub mod persist;

pub trait Sink<M>: Send + 'static {
    fn send(&self, state: M);
    fn close(&self);
    fn inspect<F: Fn(&M)>(self, f: F) -> SinkInspect<Self, F>
    where
        Self: Sized,
    {
        SinkInspect { inner: self, f }
    }
}

impl<M> Sink<M> for () {
    fn send(&self, _: M) {}
    fn close(&self) {}
}

pub struct SinkInspect<S, F> {
    inner: S,
    f: F,
}

impl<M, S: Sink<M>, F: Fn(&M) + Send + 'static> Sink<M> for SinkInspect<S, F> {
    fn send(&self, state: M) {
        (self.f)(&state);
        self.inner.send(state);
    }

    fn close(&self) {
        self.inner.close()
    }
}
