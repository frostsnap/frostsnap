//! The hardware-abstraction seam the lifted `DeviceLoop` runs on.
//!
//! `device/` implements `DeviceHal`/`FirmwareServices` over esp-hal; a host/sim
//! build implements them with in-memory peripherals. The portable run-loop only
//! ever sees these traits — never `esp_hal`.
//!
//! NOTE: this is scaffolding for the `DeviceLoop`-generic phase; some signatures
//! (esp. `FirmwareServices::handle`) may tighten once the loop wires them in.

use crate::flash_header::KeyedHash;
use crate::framed_serial::SerialPort;
use crate::ui::UserInteraction;
use alloc::boxed::Box;
use embedded_graphics::geometry::Point;
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use frostsnap_comms::{CoordinatorSendBody, DeviceSendBody, Downstream, Sha256Digest, Upstream};
use frostsnap_core::device::DeviceSecretDerivation;
use rand_core::{CryptoRng, RngCore};

/// A decoded touch sample in device-display space (calibration already applied).
/// The esp impl converts CST816S events/gestures into these; host/sim synthesizes
/// them. The portable touch handler keys off `lift_up` + `gesture`.
#[derive(Clone, Copy, Debug)]
pub struct TouchEvent {
    pub point: Point,
    /// true on release (CST816S `action == 1`).
    pub lift_up: bool,
    pub gesture: TouchGesture,
}

/// Portable mirror of the CST816S gesture set the UI cares about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchGesture {
    None,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
}

/// A monotonic millisecond clock. `FrostyUi` owns one so `poll()`/`force_redraw()`
/// can stay signatureless and still advance during blocking loops (e.g. erase);
/// `DeviceLoop` takes a separate `&dyn Clock` for its own protocol timing (it
/// can't read the UI's private clock, and `parts()` borrows all of the HAL).
/// esp: wraps a TIMG timer; host: a fake/atomic counter.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// A source of already-decoded touch samples. esp: wraps the CST816S
/// `TouchReceiver` + panel calibration; host/sim: a scriptable queue.
pub trait TouchSource {
    fn next_touch(&mut self) -> Option<TouchEvent>;
}

/// Outcome of one `DeviceLoop::poll`. `ResetRequested` means the loop already
/// sent the upstream reset signal; the device shell performs the actual reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    Continue,
    ResetRequested,
}

/// Outcome of `DeviceLoop` construction: either a ready loop, or a reset request
/// (the init-time recovery erase ran). The shell resets instead of running.
pub enum InitOutcome<L> {
    Ready(L),
    ResetRequested,
}

/// Device-only firmware/attestation services (OTA + genuine challenge). Kept out
/// of the portable core — exactly what the sim drops. `ResetRequested` is the
/// only reason this carries a reset: the esp `enter_upgrade_mode` takeover ends
/// in a reset.
pub enum FirmwareAction {
    None,
    Send(Box<DeviceSendBody>),
    ResetRequested,
}

pub trait FirmwareServices {
    /// Active firmware digest, for `DeviceSendBody::Announce`.
    fn firmware_digest(&self) -> Sha256Digest;

    /// Handle an `Upgrade`/`Challenge` coordinator message. `PrepareUpgrade`
    /// stages an upgrade; `EnterUpgradeMode` takes over the serial links
    /// (`enter_upgrade_mode`) and returns `ResetRequested`; `Challenge` returns a
    /// `SignedChallenge` to forward. The sim impl stages nothing (its seeded
    /// digest means no upgrade is ever offered) and signs no challenge.
    fn handle<U, D>(
        &mut self,
        msg: &CoordinatorSendBody,
        upstream: &mut U,
        downstream: Option<&mut D>,
        ui: &mut dyn UserInteraction,
    ) -> FirmwareAction
    where
        U: SerialPort<Upstream>,
        D: SerialPort<Downstream>;

    /// Drive a staged upgrade (erase progress, ack). Returns a message to forward.
    fn poll(&mut self, ui: &mut dyn UserInteraction) -> FirmwareAction;

    /// The user confirmed the on-screen upgrade prompt (`UiEvent::UpgradeConfirm`).
    fn confirm_upgrade(&mut self);

    /// Drop any staged upgrade (coordinator `Cancel` or a soft reset).
    fn cancel(&mut self);
}

/// The device's peripheral bundle. `DeviceLoop` owns the split-borrow returned by
/// `parts()` plus a `&dyn Clock`, a `&mut UserInteraction`, and the NVS
/// `FlashPartition` (passed to `DeviceLoop::new` from an externally-owned
/// `RefCell<Storage>`, so the HAL never has to expose a self-referential borrow).
/// Two methods is the whole seam: everything time- and detect-related the loop
/// needs is passed in, not pulled from here.
pub trait DeviceHal {
    // `Debug` because `FrostSigner`'s methods require it on the nonce-slot's
    // storage type (the nonce slot derives `Debug`).
    type Storage: NorFlash + ReadNorFlash + core::fmt::Debug;
    type Upstream: SerialPort<frostsnap_comms::Upstream>;
    type Downstream: SerialPort<frostsnap_comms::Downstream>;
    type Rng: RngCore + CryptoRng;
    type Secrets: DeviceSecretDerivation;
    type Firmware: FirmwareServices;

    /// Split-borrow: one `&mut self` yields independent `&mut` sub-borrows so the
    /// loop can hold several at once (e.g. `keygen_ack(parts.secrets, parts.rng)`).
    fn parts(&mut self) -> HalParts<'_, Self>;

    /// The fixed-entropy keyed hash used (once, at init) to derive the device
    /// keypair seed. Separate from `parts().secrets` (the share-encryption key):
    /// they are distinct device secrets.
    fn keypair_hasher(&mut self) -> &mut dyn KeyedHash;
}

pub struct HalParts<'a, H: DeviceHal + ?Sized> {
    pub upstream: &'a mut H::Upstream,
    pub downstream: &'a mut H::Downstream,
    pub rng: &'a mut H::Rng,
    pub secrets: &'a mut H::Secrets,
    pub firmware: &'a mut H::Firmware,
}
