//! Precomputed circle coverage map and framebuffer construction.
//!
//! The build script precomputes SDF coverage values for one quadrant of the circle button.
//! This module applies colors to the coverage map to produce full Rgb565 framebuffers,
//! avoiding expensive sqrt-per-pixel SDF math at runtime.

use crate::vec_framebuffer::VecFramebuffer;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};

/// Coverage map for the top-left quadrant (50×50 pixels).
/// Each pixel is 2 bytes: (shape_coverage_u8, fill_coverage_u8).
static COVERAGE_MAP: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/circle_coverage.bin"));

const QUADRANT: usize = 50;
const DIAMETER: usize = 100;

/// Build a full 100×100 Rgb565 framebuffer from the coverage map by applying colors
/// and mirroring the top-left quadrant to all four quadrants.
pub fn build_circle_fb(
    fill_color: Rgb565,
    stroke_color: Rgb565,
    bg_color: Rgb565,
) -> VecFramebuffer<Rgb565> {
    let mut fb = VecFramebuffer::<Rgb565>::new(DIAMETER, DIAMETER);
    fb.clear(bg_color);

    let fill_r = fill_color.r() as u16;
    let fill_g = fill_color.g() as u16;
    let fill_b = fill_color.b() as u16;
    let stroke_r = stroke_color.r() as u16;
    let stroke_g = stroke_color.g() as u16;
    let stroke_b = stroke_color.b() as u16;
    let bg_r = bg_color.r() as u16;
    let bg_g = bg_color.g() as u16;
    let bg_b = bg_color.b() as u16;

    for qy in 0..QUADRANT {
        for qx in 0..QUADRANT {
            let idx = (qy * QUADRANT + qx) * 2;
            let shape_cov = COVERAGE_MAP[idx] as u16;
            let fill_cov = COVERAGE_MAP[idx + 1] as u16;

            if shape_cov == 0 {
                continue; // background pixel, already cleared
            }

            let stroke_cov = shape_cov - fill_cov;
            let bg_cov = 255 - shape_cov;

            let r = (stroke_r * stroke_cov + fill_r * fill_cov + bg_r * bg_cov) / 255;
            let g = (stroke_g * stroke_cov + fill_g * fill_cov + bg_g * bg_cov) / 255;
            let b = (stroke_b * stroke_cov + fill_b * fill_cov + bg_b * bg_cov) / 255;

            let color = Rgb565::new(r as u8, g as u8, b as u8);

            // Mirror to all 4 quadrants
            let mx = DIAMETER - 1 - qx;
            let my = DIAMETER - 1 - qy;

            fb.set_pixel(embedded_graphics::prelude::Point::new(qx as i32, qy as i32), color);
            fb.set_pixel(embedded_graphics::prelude::Point::new(mx as i32, qy as i32), color);
            fb.set_pixel(embedded_graphics::prelude::Point::new(qx as i32, my as i32), color);
            fb.set_pixel(embedded_graphics::prelude::Point::new(mx as i32, my as i32), color);
        }
    }

    fb
}
