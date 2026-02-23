// Build script: precomputes hold-to-confirm border pixels and circle coverage map at build time.
//
// The SDF math is duplicated here because build scripts can't import from the crate being built.

use std::env;
use std::fs;
use std::path::Path;

// Screen and border constants (must match runtime values)
const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 280;
const BORDER_WIDTH: u32 = 5;
const CORNER_RADIUS: f32 = 42.0;

// Circle constants (must match circle_button.rs)
const CIRCLE_RADIUS: u32 = 50;
const SDF_CIRCLE_RADIUS: f32 = 48.0;
const SDF_STROKE_WIDTH: f32 = 2.0;

// --- Duplicated SDF functions (from src/sdf.rs) ---

fn sdf_rounded_rect(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
    corner_r: f32,
) -> f32 {
    let dx = (px - cx).abs() - (half_w - corner_r);
    let dy = (py - cy).abs() - (half_h - corner_r);

    let dx_pos = if dx > 0.0 { dx } else { 0.0 };
    let dy_pos = if dy > 0.0 { dy } else { 0.0 };

    let outside = (dx_pos * dx_pos + dy_pos * dy_pos).sqrt();
    let inside = if dx > dy { dx } else { dy };
    let inside = if inside < 0.0 { inside } else { 0.0 };

    outside + inside - corner_r
}

fn sdf_circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    let dx = px - cx;
    let dy = py - cy;
    (dx * dx + dy * dy).sqrt() - r
}

fn sdf_coverage(distance: f32) -> f32 {
    let v = 0.5 - distance;
    if v <= 0.0 {
        0.0
    } else if v >= 1.0 {
        1.0
    } else {
        v
    }
}

fn coverage_to_gray4(coverage: f32) -> u8 {
    let v = coverage * 15.0 + 0.5;
    if v <= 0.0 {
        0
    } else if v >= 15.0 {
        15
    } else {
        v as u8
    }
}

// --- Border pixel generation (mirrors record_border_pixels logic) ---

fn generate_border_pixels() -> Vec<(u8, u16, u8)> {
    let middle_x = SCREEN_WIDTH as i32 / 2;
    let w = SCREEN_WIDTH as f32;
    let h = SCREEN_HEIGHT as f32;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let half_w = cx;
    let half_h = cy;
    let sw = BORDER_WIDTH as f32;

    let outer_cr = CORNER_RADIUS;
    let inner_cr = (CORNER_RADIUS - sw).max(0.0);
    let inner_half_w = half_w - sw;
    let inner_half_h = half_h - sw;

    let corner_margin = CORNER_RADIUS as i32 + 2;
    let stroke_margin = BORDER_WIDTH as i32 + 2;

    let mut pixels = Vec::new();

    for y in 0..SCREEN_HEIGHT as i32 {
        for x in 0..=middle_x {
            let in_top_corner_region = y < corner_margin && x < corner_margin;
            let in_bottom_corner_region =
                y >= (SCREEN_HEIGHT as i32 - corner_margin) && x < corner_margin;
            let in_left_edge = x < stroke_margin;
            let in_top_edge = y < stroke_margin;
            let in_bottom_edge = y >= (SCREEN_HEIGHT as i32 - stroke_margin);

            let near_border = in_top_corner_region
                || in_bottom_corner_region
                || in_left_edge
                || in_top_edge
                || in_bottom_edge;

            if !near_border {
                continue;
            }

            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let d_outer = sdf_rounded_rect(px, py, cx, cy, half_w, half_h, outer_cr);
            let d_inner =
                sdf_rounded_rect(px, py, cx, cy, inner_half_w, inner_half_h, inner_cr);

            let outer_cov = sdf_coverage(d_outer);
            let inner_cov = sdf_coverage(d_inner);
            let stroke_cov = outer_cov - inner_cov;
            let stroke_cov = if stroke_cov > 0.0 { stroke_cov } else { 0.0 };

            let level = coverage_to_gray4(stroke_cov);
            if level > 0 {
                pixels.push((x as u8, y as u16, level));
            }
        }
    }

    // Sort identically to record_border_pixels
    let bucket_margin = BORDER_WIDTH as i32 + 2;
    pixels.sort_unstable_by_key(|&(x, y, _)| {
        let mut y_bucket = y as i32;

        if y_bucket < bucket_margin {
            y_bucket = 0;
        } else if y_bucket > (SCREEN_HEIGHT as i32 - bucket_margin - 1) {
            y_bucket = i32::MAX;
        }

        let x_distance = middle_x - x as i32;
        let final_distance = if (y as i32) > SCREEN_HEIGHT as i32 / 2 {
            -x_distance
        } else {
            x_distance
        };

        (y_bucket, final_distance)
    });

    pixels
}

// --- Circle coverage map generation ---
//
// The circle button uses a stroked+filled circle rendered via SDF. The SDF math (sqrt per pixel)
// is expensive on the ESP32, but the coverage values are color-independent — only the final
// color blend depends on which palette colors are used.
//
// We precompute a coverage map for one quadrant (the circle has 4-fold symmetry) storing
// (shape_coverage, fill_coverage) per pixel as u8 values (0–255). At runtime, applying colors
// is just integer multiply+shift — no sqrt needed.
//
// Quadrant size: 50×50 = 2500 pixels × 2 bytes = 5000 bytes.

const QUADRANT: usize = CIRCLE_RADIUS as usize; // 50

/// Quantize a 0.0–1.0 coverage to 0–255 u8.
fn cov_to_u8(c: f32) -> u8 {
    let v = c * 255.0 + 0.5;
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

fn generate_circle_coverage(out_dir: &Path) {
    let cx = CIRCLE_RADIUS as f32; // 50.0
    let half_sw = SDF_STROKE_WIDTH * 0.5;

    // Coverage map for top-left quadrant: x in [0, QUADRANT), y in [0, QUADRANT)
    // Stored as interleaved (shape_cov, fill_cov) pairs.
    let mut map = Vec::with_capacity(QUADRANT * QUADRANT * 2);

    for y in 0..QUADRANT {
        for x in 0..QUADRANT {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let d = sdf_circle(px, py, cx, cx, SDF_CIRCLE_RADIUS);
            let shape_cov = sdf_coverage(d - half_sw);
            let fill_cov = sdf_coverage(d + half_sw);

            map.push(cov_to_u8(shape_cov));
            map.push(cov_to_u8(fill_cov));
        }
    }

    // Write as binary file for include_bytes!
    let dest = out_dir.join("circle_coverage.bin");
    fs::write(&dest, &map).unwrap();
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // Generate border pixels
    let border_dest = out_path.join("border_pixels.rs");
    let pixels = generate_border_pixels();

    let mut code = String::new();
    code.push_str("// Generated by build.rs - do not edit\n");
    code.push_str("pub const BORDER_PIXELS: &[(u8, u16, u8)] = &[\n");
    for (x, y, cov) in &pixels {
        code.push_str(&format!("    ({}, {}, {}),\n", x, y, cov));
    }
    code.push_str("];\n");
    fs::write(&border_dest, code).unwrap();

    // Generate circle coverage map
    generate_circle_coverage(out_path);

    // Only rerun if build.rs itself changes (the inputs are all constants)
    println!("cargo:rerun-if-changed=build.rs");
}
