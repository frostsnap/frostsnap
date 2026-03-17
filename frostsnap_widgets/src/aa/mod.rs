pub mod circle;
pub mod rounded_rect;

use crate::Frac;

/// Scale factor for fixed-point SDF calculations
pub(crate) const SCALE: i64 = 256;

#[inline]
pub(crate) fn coverage_from_distance(distance_scaled: i64) -> Frac {
    let half = SCALE / 2;
    let clamped = (half - distance_scaled).clamp(0, SCALE);
    Frac::from_ratio(clamped as u32, SCALE as u32)
}

#[inline]
pub(crate) fn isqrt_distance(dx: i64, dy: i64, radius_scaled: i64) -> i64 {
    (dx * dx + dy * dy).unsigned_abs().isqrt() as i64 - radius_scaled
}
