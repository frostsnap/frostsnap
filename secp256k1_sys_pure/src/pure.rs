// SPDX-License-Identifier: CC0-1.0

//! A pure Rust stand-in for `secp256k1-sys`.
//!
//! This module mirrors the parts of the `secp256k1_sys` API that this crate
//! consumes, so that `pub use` swapping it in for `secp256k1_sys` leaves the
//! rest of the crate compiling unchanged. Everything here is safe Rust
//! underneath, but the signatures keep the C shapes (out-pointers, `c_int`
//! returns, `1` for success) so the call sites do not have to change.
//!
//! # Representation
//!
//! The C library's opaque types hold an internal, platform-dependent encoding.
//! Since there is no C here, we are free to choose, and we simply store the
//! standard serializations left-aligned in the same sized buffers:
//!
//! | type | layout |
//! |---|---|
//! | [`PublicKey`] | `[0..33]` compressed point, rest zero |
//! | [`XOnlyPublicKey`] | `[0..32]` x-coordinate, rest zero |
//! | [`Keypair`] | `[0..32]` secret scalar, `[32..65]` compressed point |
//! | [`Signature`] | `[0..32]` r, `[32..64]` s, both big-endian |
//! | `RecoverableSignature` | `[0..64]` as [`Signature`], `[64]` recovery id |
//!
//! This costs a re-parse per operation relative to the C library, which caches
//! the decompressed point. That is the right trade here: the operations are
//! rare, and it keeps the code small enough to audit.
//!
//! # Not implemented
//!
//! Anything that signs or verifies panics rather than guessing:
//!
//! - ECDSA signing, verification, and recovery
//! - Schnorr (BIP-340) signing and verification
//! - ECDH
//! - ElligatorSwift
//! - lax DER parsing
//!
//! These panic deliberately. A verifier that disagrees with the consensus
//! implementation in any edge case is far worse than one that is absent, so
//! they are left out until they can be implemented and differentially tested
//! against the C library.

#![allow(non_upper_case_globals)]

use core::{cmp, ptr};

use super::arith::{
    self, point_add_mul_generator, point_from_acc, point_from_bytes, point_from_bytes_uncompressed,
    point_is_y_even, point_lift_x, point_mul_scalar, point_negate, point_to_bytes,
    point_to_bytes_uncompressed, point_x_bytes, scalar_conditional_negate, scalar_from_bytes,
    scalar_from_bytes_nonzero, scalar_is_high, scalar_nonzero, scalar_to_bytes, Point, PointAcc,
    Scalar,
};

pub mod types;

use self::types::{c_int, c_uchar, c_uint, c_void, size_t};

/// Panics with a message naming the operation that is missing.
macro_rules! unimplemented_op {
    ($name:literal) => {
        unimplemented!(concat!(
            "secp256k1: `",
            $name,
            "` is not available in the pure Rust backend. ",
            "Enable the `sys` feature to use the C libsecp256k1 implementation."
        ))
    };
}

/* ------------------------------------------------------------------------- */
/* Constants and function pointer types                                       */
/* ------------------------------------------------------------------------- */

/// Flag for context to enable no precomputation
pub const SECP256K1_START_NONE: c_uint = 1;
/// Flag for context to enable verification precomputation
pub const SECP256K1_START_VERIFY: c_uint = 1 | (1 << 8);
/// Flag for context to enable signing precomputation
pub const SECP256K1_START_SIGN: c_uint = 1 | (1 << 9);
/// Flag for keys to indicate uncompressed serialization format
#[allow(unused_parens)]
pub const SECP256K1_SER_UNCOMPRESSED: c_uint = (1 << 1);
/// Flag for keys to indicate compressed serialization format
pub const SECP256K1_SER_COMPRESSED: c_uint = (1 << 1) | (1 << 8);

/// A nonce generation function.
pub type NonceFn = Option<
    unsafe extern "C" fn(
        nonce32: *mut c_uchar,
        msg32: *const c_uchar,
        key32: *const c_uchar,
        algo16: *const c_uchar,
        data: *mut c_void,
        attempt: c_uint,
    ) -> c_int,
>;

/// Hash function to use to post-process an ECDH point to get a shared secret.
pub type EcdhHashFn = Option<
    unsafe extern "C" fn(
        output: *mut c_uchar,
        x: *const c_uchar,
        y: *const c_uchar,
        data: *mut c_void,
    ) -> c_int,
>;

/// Nonce function for Schnorr signatures.
pub type SchnorrNonceFn = Option<
    unsafe extern "C" fn(
        nonce32: *mut c_uchar,
        msg32: *const c_uchar,
        msg_len: size_t,
        key32: *const c_uchar,
        xonly_pk32: *const c_uchar,
        algo16: *const c_uchar,
        algo_len: size_t,
        data: *mut c_void,
    ) -> c_int,
>;

/// A hash function used by `ellswift_ecdh` to hash the final ECDH shared secret.
pub type EllswiftEcdhHashFn = Option<
    unsafe extern "C" fn(
        output: *mut c_uchar,
        x32: *const c_uchar,
        ell_a64: *const c_uchar,
        ell_b64: *const c_uchar,
        data: *mut c_void,
    ) -> c_int,
>;

/// Data structure that contains additional arguments for `schnorrsig_sign_custom`.
#[repr(C)]
#[derive(Debug)]
pub struct SchnorrSigExtraParams {
    magic: [c_uchar; 4],
    nonce_fp: SchnorrNonceFn,
    ndata: *const c_void,
}

impl SchnorrSigExtraParams {
    /// Create a new `SchnorrSigExtraParams` properly initialized.
    pub fn new(nonce_fp: SchnorrNonceFn, ndata: *const c_void) -> Self {
        SchnorrSigExtraParams {
            magic: [0xda, 0x6f, 0xb3, 0x8c],
            nonce_fp,
            ndata,
        }
    }
}

/// Default ECDH hash function. Unused by the pure backend.
pub static secp256k1_ecdh_hash_function_default: EcdhHashFn = None;
/// Default ECDH hash function for BIP324. Unused by the pure backend.
pub static secp256k1_ellswift_xdh_hash_function_bip324: EllswiftEcdhHashFn = None;
/// RFC6979 nonce function. Unused by the pure backend.
pub static secp256k1_nonce_function_rfc6979: NonceFn = None;
/// Default nonce function. Unused by the pure backend.
pub static secp256k1_nonce_function_default: NonceFn = None;
/// BIP-340 nonce function. Unused by the pure backend.
pub static secp256k1_nonce_function_bip340: SchnorrNonceFn = None;

/* ------------------------------------------------------------------------- */
/* Opaque types                                                               */
/* ------------------------------------------------------------------------- */

/// Implement the inherent methods and traits `secp256k1-sys` puts on its
/// fixed-size byte-array newtypes.
macro_rules! impl_array_newtype {
    ($thing:ident, $ty:ty, $len:expr) => {
        impl $thing {
            /// Like `cmp::Ord` but faster and with no guarantees across library versions.
            pub fn cmp_fast_unstable(&self, other: &Self) -> core::cmp::Ordering {
                self[..].cmp(&other[..])
            }

            /// Like `cmp::Eq` but faster and with no guarantees across library versions.
            pub fn eq_fast_unstable(&self, other: &Self) -> bool {
                self[..].eq(&other[..])
            }
        }

        impl AsRef<[$ty; $len]> for $thing {
            #[inline]
            fn as_ref(&self) -> &[$ty; $len] {
                let &$thing(ref dat) = self;
                dat
            }
        }

        impl<I> core::ops::Index<I> for $thing
        where
            [$ty]: core::ops::Index<I>,
        {
            type Output = <[$ty] as core::ops::Index<I>>::Output;

            #[inline]
            fn index(&self, index: I) -> &Self::Output {
                &self.0[index]
            }
        }

        impl crate::pure::CPtr for $thing {
            type Target = $ty;

            fn as_c_ptr(&self) -> *const Self::Target {
                let &$thing(ref dat) = self;
                dat.as_ptr()
            }

            fn as_mut_c_ptr(&mut self) -> *mut Self::Target {
                let &mut $thing(ref mut dat) = self;
                dat.as_mut_ptr()
            }
        }

        impl core::fmt::Debug for $thing {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                for i in self[..].iter().cloned() {
                    write!(f, "{:02x}", i)?;
                }
                Ok(())
            }
        }
    };
}

/// A Secp256k1 context.
///
/// The pure backend has no precomputation tables and no state, so this only
/// exists to keep the shape of the API. It carries the flags it was created
/// with so that `preallocated_clone` has something to copy.
// Deliberately not `Copy`, to match the C backend's type exactly.
#[allow(missing_copy_implementations)]
#[derive(Clone, Debug)]
#[repr(C)]
pub struct Context(c_int);

/// Library-internal representation of a Secp256k1 public key.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PublicKey([c_uchar; 64]);
impl_array_newtype!(PublicKey, c_uchar, 64);

impl PublicKey {
    /// Creates an "uninitialized" public key which is zeroed out.
    ///
    /// # Safety
    ///
    /// The result is not a valid key; only use it as an out-parameter.
    pub unsafe fn new() -> Self {
        Self::from_array_unchecked([0; 64])
    }

    /// Create a new public key from the raw internal representation.
    ///
    /// # Safety
    ///
    /// Does not check the validity of the underlying representation.
    pub unsafe fn from_array_unchecked(data: [c_uchar; 64]) -> Self {
        PublicKey(data)
    }

    /// Returns the underlying opaque representation of the public key.
    pub fn underlying_bytes(self) -> [c_uchar; 64] {
        self.0
    }

    /// Serializes this public key in compressed form.
    fn serialize(&self) -> [u8; 33] {
        let mut buf = [0u8; 33];
        buf.copy_from_slice(&self.0[..33]);
        buf
    }
}

impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &PublicKey) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &PublicKey) -> cmp::Ordering {
        self.serialize().cmp(&other.serialize())
    }
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for PublicKey {}

impl core::hash::Hash for PublicKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.serialize().hash(state);
    }
}

/// Library-internal representation of a Secp256k1 signature.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Signature([c_uchar; 64]);
impl_array_newtype!(Signature, c_uchar, 64);

impl Signature {
    /// Creates an "uninitialized" signature which is zeroed out.
    ///
    /// # Safety
    ///
    /// The result is not a valid signature; only use it as an out-parameter.
    pub unsafe fn new() -> Self {
        Self::from_array_unchecked([0; 64])
    }

    /// Create a new signature from the raw internal representation.
    ///
    /// # Safety
    ///
    /// Does not check the validity of the underlying representation.
    pub unsafe fn from_array_unchecked(data: [c_uchar; 64]) -> Self {
        Signature(data)
    }

    /// Returns the underlying opaque representation of the signature.
    pub fn underlying_bytes(self) -> [c_uchar; 64] {
        self.0
    }

    /// Serializes the signature in compact format.
    fn serialize(&self) -> [u8; 64] {
        self.0
    }
}

impl PartialOrd for Signature {
    fn partial_cmp(&self, other: &Signature) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Signature {
    fn cmp(&self, other: &Signature) -> cmp::Ordering {
        self.serialize().cmp(&other.serialize())
    }
}

impl PartialEq for Signature {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for Signature {}

impl core::hash::Hash for Signature {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.serialize().hash(state);
    }
}

/// Library-internal representation of an x-only public key.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct XOnlyPublicKey([c_uchar; 64]);
impl_array_newtype!(XOnlyPublicKey, c_uchar, 64);

impl XOnlyPublicKey {
    /// Creates an "uninitialized" x-only public key which is zeroed out.
    ///
    /// # Safety
    ///
    /// The result is not a valid key; only use it as an out-parameter.
    pub unsafe fn new() -> Self {
        Self::from_array_unchecked([0; 64])
    }

    /// Create a new x-only public key from the raw internal representation.
    ///
    /// # Safety
    ///
    /// Does not check the validity of the underlying representation.
    pub unsafe fn from_array_unchecked(data: [c_uchar; 64]) -> Self {
        XOnlyPublicKey(data)
    }

    /// Returns the underlying opaque representation of the key.
    pub fn underlying_bytes(self) -> [c_uchar; 64] {
        self.0
    }

    /// Serializes this key to its 32-byte x-coordinate.
    fn serialize(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&self.0[..32]);
        buf
    }
}

impl PartialOrd for XOnlyPublicKey {
    fn partial_cmp(&self, other: &XOnlyPublicKey) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for XOnlyPublicKey {
    fn cmp(&self, other: &XOnlyPublicKey) -> cmp::Ordering {
        self.serialize().cmp(&other.serialize())
    }
}

impl PartialEq for XOnlyPublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for XOnlyPublicKey {}

impl core::hash::Hash for XOnlyPublicKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.serialize().hash(state);
    }
}

/// Library-internal representation of a keypair.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Keypair([c_uchar; 96]);
impl_array_newtype!(Keypair, c_uchar, 96);

impl Keypair {
    /// Creates an "uninitialized" keypair which is zeroed out.
    ///
    /// # Safety
    ///
    /// The result is not a valid keypair; only use it as an out-parameter.
    pub unsafe fn new() -> Self {
        Self::from_array_unchecked([0; 96])
    }

    /// Create a new keypair from the raw internal representation.
    ///
    /// # Safety
    ///
    /// Does not check the validity of the underlying representation.
    pub unsafe fn from_array_unchecked(data: [c_uchar; 96]) -> Self {
        Keypair(data)
    }

    /// Returns the underlying opaque representation of the keypair.
    pub fn underlying_bytes(self) -> [c_uchar; 96] {
        self.0
    }

    /// Serializes the public half in compressed form.
    fn serialize_pk(&self) -> [u8; 33] {
        let mut buf = [0u8; 33];
        buf.copy_from_slice(&self.0[32..65]);
        buf
    }

    /// Attempts to erase the secret contents of the keypair.
    ///
    /// As in the C backend, the keypair is overwritten with a valid dummy
    /// rather than zeroed, so that it stays loadable.
    #[inline]
    pub fn non_secure_erase(&mut self) {
        non_secure_erase_impl(&mut self.0, DUMMY_KEYPAIR);
    }
}

/// The internal representation of the keypair with secret key `[1u8; 32]`, so
/// the public half is the generator.
///
/// Unlike the C backend this needs no per-endianness variants, because the
/// representation here is just the standard serializations.
const DUMMY_KEYPAIR: [c_uchar; 96] = {
    let mut kp = [0u8; 96];
    let mut i = 0;
    while i < 32 {
        kp[i] = 1;
        i += 1;
    }
    // Compressed encoding of G.
    let g = [
        0x02, 0x79, 0xBE, 0x66, 0x7E, 0xF9, 0xDC, 0xBB, 0xAC, 0x55, 0xA0, 0x62, 0x95, 0xCE, 0x87,
        0x0B, 0x07, 0x02, 0x9B, 0xFC, 0xDB, 0x2D, 0xCE, 0x28, 0xD9, 0x59, 0xF2, 0x81, 0x5B, 0x16,
        0xF8, 0x17, 0x98,
    ];
    i = 0;
    while i < 33 {
        kp[32 + i] = g[i];
        i += 1;
    }
    kp
};

/// Does a best attempt at secure erasure using Rust intrinsics.
///
/// The implementation is based on the approach used by the [`zeroize`] crate.
///
/// [`zeroize`]: https://docs.rs/zeroize
#[inline(always)]
pub fn non_secure_erase_impl<T>(dst: &mut T, src: T) {
    use core::sync::atomic;
    // Overwrite using a volatile write so it cannot be optimized away.
    unsafe {
        ptr::write_volatile(dst, src);
    }
    // Prevent future accesses from being reordered to before the erasure.
    atomic::compiler_fence(atomic::Ordering::SeqCst);
}

impl PartialOrd for Keypair {
    fn partial_cmp(&self, other: &Keypair) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Keypair {
    fn cmp(&self, other: &Keypair) -> cmp::Ordering {
        self.serialize_pk().cmp(&other.serialize_pk())
    }
}

impl PartialEq for Keypair {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for Keypair {}

impl core::hash::Hash for Keypair {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.serialize_pk().hash(state);
    }
}

/// Library-internal representation of an ElligatorSwift encoded point.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElligatorSwift([u8; 64]);

impl ElligatorSwift {
    /// Creates an `ElligatorSwift` from its raw 64-byte encoding.
    pub fn from_array(arr: [u8; 64]) -> Self {
        ElligatorSwift(arr)
    }
    /// Returns the raw 64-byte encoding.
    pub fn to_array(self) -> [u8; 64] {
        self.0
    }
}

impl_array_newtype!(ElligatorSwift, u8, 64);

/* ------------------------------------------------------------------------- */
/* CPtr                                                                       */
/* ------------------------------------------------------------------------- */

/// A trait for types that can be converted to a raw pointer for the C API.
pub trait CPtr {
    /// The pointee type.
    type Target;
    /// Returns a const pointer to the data.
    fn as_c_ptr(&self) -> *const Self::Target;
    /// Returns a mutable pointer to the data.
    fn as_mut_c_ptr(&mut self) -> *mut Self::Target;
}

impl<T> CPtr for [T] {
    type Target = T;
    fn as_c_ptr(&self) -> *const Self::Target {
        if self.is_empty() {
            ptr::null()
        } else {
            self.as_ptr()
        }
    }

    fn as_mut_c_ptr(&mut self) -> *mut Self::Target {
        if self.is_empty() {
            ptr::null_mut::<Self::Target>()
        } else {
            self.as_mut_ptr()
        }
    }
}

impl<T> CPtr for &[T] {
    type Target = T;
    fn as_c_ptr(&self) -> *const Self::Target {
        if self.is_empty() {
            ptr::null()
        } else {
            self.as_ptr()
        }
    }

    fn as_mut_c_ptr(&mut self) -> *mut Self::Target {
        if self.is_empty() {
            ptr::null_mut()
        } else {
            self.as_ptr() as *mut Self::Target
        }
    }
}

impl CPtr for [u8; 32] {
    type Target = u8;
    fn as_c_ptr(&self) -> *const Self::Target {
        self.as_ptr()
    }
    fn as_mut_c_ptr(&mut self) -> *mut Self::Target {
        self.as_mut_ptr()
    }
}

impl<T: CPtr> CPtr for Option<T> {
    type Target = T::Target;
    fn as_mut_c_ptr(&mut self) -> *mut Self::Target {
        match self {
            Some(contents) => contents.as_mut_c_ptr(),
            None => ptr::null_mut(),
        }
    }
    fn as_c_ptr(&self) -> *const Self::Target {
        match self {
            Some(content) => content.as_c_ptr(),
            None => ptr::null(),
        }
    }
}

/* ------------------------------------------------------------------------- */
/* Internal helpers                                                           */
/* ------------------------------------------------------------------------- */

static NO_PRECOMP: Context = Context(SECP256K1_START_NONE as c_int);

/// A context that does not carry precomputation tables.
pub static secp256k1_context_no_precomp: &Context = &NO_PRECOMP;

/// Reads a fixed-size array out of a raw pointer.
///
/// # Safety
///
/// `p` must be valid for reads of `N` bytes.
unsafe fn read_arr<const N: usize>(p: *const c_uchar) -> [u8; N] {
    let mut buf = [0u8; N];
    ptr::copy_nonoverlapping(p, buf.as_mut_ptr(), N);
    buf
}

/// Decodes the compressed point held in a [`PublicKey`].
fn load_pubkey(pk: &PublicKey) -> Option<Point> {
    let mut buf = [0u8; 33];
    buf.copy_from_slice(&pk.0[..33]);
    point_from_bytes(buf)
}

/// Encodes a point into the [`PublicKey`] representation.
fn save_pubkey(point: &Point) -> PublicKey {
    let mut buf = [0u8; 64];
    buf[..33].copy_from_slice(&point_to_bytes(point));
    PublicKey(buf)
}

/// Lifts the x-coordinate held in an [`XOnlyPublicKey`] to a point with even y.
fn load_xonly(pk: &XOnlyPublicKey) -> Option<Point> {
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&pk.0[..32]);
    point_lift_x(buf)
}

/// Encodes the x-coordinate of a point into the [`XOnlyPublicKey`] representation.
fn save_xonly(x: [u8; 32]) -> XOnlyPublicKey {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&x);
    XOnlyPublicKey(buf)
}

/// Decodes the secret scalar and public point held in a [`Keypair`].
fn load_keypair(kp: &Keypair) -> Option<(Scalar, Point)> {
    let mut sk_buf = [0u8; 32];
    sk_buf.copy_from_slice(&kp.0[..32]);
    let sk = scalar_from_bytes_nonzero(sk_buf)?;
    let mut pk_buf = [0u8; 33];
    pk_buf.copy_from_slice(&kp.0[32..65]);
    let pk = point_from_bytes(pk_buf)?;
    Some((sk, pk))
}

/// Encodes a secret scalar and its public point into the [`Keypair`] representation.
fn save_keypair(sk: &Scalar, pk: &Point) -> Keypair {
    let mut buf = [0u8; 96];
    buf[..32].copy_from_slice(&scalar_to_bytes(sk));
    buf[32..65].copy_from_slice(&point_to_bytes(pk));
    Keypair(buf)
}

/// Computes `scalar * G`.
fn mul_generator(sk: &Scalar) -> Point {
    arith::mul_generator(sk).expect("scalar is non-zero")
}

/// Reads a tweak, which may be zero but must be less than the curve order.
fn load_tweak(tweak32: [u8; 32]) -> Option<Scalar> {
    scalar_from_bytes(tweak32)
}

/* ------------------------------------------------------------------------- */
/* Contexts                                                                   */
/* ------------------------------------------------------------------------- */

/// Returns the size of a preallocated context buffer.
///
/// # Safety
///
/// Always safe; `unsafe` only to match the C signature.
pub unsafe fn secp256k1_context_preallocated_size(_flags: c_uint) -> size_t {
    core::mem::size_of::<Context>()
}

/// Creates a context in caller-provided memory.
///
/// # Safety
///
/// `prealloc` must point to at least `secp256k1_context_preallocated_size(flags)`
/// writable, suitably aligned bytes.
pub unsafe fn secp256k1_context_preallocated_create(
    prealloc: ptr::NonNull<c_void>,
    flags: c_uint,
) -> ptr::NonNull<Context> {
    let ctx = prealloc.cast::<Context>();
    ptr::write(ctx.as_ptr(), Context(flags as c_int));
    ctx
}

/// Returns the buffer size needed to clone a context.
///
/// # Safety
///
/// `cx` must be a valid context pointer.
pub unsafe fn secp256k1_context_preallocated_clone_size(_cx: *const Context) -> size_t {
    core::mem::size_of::<Context>()
}

/// Clones a context into caller-provided memory.
///
/// # Safety
///
/// `cx` must be valid, and `prealloc` must satisfy the same requirements as in
/// [`secp256k1_context_preallocated_create`].
pub unsafe fn secp256k1_context_preallocated_clone(
    cx: *const Context,
    prealloc: ptr::NonNull<c_void>,
) -> ptr::NonNull<Context> {
    let flags = if cx.is_null() {
        SECP256K1_START_NONE as c_int
    } else {
        (*cx).0
    };
    let ctx = prealloc.cast::<Context>();
    ptr::write(ctx.as_ptr(), Context(flags));
    ctx
}

/// Destroys a context. A no-op: the pure backend holds no resources.
///
/// # Safety
///
/// `cx` must be a valid context pointer.
pub unsafe fn secp256k1_context_preallocated_destroy(_cx: ptr::NonNull<Context>) {}

/// Re-randomizes a context. A no-op: there is no secret state to blind.
///
/// # Safety
///
/// `cx` must be a valid context pointer.
pub unsafe fn secp256k1_context_randomize(
    _cx: ptr::NonNull<Context>,
    _seed32: *const c_uchar,
) -> c_int {
    1
}

/* ------------------------------------------------------------------------- */
/* Public keys                                                                */
/* ------------------------------------------------------------------------- */

/// Parses a 33-byte compressed or 65-byte uncompressed public key.
///
/// # Safety
///
/// `pk` must be writable and `input` valid for `in_len` bytes.
pub unsafe fn secp256k1_ec_pubkey_parse(
    _cx: *const Context,
    pk: *mut PublicKey,
    input: *const c_uchar,
    in_len: size_t,
) -> c_int {
    let point = match in_len {
        33 => point_from_bytes(read_arr::<33>(input)),
        65 => point_from_bytes_uncompressed(read_arr::<65>(input)),
        _ => None,
    };
    match point {
        Some(point) => {
            ptr::write(pk, save_pubkey(&point));
            1
        }
        None => 0,
    }
}

/// Serializes a public key, compressed or uncompressed depending on `flags`.
///
/// # Safety
///
/// `output` must be writable for `*out_len` bytes and `pk` must be valid.
pub unsafe fn secp256k1_ec_pubkey_serialize(
    _cx: *const Context,
    output: *mut c_uchar,
    out_len: *mut size_t,
    pk: *const PublicKey,
    flags: c_uint,
) -> c_int {
    let point = match load_pubkey(&*pk) {
        Some(point) => point,
        None => return 0,
    };
    // The C library treats an undersized buffer as a caller bug and charges past
    // it. Refusing is safer and costs correct callers nothing, since they always
    // pass exactly 33 or 65 bytes.
    if flags == SECP256K1_SER_COMPRESSED {
        if *out_len < 33 {
            return 0;
        }
        let ser = point_to_bytes(&point);
        ptr::copy_nonoverlapping(ser.as_ptr(), output, 33);
        *out_len = 33;
    } else {
        if *out_len < 65 {
            return 0;
        }
        let ser = point_to_bytes_uncompressed(&point);
        ptr::copy_nonoverlapping(ser.as_ptr(), output, 65);
        *out_len = 65;
    }
    1
}

/// Computes the public key corresponding to a secret key.
///
/// # Safety
///
/// `pk` must be writable and `sk` valid for 32 bytes.
pub unsafe fn secp256k1_ec_pubkey_create(
    _cx: *const Context,
    pk: *mut PublicKey,
    sk: *const c_uchar,
) -> c_int {
    let sk = match scalar_from_bytes_nonzero(read_arr::<32>(sk)) {
        Some(sk) => sk,
        None => return 0,
    };
    ptr::write(pk, save_pubkey(&mul_generator(&sk)));
    1
}

/// Negates a public key in place.
///
/// # Safety
///
/// `pk` must point to a valid, writable public key.
pub unsafe fn secp256k1_ec_pubkey_negate(_cx: *const Context, pk: *mut PublicKey) -> c_int {
    let point = match load_pubkey(&*pk) {
        Some(point) => point,
        None => return 0,
    };
    ptr::write(pk, save_pubkey(&point_negate(&point)));
    1
}

/// Compares two public keys by their compressed serialization.
///
/// # Safety
///
/// Both pointers must be valid.
pub unsafe fn secp256k1_ec_pubkey_cmp(
    _cx: *const Context,
    pk1: *const PublicKey,
    pk2: *const PublicKey,
) -> c_int {
    match (*pk1).serialize().cmp(&(*pk2).serialize()) {
        cmp::Ordering::Less => -1,
        cmp::Ordering::Equal => 0,
        cmp::Ordering::Greater => 1,
    }
}

/// Tweaks a public key by adding `tweak * G` to it.
///
/// # Safety
///
/// `pk` must point to a valid, writable public key and `tweak` to 32 bytes.
pub unsafe fn secp256k1_ec_pubkey_tweak_add(
    _cx: *const Context,
    pk: *mut PublicKey,
    tweak: *const c_uchar,
) -> c_int {
    let point = match load_pubkey(&*pk) {
        Some(point) => point,
        None => return 0,
    };
    let tweak = match load_tweak(read_arr::<32>(tweak)) {
        Some(tweak) => tweak,
        None => return 0,
    };
    match point_add_mul_generator(&point, &tweak) {
        Some(tweaked) => {
            ptr::write(pk, save_pubkey(&tweaked));
            1
        }
        None => 0,
    }
}

/// Tweaks a public key by multiplying it by `tweak`.
///
/// # Safety
///
/// `pk` must point to a valid, writable public key and `tweak` to 32 bytes.
pub unsafe fn secp256k1_ec_pubkey_tweak_mul(
    _cx: *const Context,
    pk: *mut PublicKey,
    tweak: *const c_uchar,
) -> c_int {
    let point = match load_pubkey(&*pk) {
        Some(point) => point,
        None => return 0,
    };
    // Unlike `tweak_add`, a zero tweak is rejected: the result would be the
    // point at infinity, which has no valid encoding.
    let tweak = match scalar_from_bytes_nonzero(read_arr::<32>(tweak)) {
        Some(tweak) => tweak,
        None => return 0,
    };
    // A non-zero scalar times a non-identity point is never the identity.
    let tweaked = point_mul_scalar(&point, &tweak).expect("neither operand is zero");
    ptr::write(pk, save_pubkey(&tweaked));
    1
}

/// Adds a number of public keys together.
///
/// # Safety
///
/// `ins` must point to `n` valid public key pointers, and `out` must be writable.
pub unsafe fn secp256k1_ec_pubkey_combine(
    _cx: *const Context,
    out: *mut PublicKey,
    ins: *const *const PublicKey,
    n: size_t,
) -> c_int {
    if n == 0 {
        return 0;
    }
    let mut acc = PointAcc::IDENTITY;
    for i in 0..n {
        let pk = *ins.add(i);
        let point = match load_pubkey(&*pk) {
            Some(point) => point,
            None => return 0,
        };
        acc += PointAcc::from(&point);
    }
    match point_from_acc(acc) {
        Some(sum) => {
            ptr::write(out, save_pubkey(&sum));
            1
        }
        None => 0,
    }
}

/* ------------------------------------------------------------------------- */
/* Secret keys                                                                */
/* ------------------------------------------------------------------------- */

/// Checks that a secret key is in `[1, n)`.
///
/// # Safety
///
/// `sk` must be valid for 32 bytes.
pub unsafe fn secp256k1_ec_seckey_verify(_cx: *const Context, sk: *const c_uchar) -> c_int {
    c_int::from(scalar_from_bytes_nonzero(read_arr::<32>(sk)).is_some())
}

/// Negates a secret key in place.
///
/// Matches the C library: on an invalid input the key is zeroed and 0 returned.
///
/// # Safety
///
/// `sk` must be valid for reads and writes of 32 bytes.
pub unsafe fn secp256k1_ec_seckey_negate(_cx: *const Context, sk: *mut c_uchar) -> c_int {
    match scalar_from_bytes_nonzero(read_arr::<32>(sk)) {
        Some(scalar) => {
            ptr::copy_nonoverlapping(scalar_to_bytes(&-scalar).as_ptr(), sk, 32);
            1
        }
        None => {
            ptr::write_bytes(sk, 0, 32);
            0
        }
    }
}

/// Adds `tweak` to a secret key in place.
///
/// # Safety
///
/// `sk` must be valid for reads and writes of 32 bytes, `tweak` for 32 reads.
pub unsafe fn secp256k1_ec_seckey_tweak_add(
    _cx: *const Context,
    sk: *mut c_uchar,
    tweak: *const c_uchar,
) -> c_int {
    let scalar = match scalar_from_bytes_nonzero(read_arr::<32>(sk)) {
        Some(scalar) => scalar,
        None => {
            ptr::write_bytes(sk, 0, 32);
            return 0;
        }
    };
    let tweak = match scalar_from_bytes(read_arr::<32>(tweak)) {
        Some(tweak) => tweak,
        None => {
            ptr::write_bytes(sk, 0, 32);
            return 0;
        }
    };
    match scalar_nonzero(scalar + tweak) {
        Some(sum) => {
            ptr::copy_nonoverlapping(scalar_to_bytes(&sum).as_ptr(), sk, 32);
            1
        }
        None => {
            ptr::write_bytes(sk, 0, 32);
            0
        }
    }
}

/// Multiplies a secret key by `tweak` in place.
///
/// # Safety
///
/// `sk` must be valid for reads and writes of 32 bytes, `tweak` for 32 reads.
pub unsafe fn secp256k1_ec_seckey_tweak_mul(
    _cx: *const Context,
    sk: *mut c_uchar,
    tweak: *const c_uchar,
) -> c_int {
    let scalar = match scalar_from_bytes_nonzero(read_arr::<32>(sk)) {
        Some(scalar) => scalar,
        None => {
            ptr::write_bytes(sk, 0, 32);
            return 0;
        }
    };
    let tweak = match scalar_from_bytes_nonzero(read_arr::<32>(tweak)) {
        Some(tweak) => tweak,
        None => {
            ptr::write_bytes(sk, 0, 32);
            return 0;
        }
    };
    ptr::copy_nonoverlapping(scalar_to_bytes(&(scalar * tweak)).as_ptr(), sk, 32);
    1
}

/* ------------------------------------------------------------------------- */
/* Keypairs and x-only keys                                                   */
/* ------------------------------------------------------------------------- */

/// Builds a keypair from a secret key.
///
/// # Safety
///
/// `keypair` must be writable and `seckey` valid for 32 bytes.
pub unsafe fn secp256k1_keypair_create(
    _cx: *const Context,
    keypair: *mut Keypair,
    seckey: *const c_uchar,
) -> c_int {
    let sk = match scalar_from_bytes_nonzero(read_arr::<32>(seckey)) {
        Some(sk) => sk,
        None => return 0,
    };
    ptr::write(keypair, save_keypair(&sk, &mul_generator(&sk)));
    1
}

/// Extracts the secret key from a keypair.
///
/// # Safety
///
/// `output_seckey` must be writable for 32 bytes and `keypair` must be valid.
pub unsafe fn secp256k1_keypair_sec(
    _cx: *const Context,
    output_seckey: *mut c_uchar,
    keypair: *const Keypair,
) -> c_int {
    ptr::copy_nonoverlapping((*keypair).0.as_ptr(), output_seckey, 32);
    1
}

/// Extracts the public key from a keypair.
///
/// # Safety
///
/// `output_pubkey` must be writable and `keypair` must be valid.
pub unsafe fn secp256k1_keypair_pub(
    _cx: *const Context,
    output_pubkey: *mut PublicKey,
    keypair: *const Keypair,
) -> c_int {
    let keypair = *keypair;
    let mut buf = [0u8; 64];
    buf[..33].copy_from_slice(&keypair.0[32..65]);
    ptr::write(output_pubkey, PublicKey(buf));
    1
}

/// Extracts the x-only public key and parity from a keypair.
///
/// # Safety
///
/// `pubkey` must be writable, `pk_parity` writable or null, `keypair` valid.
pub unsafe fn secp256k1_keypair_xonly_pub(
    _cx: *const Context,
    pubkey: *mut XOnlyPublicKey,
    pk_parity: *mut c_int,
    keypair: *const Keypair,
) -> c_int {
    let (_, pk) = match load_keypair(&*keypair) {
        Some(pair) => pair,
        None => return 0,
    };
    if !pk_parity.is_null() {
        *pk_parity = c_int::from(!point_is_y_even(&pk));
    }
    ptr::write(pubkey, save_xonly(point_x_bytes(&pk)));
    1
}

/// Applies a BIP-341 style x-only tweak to a keypair.
///
/// The secret key is first conditionally negated so that its public key has an
/// even y-coordinate, then the tweak is added.
///
/// # Safety
///
/// `keypair` must point to a valid, writable keypair and `tweak32` to 32 bytes.
pub unsafe fn secp256k1_keypair_xonly_tweak_add(
    _cx: *const Context,
    keypair: *mut Keypair,
    tweak32: *const c_uchar,
) -> c_int {
    let (sk, pk) = match load_keypair(&*keypair) {
        Some(pair) => pair,
        None => return 0,
    };
    let tweak = match scalar_from_bytes(read_arr::<32>(tweak32)) {
        Some(tweak) => tweak,
        None => return 0,
    };
    let mut sk = sk;
    scalar_conditional_negate(&mut sk, !point_is_y_even(&pk));
    let tweaked = match scalar_nonzero(sk + tweak) {
        Some(tweaked) => tweaked,
        None => return 0,
    };
    ptr::write(keypair, save_keypair(&tweaked, &mul_generator(&tweaked)));
    1
}

/// Parses a 32-byte x-only public key, lifting it to a point with even y.
///
/// # Safety
///
/// `pubkey` must be writable and `input32` valid for 32 bytes.
pub unsafe fn secp256k1_xonly_pubkey_parse(
    _cx: *const Context,
    pubkey: *mut XOnlyPublicKey,
    input32: *const c_uchar,
) -> c_int {
    let x = read_arr::<32>(input32);
    match point_lift_x(x) {
        Some(_) => {
            ptr::write(pubkey, save_xonly(x));
            1
        }
        None => 0,
    }
}

/// Serializes an x-only public key to its 32-byte x-coordinate.
///
/// # Safety
///
/// `output32` must be writable for 32 bytes and `pubkey` must be valid.
pub unsafe fn secp256k1_xonly_pubkey_serialize(
    _cx: *const Context,
    output32: *mut c_uchar,
    pubkey: *const XOnlyPublicKey,
) -> c_int {
    ptr::copy_nonoverlapping((*pubkey).0.as_ptr(), output32, 32);
    1
}

/// Converts a full public key to an x-only key plus its parity.
///
/// # Safety
///
/// `xonly_pubkey` must be writable, `pk_parity` writable or null, `pubkey` valid.
pub unsafe fn secp256k1_xonly_pubkey_from_pubkey(
    _cx: *const Context,
    xonly_pubkey: *mut XOnlyPublicKey,
    pk_parity: *mut c_int,
    pubkey: *const PublicKey,
) -> c_int {
    let point = match load_pubkey(&*pubkey) {
        Some(point) => point,
        None => return 0,
    };
    if !pk_parity.is_null() {
        *pk_parity = c_int::from(!point_is_y_even(&point));
    }
    ptr::write(xonly_pubkey, save_xonly(point_x_bytes(&point)));
    1
}

/// Compares two x-only public keys by x-coordinate.
///
/// # Safety
///
/// Both pointers must be valid.
pub unsafe fn secp256k1_xonly_pubkey_cmp(
    _cx: *const Context,
    pk1: *const XOnlyPublicKey,
    pk2: *const XOnlyPublicKey,
) -> c_int {
    match (*pk1).serialize().cmp(&(*pk2).serialize()) {
        cmp::Ordering::Less => -1,
        cmp::Ordering::Equal => 0,
        cmp::Ordering::Greater => 1,
    }
}

/// Computes `lift_x(internal_pubkey) + tweak32 * G`.
///
/// # Safety
///
/// `output_pubkey` must be writable, `internal_pubkey` valid, `tweak32` valid
/// for 32 bytes.
pub unsafe fn secp256k1_xonly_pubkey_tweak_add(
    _cx: *const Context,
    output_pubkey: *mut PublicKey,
    internal_pubkey: *const XOnlyPublicKey,
    tweak32: *const c_uchar,
) -> c_int {
    let internal = match load_xonly(&*internal_pubkey) {
        Some(point) => point,
        None => return 0,
    };
    let tweak = match load_tweak(read_arr::<32>(tweak32)) {
        Some(tweak) => tweak,
        None => return 0,
    };
    match point_add_mul_generator(&internal, &tweak) {
        Some(tweaked) => {
            ptr::write(output_pubkey, save_pubkey(&tweaked));
            1
        }
        None => 0,
    }
}

/// Checks that `tweaked_pubkey32` with `tweaked_pubkey_parity` really is
/// `lift_x(internal_pubkey) + tweak32 * G`.
///
/// # Safety
///
/// `tweaked_pubkey32` and `tweak32` must be valid for 32 bytes and
/// `internal_pubkey` must be valid.
pub unsafe fn secp256k1_xonly_pubkey_tweak_add_check(
    _cx: *const Context,
    tweaked_pubkey32: *const c_uchar,
    tweaked_pubkey_parity: c_int,
    internal_pubkey: *const XOnlyPublicKey,
    tweak32: *const c_uchar,
) -> c_int {
    let internal = match load_xonly(&*internal_pubkey) {
        Some(point) => point,
        None => return 0,
    };
    let tweak = match load_tweak(read_arr::<32>(tweak32)) {
        Some(tweak) => tweak,
        None => return 0,
    };
    let tweaked = match point_add_mul_generator(&internal, &tweak) {
        Some(tweaked) => tweaked,
        None => return 0,
    };
    let expected_x = read_arr::<32>(tweaked_pubkey32);
    let parity_matches = c_int::from(!point_is_y_even(&tweaked)) == tweaked_pubkey_parity;
    c_int::from(parity_matches && point_x_bytes(&tweaked) == expected_x)
}

/* ------------------------------------------------------------------------- */
/* ECDSA signature encoding                                                   */
/* ------------------------------------------------------------------------- */

/// Reads a DER length, following X.690 8.1.3 with DER's minimality rules.
///
/// Mirrors `secp256k1_der_read_len`.
fn der_read_len(sig: &[u8], pos: &mut usize) -> Option<usize> {
    if *pos >= sig.len() {
        return None;
    }
    let b1 = sig[*pos];
    *pos += 1;
    // X.690-0207 8.1.3.5.c: the value 0xFF shall not be used.
    if b1 == 0xFF {
        return None;
    }
    // X.690-0207 8.1.3.4: short form length octets.
    if b1 & 0x80 == 0 {
        return Some(usize::from(b1));
    }
    // Indefinite length is not allowed in DER.
    if b1 == 0x80 {
        return None;
    }
    // X.690-0207 8.1.3.5: long form length octets.
    let mut lenleft = usize::from(b1 & 0x7F);
    if lenleft > sig.len() - *pos {
        return None;
    }
    // Not the shortest possible length encoding.
    if sig[*pos] == 0 {
        return None;
    }
    if lenleft > core::mem::size_of::<usize>() {
        return None;
    }
    let mut len = 0usize;
    while lenleft > 0 {
        len = (len << 8) | usize::from(sig[*pos]);
        *pos += 1;
        lenleft -= 1;
    }
    if len > sig.len() - *pos {
        return None;
    }
    // Not the shortest possible length encoding.
    if len < 128 {
        return None;
    }
    Some(len)
}

/// Reads a DER INTEGER into a scalar, reducing out-of-range values to zero.
///
/// Mirrors `secp256k1_der_parse_integer`.
fn der_parse_integer(sig: &[u8], pos: &mut usize) -> Option<Scalar> {
    // Not a primitive integer (X.690-0207 8.3.1).
    if *pos == sig.len() || sig[*pos] != 0x02 {
        return None;
    }
    *pos += 1;
    let mut rlen = der_read_len(sig, pos)?;
    // Exceeds bounds, or not at least length 1 (X.690-0207 8.3.1).
    if rlen == 0 || rlen > sig.len() - *pos {
        return None;
    }
    // Excessive 0x00 padding.
    if sig[*pos] == 0x00 && rlen > 1 && sig[*pos + 1] & 0x80 == 0x00 {
        return None;
    }
    // Excessive 0xFF padding.
    if sig[*pos] == 0xFF && rlen > 1 && sig[*pos + 1] & 0x80 == 0x80 {
        return None;
    }
    // Negative.
    let mut overflow = sig[*pos] & 0x80 == 0x80;
    // There is at most one leading zero byte: two would have been rejected as
    // excessive 0x00 padding above.
    if rlen > 0 && sig[*pos] == 0 {
        rlen -= 1;
        *pos += 1;
    }
    if rlen > 32 {
        overflow = true;
    }
    let mut scalar = Scalar::ZERO;
    if !overflow {
        let mut buf = [0u8; 32];
        buf[32 - rlen..].copy_from_slice(&sig[*pos..*pos + rlen]);
        // `from_bytes` returns `None` exactly when the value is >= the curve
        // order, which is the C library's overflow condition.
        match scalar_from_bytes(buf) {
            Some(parsed) => scalar = parsed,
            None => overflow = true,
        }
    }
    if overflow {
        scalar = Scalar::ZERO;
    }
    *pos += rlen;
    Some(scalar)
}

/// Parses a strict DER-encoded ECDSA signature.
///
/// Mirrors `secp256k1_ecdsa_sig_parse`.
fn ecdsa_sig_parse(sig: &[u8]) -> Option<Signature> {
    // The encoding doesn't start with a constructed sequence (X.690-0207 8.9.1).
    if sig.is_empty() || sig[0] != 0x30 {
        return None;
    }
    let mut pos = 1usize;
    let rlen = der_read_len(sig, &mut pos)?;
    // Tuple exceeds bounds, or garbage after the tuple.
    if rlen != sig.len() - pos {
        return None;
    }
    let r = der_parse_integer(sig, &mut pos)?;
    let s = der_parse_integer(sig, &mut pos)?;
    // Trailing garbage inside the tuple.
    if pos != sig.len() {
        return None;
    }
    let mut out = [0u8; 64];
    out[..32].copy_from_slice(&r.to_bytes());
    out[32..].copy_from_slice(&s.to_bytes());
    Some(Signature(out))
}

/// Parses a DER-encoded ECDSA signature.
///
/// # Safety
///
/// `sig` must be writable and `input` valid for `in_len` bytes.
pub unsafe fn secp256k1_ecdsa_signature_parse_der(
    _cx: *const Context,
    sig: *mut Signature,
    input: *const c_uchar,
    in_len: size_t,
) -> c_int {
    let input = core::slice::from_raw_parts(input, in_len);
    match ecdsa_sig_parse(input) {
        Some(parsed) => {
            ptr::write(sig, parsed);
            1
        }
        None => {
            ptr::write(sig, Signature([0; 64]));
            0
        }
    }
}

/// Parses a 64-byte compact ECDSA signature.
///
/// # Safety
///
/// `sig` must be writable and `input64` valid for 64 bytes.
pub unsafe fn secp256k1_ecdsa_signature_parse_compact(
    _cx: *const Context,
    sig: *mut Signature,
    input64: *const c_uchar,
) -> c_int {
    let bytes = read_arr::<64>(input64);
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&bytes[..32]);
    s.copy_from_slice(&bytes[32..]);
    // Both halves must be canonical scalars, i.e. less than the curve order.
    if scalar_from_bytes(r).is_none() || scalar_from_bytes(s).is_none() {
        ptr::write(sig, Signature([0; 64]));
        return 0;
    }
    ptr::write(sig, Signature(bytes));
    1
}

/// Lax DER parsing. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn ecdsa_signature_parse_der_lax(
    _cx: *const Context,
    _sig: *mut Signature,
    _input: *const c_uchar,
    _in_len: size_t,
) -> c_int {
    unimplemented_op!("ecdsa_signature_parse_der_lax")
}

/// Serializes an ECDSA signature in DER.
///
/// Mirrors `secp256k1_ecdsa_sig_serialize`.
///
/// # Safety
///
/// `output` must be writable for `*out_len` bytes and `sig` must be valid.
pub unsafe fn secp256k1_ecdsa_signature_serialize_der(
    _cx: *const Context,
    output: *mut c_uchar,
    out_len: *mut size_t,
    sig: *const Signature,
) -> c_int {
    // A leading zero byte is prepended so that values with the high bit set
    // are not read back as negative; it is then stripped when unnecessary.
    let sig = *sig;
    let mut r = [0u8; 33];
    let mut s = [0u8; 33];
    r[1..].copy_from_slice(&sig.0[..32]);
    s[1..].copy_from_slice(&sig.0[32..]);

    let mut r_off = 0usize;
    let mut s_off = 0usize;
    let mut len_r = 33usize;
    let mut len_s = 33usize;
    while len_r > 1 && r[r_off] == 0 && r[r_off + 1] < 0x80 {
        len_r -= 1;
        r_off += 1;
    }
    while len_s > 1 && s[s_off] == 0 && s[s_off + 1] < 0x80 {
        len_s -= 1;
        s_off += 1;
    }

    let total = 6 + len_r + len_s;
    if *out_len < total {
        *out_len = total;
        return 0;
    }
    *out_len = total;

    let out = core::slice::from_raw_parts_mut(output, total);
    out[0] = 0x30;
    // These casts cannot truncate: len_r and len_s are at most 33.
    out[1] = (4 + len_r + len_s) as u8;
    out[2] = 0x02;
    out[3] = len_r as u8;
    out[4..4 + len_r].copy_from_slice(&r[r_off..r_off + len_r]);
    out[4 + len_r] = 0x02;
    out[5 + len_r] = len_s as u8;
    out[6 + len_r..total].copy_from_slice(&s[s_off..s_off + len_s]);
    1
}

/// Serializes an ECDSA signature in 64-byte compact form.
///
/// # Safety
///
/// `output64` must be writable for 64 bytes and `sig` must be valid.
pub unsafe fn secp256k1_ecdsa_signature_serialize_compact(
    _cx: *const Context,
    output64: *mut c_uchar,
    sig: *const Signature,
) -> c_int {
    ptr::copy_nonoverlapping((*sig).0.as_ptr(), output64, 64);
    1
}

/// Normalizes an ECDSA signature to low-s form.
///
/// Returns 1 if the input was not normalized, 0 if it already was.
///
/// # Safety
///
/// `in_sig` must be valid; `out_sig` must be writable or null.
pub unsafe fn secp256k1_ecdsa_signature_normalize(
    _cx: *const Context,
    out_sig: *mut Signature,
    in_sig: *const Signature,
) -> c_int {
    let bytes = (*in_sig).0;
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&bytes[32..]);
    let mut s = match scalar_from_bytes(s_bytes) {
        Some(s) => s,
        // Not a canonical scalar; the C library cannot represent this state, so
        // there is nothing to normalize.
        None => return 0,
    };
    let was_high = scalar_is_high(&s);
    if !out_sig.is_null() {
        scalar_conditional_negate(&mut s, was_high);
        let mut out = bytes;
        out[32..].copy_from_slice(&scalar_to_bytes(&s));
        ptr::write(out_sig, Signature(out));
    }
    c_int::from(was_high)
}

/* ------------------------------------------------------------------------- */
/* Operations that are deliberately absent                                    */
/* ------------------------------------------------------------------------- */

/// ECDSA verification. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ecdsa_verify(
    _cx: *const Context,
    _sig: *const Signature,
    _msg32: *const c_uchar,
    _pk: *const PublicKey,
) -> c_int {
    unimplemented_op!("ecdsa_verify")
}

/// ECDSA signing. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ecdsa_sign(
    _cx: *const Context,
    _sig: *mut Signature,
    _msg32: *const c_uchar,
    _sk: *const c_uchar,
    _noncefn: NonceFn,
    _noncedata: *const c_void,
) -> c_int {
    unimplemented_op!("ecdsa_sign")
}

/// BIP-340 signing. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_schnorrsig_sign(
    _cx: *const Context,
    _sig: *mut c_uchar,
    _msg32: *const c_uchar,
    _keypair: *const Keypair,
    _aux_rand32: *const c_uchar,
) -> c_int {
    unimplemented_op!("schnorrsig_sign")
}

/// BIP-340 signing with extra parameters. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_schnorrsig_sign_custom(
    _cx: *const Context,
    _sig: *mut c_uchar,
    _msg: *const c_uchar,
    _msg_len: size_t,
    _keypair: *const Keypair,
    _extra_params: *const SchnorrSigExtraParams,
) -> c_int {
    unimplemented_op!("schnorrsig_sign_custom")
}

/// BIP-340 verification. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_schnorrsig_verify(
    _cx: *const Context,
    _sig64: *const c_uchar,
    _msg32: *const c_uchar,
    _msglen: size_t,
    _pubkey: *const XOnlyPublicKey,
) -> c_int {
    unimplemented_op!("schnorrsig_verify")
}

/// ECDH. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ecdh(
    _cx: *const Context,
    _output: *mut c_uchar,
    _pubkey: *const PublicKey,
    _seckey: *const c_uchar,
    _hashfp: EcdhHashFn,
    _data: *mut c_void,
) -> c_int {
    unimplemented_op!("ecdh")
}

/// ElligatorSwift encoding. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ellswift_encode(
    _cx: *const Context,
    _ell64: *mut c_uchar,
    _pubkey: *const PublicKey,
    _rnd32: *const c_uchar,
) -> c_int {
    unimplemented_op!("ellswift_encode")
}

/// ElligatorSwift decoding. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ellswift_decode(
    _cx: *const Context,
    _pubkey: *mut u8,
    _ell64: *const c_uchar,
) -> c_int {
    unimplemented_op!("ellswift_decode")
}

/// ElligatorSwift key creation. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
pub unsafe fn secp256k1_ellswift_create(
    _cx: *const Context,
    _ell64: *mut c_uchar,
    _seckey32: *const c_uchar,
    _aux_rand32: *const c_uchar,
) -> c_int {
    unimplemented_op!("ellswift_create")
}

/// ElligatorSwift ECDH. Not implemented; see the module docs.
///
/// # Safety
///
/// Never returns.
#[allow(clippy::too_many_arguments)]
pub unsafe fn secp256k1_ellswift_xdh(
    _cx: *const Context,
    _output: *mut c_uchar,
    _ell_a64: *const c_uchar,
    _ell_b64: *const c_uchar,
    _seckey32: *const c_uchar,
    _party: c_int,
    _hashfp: EllswiftEcdhHashFn,
    _data: *mut c_void,
) -> c_int {
    unimplemented_op!("ellswift_xdh")
}

#[cfg(feature = "recovery")]
pub mod recovery;
