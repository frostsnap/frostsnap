use alloc::vec::Vec;
use esp_hal::{peripherals::DS, sha::Sha};

/// Hardware DS signing implementation using ESP32's Digital Signature peripheral.
///
/// TODO: upstream esp-hal v1.0.0 does not expose a public driver for the DS
/// peripheral (the frostsnap fork had one). For now this is stubbed out — every
/// call to [`HardwareDs::sign`] will panic. The DS peripheral singleton is
/// still held so the plumbing can be wired up once an upstream (or vendored)
/// driver lands.
pub struct HardwareDs<'a> {
    _ds: DS<'a>,
    _encrypted_params: Vec<u8>,
}

impl<'a> HardwareDs<'a> {
    /// Create a new HardwareDs instance
    pub fn new(ds: DS<'a>, encrypted_params: Vec<u8>) -> Self {
        Self {
            _ds: ds,
            _encrypted_params: encrypted_params,
        }
    }

    /// Sign a message using the hardware DS peripheral
    pub fn sign(&mut self, _message: &[u8], _sha256: &mut Sha<'_>) -> [u8; 384] {
        todo!("DS peripheral signing is stubbed out during esp-hal v1 migration")
    }
}
