//! Corrects the touch sensor's offset from where the finger actually is.

use embedded_graphics::prelude::Point;
use fixed::types::I16F16;

/// Fitted polynomial of the x error, lowest degree first.
const X_FIT: [f64; 8] = [
    -20.0,
    0.8356,
    -1.2229e-2,
    6.4233e-5,
    3.2578e-8,
    -7.6483e-10,
    -2.1879e-12,
    1.3189e-14,
];
/// Fitted polynomial of the y error, lowest degree first.
const Y_FIT: [f64; 5] = [40.0, -2.3443e-02, -1.5104e-02, 1.7576e-04, -5.5439e-07];

const X_POLY: [I16F16; 8] = scale_to_unit_input(X_FIT);
const Y_POLY: [I16F16; 5] = scale_to_unit_input(Y_FIT);

/// Rewrites `sum(a_k * v^k)` as `sum((a_k * 256^k) * u^k)` with `u = v / 256`, which keeps every
/// Horner partial inside `I16F16`'s range for `v` in 0..=255.
const fn scale_to_unit_input<const N: usize>(fitted: [f64; N]) -> [I16F16; N] {
    let mut out = [I16F16::ZERO; N];
    let mut scale = 1.0;
    let mut k = 0;
    while k < N {
        let bits = fitted[k] * scale * 65536.0;
        let rounded = if bits < 0.0 { bits - 0.5 } else { bits + 0.5 };
        out[k] = I16F16::from_bits(rounded as i32);
        scale *= 256.0;
        k += 1;
    }
    out
}

fn eval(poly: &[I16F16], v: i32) -> i32 {
    // Clamped because the fit is only valid on the panel, and beyond 255 the partials overflow.
    let u = I16F16::from_bits(v.clamp(0, 255) << 8);
    let corrected = poly.iter().rev().fold(I16F16::ZERO, |acc, &a| acc * u + a);
    (-corrected).round_to_zero().to_num()
}

/// The y offset to apply at raw sensor column `x`.
pub fn x_based_adjustment(x: i32) -> i32 {
    eval(&X_POLY, x)
}

/// The y offset to apply at raw sensor row `y`.
pub fn y_based_adjustment(y: i32) -> i32 {
    if y > 170 {
        return 0;
    }
    eval(&Y_POLY, y)
}

/// Maps a raw sensor point to where the finger is on screen.
pub fn adjust_touch_point(mut point: Point) -> Point {
    point.y += x_based_adjustment(point.x) + y_based_adjustment(point.y);
    point.x = point.x.max(0);
    point.y = point.y.max(0);
    point
}
