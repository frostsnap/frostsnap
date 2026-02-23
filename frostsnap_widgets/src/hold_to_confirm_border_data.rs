use crate::compressed_point::CompressedPointWithCoverage;
use alloc::vec::Vec;

include!(concat!(env!("OUT_DIR"), "/border_pixels.rs"));

/// Load the precomputed border pixels into a Vec.
/// This is a single allocation with no SDF math — the data was computed at build time.
pub fn load_border_pixels() -> Vec<CompressedPointWithCoverage> {
    BORDER_PIXELS
        .iter()
        .map(|&(x, y, coverage)| CompressedPointWithCoverage {
            x,
            y,
            coverage,
        })
        .collect()
}
