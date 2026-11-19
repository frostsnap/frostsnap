// SPDX-License-Identifier: CC0-1.0

//! Pure Rust stand-in for `secp256k1_sys::recovery`.
//!
//! Only the encoding half is implemented. Signing and recovery both need real
//! elliptic curve work and are deliberately absent; see the [parent module]
//! for why.
//!
//! [parent module]: super

use core::{cmp, ptr};

use super::super::arith::scalar_from_bytes;
use super::types::{c_int, c_uchar, c_void};
use super::{Context, NonceFn, PublicKey, Signature};

/// Library-internal representation of a signature plus recovery ID.
///
/// Laid out as `[0..32]` r, `[32..64]` s (both big-endian), `[64]` recovery id.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RecoverableSignature([c_uchar; 65]);

impl RecoverableSignature {
    /// Create a new (zeroed) signature usable for the FFI interface.
    pub fn new() -> RecoverableSignature {
        RecoverableSignature([0; 65])
    }

    /// Serializes the signature in compact format.
    fn serialize(&self) -> [u8; 65] {
        self.0
    }
}

impl Default for RecoverableSignature {
    fn default() -> Self {
        RecoverableSignature::new()
    }
}

impl AsRef<[c_uchar; 65]> for RecoverableSignature {
    #[inline]
    fn as_ref(&self) -> &[c_uchar; 65] {
        &self.0
    }
}

impl<I> core::ops::Index<I> for RecoverableSignature
where
    [c_uchar]: core::ops::Index<I>,
{
    type Output = <[c_uchar] as core::ops::Index<I>>::Output;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        &self.0[index]
    }
}

impl super::CPtr for RecoverableSignature {
    type Target = c_uchar;

    fn as_c_ptr(&self) -> *const Self::Target {
        self.0.as_ptr()
    }
    fn as_mut_c_ptr(&mut self) -> *mut Self::Target {
        self.0.as_mut_ptr()
    }
}

impl core::fmt::Debug for RecoverableSignature {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // The compact signature followed by the recovery id, matching the C backend.
        for i in self.0 {
            write!(f, "{i:02x}")?;
        }
        Ok(())
    }
}

impl PartialOrd for RecoverableSignature {
    fn partial_cmp(&self, other: &RecoverableSignature) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RecoverableSignature {
    fn cmp(&self, other: &RecoverableSignature) -> cmp::Ordering {
        self.serialize().cmp(&other.serialize())
    }
}

impl PartialEq for RecoverableSignature {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for RecoverableSignature {}

impl core::hash::Hash for RecoverableSignature {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.serialize().hash(state);
    }
}

/// Parses a 64-byte compact signature plus a recovery ID.
///
/// # Safety
///
/// `sig` must be writable and `input64` valid for 64 bytes.
pub unsafe fn secp256k1_ecdsa_recoverable_signature_parse_compact(
    _cx: *const Context,
    sig: *mut RecoverableSignature,
    input64: *const c_uchar,
    recid: c_int,
) -> c_int {
    if !(0..=3).contains(&recid) {
        return 0;
    }
    let mut buf = [0u8; 65];
    ptr::copy_nonoverlapping(input64, buf.as_mut_ptr(), 64);
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&buf[..32]);
    s.copy_from_slice(&buf[32..64]);
    // Both halves must be canonical scalars, i.e. less than the curve order.
    if scalar_from_bytes(r).is_none() || scalar_from_bytes(s).is_none() {
        return 0;
    }
    // This cast cannot truncate: recid is in 0..=3.
    buf[64] = recid as u8;
    ptr::write(sig, RecoverableSignature(buf));
    1
}

/// Serializes a recoverable signature into 64 bytes plus a recovery ID.
///
/// # Safety
///
/// `output64` must be writable for 64 bytes, `recid` writable, `sig` valid.
pub unsafe fn secp256k1_ecdsa_recoverable_signature_serialize_compact(
    _cx: *const Context,
    output64: *mut c_uchar,
    recid: *mut c_int,
    sig: *const RecoverableSignature,
) -> c_int {
    ptr::copy_nonoverlapping((*sig).0.as_ptr(), output64, 64);
    *recid = c_int::from((*sig).0[64]);
    1
}

/// Drops the recovery ID, yielding a plain ECDSA signature.
///
/// # Safety
///
/// `sig` must be writable and `recsig` must be valid.
pub unsafe fn secp256k1_ecdsa_recoverable_signature_convert(
    _cx: *const Context,
    sig: *mut Signature,
    recsig: *const RecoverableSignature,
) -> c_int {
    let recsig = *recsig;
    let mut buf = [0u8; 64];
    buf.copy_from_slice(&recsig.0[..64]);
    ptr::write(sig, Signature::from_array_unchecked(buf));
    1
}

/// Recoverable ECDSA signing. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ecdsa_sign_recoverable(
    _cx: *const Context,
    _sig: *mut RecoverableSignature,
    _msg32: *const c_uchar,
    _sk: *const c_uchar,
    _noncefn: NonceFn,
    _noncedata: *const c_void,
) -> c_int {
    unimplemented!(
        "secp256k1: `ecdsa_sign_recoverable` is not available in the pure Rust backend. \
         Enable the `sys` feature to use the C libsecp256k1 implementation."
    )
}

/// Public key recovery. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ecdsa_recover(
    _cx: *const Context,
    _pk: *mut PublicKey,
    _sig: *const RecoverableSignature,
    _msg32: *const c_uchar,
) -> c_int {
    unimplemented!(
        "secp256k1: `ecdsa_recover` is not available in the pure Rust backend. \
         Enable the `sys` feature to use the C libsecp256k1 implementation."
    )
}
