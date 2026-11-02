// SPDX-License-Identifier: CC0-1.0

//! C type aliases, mirroring `secp256k1_sys::types`.
//!
//! Nothing here actually crosses an FFI boundary in the pure backend, but the
//! names are kept so that call sites do not have to change.

#![allow(non_camel_case_types)]

/// Equivalent of C's `int`.
pub type c_int = i32;
/// Equivalent of C's `unsigned char`.
pub type c_uchar = u8;
/// Equivalent of C's `unsigned int`.
pub type c_uint = u32;
/// Equivalent of C's `size_t`.
pub type size_t = usize;

/// This might not match C's `c_char` exactly.
pub type c_char = i8;

pub use core::ffi::c_void;

/// A type that is as aligned as the biggest alignment for fundamental types in
/// C since C11, i.e. as aligned as `max_align_t`.
#[repr(align(16))]
#[derive(Debug, Default, Copy, Clone)]
#[allow(dead_code)] // We never access the inner data directly, only by way of a pointer.
pub struct AlignedType([u8; 16]);

impl AlignedType {
    /// Returns a zeroed out `AlignedType`.
    pub fn zeroed() -> Self {
        AlignedType([0u8; 16])
    }

    /// A static zeroed out `AlignedType` for use in static assignments of `[AlignedType; _]`
    pub const ZERO: AlignedType = AlignedType([0u8; 16]);
}

/// Checks that wasm32's C ABI matches our assumptions.
///
/// There is no C in the pure backend, so there is nothing to check.
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub fn sanity_checks_for_wasm() {}
