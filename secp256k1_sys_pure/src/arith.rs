// SPDX-License-Identifier: CC0-1.0

//! Point and scalar helpers built on [`secp256kfun_k256`].
//!
//! The backend crate provides field, scalar and curve arithmetic but no
//! encoding, so SEC1 serialization, x-only lifting and the non-identity checks
//! the C API relies on live here.

use secp256kfun_k256::subtle::{ConditionallyNegatable, ConstantTimeEq};
use secp256kfun_k256::{AffinePoint, FieldBytes, FieldElement, ProjectivePoint};
pub use secp256kfun_k256::{ProjectivePoint as PointAcc, Scalar};

/// A curve point known not to be the identity.
///
/// The C API has no encoding for the identity, so every point that reaches or
/// leaves the FFI boundary is of this shape.
pub type Point = AffinePoint;

/* ------------------------------------------------------------------------- */
/* Scalars                                                                    */
/* ------------------------------------------------------------------------- */

/// Parses a big-endian scalar, rejecting values that are not less than the
/// curve order.
pub fn scalar_from_bytes(bytes: [u8; 32]) -> Option<Scalar> {
    Scalar::from_repr(FieldBytes::from(bytes))
}

/// As [`scalar_from_bytes`], additionally rejecting zero.
pub fn scalar_from_bytes_nonzero(bytes: [u8; 32]) -> Option<Scalar> {
    scalar_from_bytes(bytes).and_then(scalar_nonzero)
}

/// Returns `scalar` unless it is zero.
pub fn scalar_nonzero(scalar: Scalar) -> Option<Scalar> {
    if bool::from(scalar.is_zero()) {
        None
    } else {
        Some(scalar)
    }
}

/// Big-endian encoding of a scalar.
pub fn scalar_to_bytes(scalar: &Scalar) -> [u8; 32] {
    scalar.to_bytes().into()
}

/// Whether the scalar is greater than half the curve order.
pub fn scalar_is_high(scalar: &Scalar) -> bool {
    bool::from(scalar.is_high())
}

/// Negates `scalar` if `cond`, without branching on it.
pub fn scalar_conditional_negate(scalar: &mut Scalar, cond: bool) {
    scalar.conditional_negate((cond as u8).into())
}

/* ------------------------------------------------------------------------- */
/* Point encoding                                                             */
/* ------------------------------------------------------------------------- */

/// Parses a 33-byte SEC1 compressed point.
pub fn point_from_bytes(bytes: [u8; 33]) -> Option<Point> {
    let y_is_odd = match bytes[0] {
        0x02 => 0u8,
        0x03 => 1u8,
        _ => return None,
    };
    let mut x = [0u8; 32];
    x.copy_from_slice(&bytes[1..]);
    Option::from(AffinePoint::decompress(
        &FieldBytes::from(x),
        y_is_odd.into(),
    ))
}

/// Parses a 65-byte SEC1 uncompressed point, checking that it is on the curve.
pub fn point_from_bytes_uncompressed(bytes: [u8; 65]) -> Option<Point> {
    if bytes[0] != 0x04 {
        return None;
    }
    let mut x = [0u8; 32];
    x.copy_from_slice(&bytes[1..33]);
    let mut y = [0u8; 32];
    y.copy_from_slice(&bytes[33..]);
    let y: FieldElement = Option::from(FieldElement::from_bytes(&FieldBytes::from(y)))?;
    let y = y.normalize();
    // Deriving y from x and comparing is what checks the curve equation.
    let point: Point = Option::from(AffinePoint::decompress(&FieldBytes::from(x), y.is_odd()))?;
    if bool::from(point.y.ct_eq(&y)) {
        Some(point)
    } else {
        None
    }
}

/// Lifts an x-coordinate to the point with even y, as in BIP-340 `lift_x`.
pub fn point_lift_x(x: [u8; 32]) -> Option<Point> {
    Option::from(AffinePoint::decompress(&FieldBytes::from(x), 0u8.into()))
}

/// SEC1 compressed encoding.
pub fn point_to_bytes(point: &Point) -> [u8; 33] {
    let mut out = [0u8; 33];
    out[0] = if point_is_y_even(point) { 0x02 } else { 0x03 };
    out[1..].copy_from_slice(&point.x.to_bytes());
    out
}

/// SEC1 uncompressed encoding.
pub fn point_to_bytes_uncompressed(point: &Point) -> [u8; 65] {
    let mut out = [0u8; 65];
    out[0] = 0x04;
    out[1..33].copy_from_slice(&point.x.to_bytes());
    out[33..].copy_from_slice(&point.y.to_bytes());
    out
}

/// The x-coordinate, as it appears in an x-only public key.
pub fn point_x_bytes(point: &Point) -> [u8; 32] {
    point.x.to_bytes().into()
}

/// Whether the y-coordinate is even.
pub fn point_is_y_even(point: &Point) -> bool {
    bool::from(point.y.normalize().is_even())
}

/* ------------------------------------------------------------------------- */
/* Point arithmetic                                                           */
/* ------------------------------------------------------------------------- */

/// Converts an accumulator back to a point, or `None` if it is the identity.
pub fn point_from_acc(acc: PointAcc) -> Option<Point> {
    if bool::from(acc.is_identity()) {
        None
    } else {
        Some(acc.to_affine())
    }
}

/// `-point`.
pub fn point_negate(point: &Point) -> Point {
    -*point
}

/// `point + scalar * G`, or `None` if that is the identity.
pub fn point_add_mul_generator(point: &Point, scalar: &Scalar) -> Option<Point> {
    point_from_acc(ProjectivePoint::from(point) + ProjectivePoint::GENERATOR * scalar)
}

/// `scalar * point`, or `None` if that is the identity.
pub fn point_mul_scalar(point: &Point, scalar: &Scalar) -> Option<Point> {
    point_from_acc(ProjectivePoint::from(point) * scalar)
}

/// `scalar * G`, or `None` if `scalar` is zero.
pub fn mul_generator(scalar: &Scalar) -> Option<Point> {
    point_from_acc(ProjectivePoint::GENERATOR * scalar)
}
