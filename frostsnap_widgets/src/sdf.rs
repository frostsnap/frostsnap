//! Signed Distance Field (SDF) functions for anti-aliased shape rendering.
//!
//! Provides smooth, anti-aliased edges for circles and rounded rectangles by computing
//! per-pixel signed distances to shape boundaries and converting them to coverage values.

use embedded_graphics::pixelcolor::{Rgb565, RgbColor};

// F32Ext provides sqrt() and abs() for no_std environments
#[allow(unused_imports)]
use micromath::F32Ext as _;

/// Signed distance from point (px, py) to circle boundary centered at (cx, cy) with radius r.
/// Negative inside, positive outside.
#[inline]
pub fn sdf_circle(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    let dx = px - cx;
    let dy = py - cy;
    (dx * dx + dy * dy).sqrt() - r
}

/// Signed distance from point (px, py) to a rounded rectangle boundary.
/// `cx`, `cy`: center of the rectangle
/// `half_w`, `half_h`: half-width and half-height of the rectangle
/// `corner_r`: corner radius
/// Negative inside, positive outside.
#[inline]
pub fn sdf_rounded_rect(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
    corner_r: f32,
) -> f32 {
    // Work in the first quadrant (absolute offset from center)
    let dx = (px - cx).abs() - (half_w - corner_r);
    let dy = (py - cy).abs() - (half_h - corner_r);

    let dx_pos = if dx > 0.0 { dx } else { 0.0 };
    let dy_pos = if dy > 0.0 { dy } else { 0.0 };

    let outside = (dx_pos * dx_pos + dy_pos * dy_pos).sqrt();
    let inside = if dx > dy { dx } else { dy };
    let inside = if inside < 0.0 { inside } else { 0.0 };

    outside + inside - corner_r
}

/// Convert a signed distance to a coverage value (0.0–1.0).
/// 1.0 when fully inside, 0.0 when fully outside, smooth transition over ~1px.
#[inline]
pub fn sdf_coverage(distance: f32) -> f32 {
    let v = 0.5 - distance;
    if v <= 0.0 {
        0.0
    } else if v >= 1.0 {
        1.0
    } else {
        v
    }
}

/// Build a 16-entry lookup table mapping Gray4 coverage levels (0–15) to blended Rgb565 colors.
/// Level 0 = pure background, level 15 = pure foreground.
pub fn build_aa_lut(fg: Rgb565, bg: Rgb565) -> [Rgb565; 16] {
    let mut lut = [bg; 16];
    for i in 1..16u8 {
        let alpha = i as u16;
        let inv = 15 - alpha;

        let r = (bg.r() as u16 * inv + fg.r() as u16 * alpha + 7) / 15;
        let g = (bg.g() as u16 * inv + fg.g() as u16 * alpha + 7) / 15;
        let b = (bg.b() as u16 * inv + fg.b() as u16 * alpha + 7) / 15;

        lut[i as usize] = Rgb565::new(r as u8, g as u8, b as u8);
    }
    lut
}

/// Convert a coverage value (0.0–1.0) to a Gray4 level (0–15).
#[inline]
pub fn coverage_to_gray4(coverage: f32) -> u8 {
    let v = coverage * 15.0 + 0.5;
    if v <= 0.0 {
        0
    } else if v >= 15.0 {
        15
    } else {
        v as u8
    }
}

/// Render an anti-aliased filled circle with optional stroke into an Rgb565 VecFramebuffer.
///
/// The framebuffer should be pre-cleared with `bg_color`.
/// The circle is centered at (`cx`, `cy`) with the given `radius`.
/// If `stroke_width` is provided, a stroke of that width is drawn on the circle boundary,
/// centered on the edge (half inside, half outside the fill radius).
///
/// The rendering uses a two-region SDF approach:
/// - Fill region: inside the outer edge of the stroke (or the circle if no stroke)
/// - Stroke region: the annular band of the stroke
pub fn render_circle_aa(
    fb: &mut crate::vec_framebuffer::VecFramebuffer<Rgb565>,
    cx: f32,
    cy: f32,
    radius: f32,
    stroke_width: Option<f32>,
    fill_color: Rgb565,
    stroke_color: Rgb565,
    bg_color: Rgb565,
) {
    let fill_lut = build_aa_lut(fill_color, bg_color);

    let sw = stroke_width.unwrap_or(0.0);
    let half_sw = sw * 0.5;

    // The outer boundary of the shape (including stroke)
    let outer_r = radius + half_sw;

    // Bounding box (with 1px margin for AA fringe)
    let min_x = ((cx - outer_r - 1.0) as i32).max(0);
    let min_y = ((cy - outer_r - 1.0) as i32).max(0);
    let max_x = ((cx + outer_r + 1.0) as i32 + 1).min(fb.width as i32);
    let max_y = ((cy + outer_r + 1.0) as i32 + 1).min(fb.height as i32);

    for y in min_y..max_y {
        for x in min_x..max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let d = sdf_circle(px, py, cx, cy, radius);

            if sw > 0.0 {
                // d_outer: SDF for circle of radius (R + half_sw) — the full shape boundary
                let d_outer = d - half_sw;
                // d_inner: SDF for circle of radius (R - half_sw) — the fill interior
                let d_inner = d + half_sw;

                // shape_cov: coverage of full shape (fill + stroke)
                let shape_cov = sdf_coverage(d_outer);
                // fill_cov: coverage of just the interior (inside the stroke)
                let fill_cov = sdf_coverage(d_inner);
                // stroke_cov: the annular band (shape minus interior)
                let stroke_cov = shape_cov - fill_cov;
                let stroke_cov = if stroke_cov > 0.0 { stroke_cov } else { 0.0 };

                let fill_level = coverage_to_gray4(fill_cov);
                let stroke_level = coverage_to_gray4(stroke_cov);

                if fill_level > 0 || stroke_level > 0 {
                    let pt = embedded_graphics::prelude::Point::new(x, y);
                    if stroke_level > 0 && fill_level > 0 {
                        // Blend: stroke over fill over background
                        let fill_pixel = fill_lut[fill_level as usize];
                        let stroke_over_fill_lut = build_aa_lut(stroke_color, fill_pixel);
                        fb.set_pixel(pt, stroke_over_fill_lut[stroke_level as usize]);
                    } else if stroke_level > 0 {
                        // Stroke pixel on background
                        let stroke_lut = build_aa_lut(stroke_color, bg_color);
                        fb.set_pixel(pt, stroke_lut[stroke_level as usize]);
                    } else {
                        // Fill pixel on background
                        fb.set_pixel(pt, fill_lut[fill_level as usize]);
                    }
                }
            } else {
                // Fill only — sdf_coverage handles the SDF convention directly
                let cov = sdf_coverage(d);
                let level = coverage_to_gray4(cov);
                if level > 0 {
                    fb.set_pixel(
                        embedded_graphics::prelude::Point::new(x, y),
                        fill_lut[level as usize],
                    );
                }
            }
        }
    }
}

/// Render an anti-aliased rounded rectangle stroke directly via pixel iterator.
///
/// For each pixel in the border region, evaluates the SDF and emits a blended Rgb565 color.
/// Returns an iterator of `Pixel<Rgb565>` suitable for `draw_iter`.
///
/// `rect_x`, `rect_y`: top-left of the rectangle
/// `rect_w`, `rect_h`: width and height of the rectangle
/// `corner_radius`: corner radius
/// `stroke_width`: stroke width (inside-aligned, matching embedded-graphics StrokeAlignment::Inside)
pub fn render_rounded_rect_stroke_pixels(
    rect_x: i32,
    rect_y: i32,
    rect_w: u32,
    rect_h: u32,
    corner_radius: f32,
    stroke_width: f32,
    stroke_color: Rgb565,
    bg_color: Rgb565,
    fill_color: Option<Rgb565>,
) -> alloc::vec::Vec<embedded_graphics::Pixel<Rgb565>> {
    use embedded_graphics::prelude::Point;
    use embedded_graphics::Pixel;

    let mut pixels = alloc::vec::Vec::new();

    let stroke_lut = build_aa_lut(stroke_color, bg_color);

    // For inside-aligned stroke, the outer boundary is the rectangle edge
    // and the inner boundary is inset by stroke_width
    let outer_half_w = rect_w as f32 * 0.5;
    let outer_half_h = rect_h as f32 * 0.5;
    let cx = rect_x as f32 + outer_half_w;
    let cy = rect_y as f32 + outer_half_h;

    // Outer rounded rect: corner_radius as given
    let outer_cr = corner_radius;
    // Inner rounded rect: inset by stroke_width, corner radius reduced
    let inner_cr = (corner_radius - stroke_width).max(0.0);
    let inner_half_w = outer_half_w - stroke_width;
    let inner_half_h = outer_half_h - stroke_width;

    // We need to evaluate pixels near the border.
    // Corner regions extend up to corner_radius pixels from each edge,
    // while straight edges only extend stroke_width pixels.
    let corner_margin = corner_radius as i32 + 2; // corner region + AA fringe
    let stroke_margin = stroke_width as i32 + 2; // straight edge + AA fringe

    for y in rect_y..(rect_y + rect_h as i32) {
        for x in rect_x..(rect_x + rect_w as i32) {
            let local_x = x - rect_x;
            let local_y = y - rect_y;

            // Skip interior pixels (not near any border)
            let in_left_edge = local_x < stroke_margin;
            let in_right_edge = local_x >= (rect_w as i32 - stroke_margin);
            let in_top_edge = local_y < stroke_margin;
            let in_bottom_edge = local_y >= (rect_h as i32 - stroke_margin);
            let in_corner = (local_x < corner_margin || local_x >= (rect_w as i32 - corner_margin))
                && (local_y < corner_margin || local_y >= (rect_h as i32 - corner_margin));

            let near_border =
                in_left_edge || in_right_edge || in_top_edge || in_bottom_edge || in_corner;

            if !near_border && fill_color.is_none() {
                continue;
            }

            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let d_outer = sdf_rounded_rect(px, py, cx, cy, outer_half_w, outer_half_h, outer_cr);

            if near_border {
                let d_inner =
                    sdf_rounded_rect(px, py, cx, cy, inner_half_w, inner_half_h, inner_cr);

                // outer_cov: coverage of full shape (inside outer boundary)
                let outer_cov = sdf_coverage(d_outer);
                // inner_cov: coverage of interior (inside inner boundary)
                let inner_cov = sdf_coverage(d_inner);
                // stroke_cov: the border band = shape minus interior
                let stroke_cov = outer_cov - inner_cov;
                let stroke_cov = if stroke_cov > 0.0 { stroke_cov } else { 0.0 };

                let level = coverage_to_gray4(stroke_cov);
                if level > 0 {
                    let bg = if let Some(fc) = fill_color {
                        // If we have fill, blend stroke over fill when inside inner boundary
                        if d_inner < 0.5 {
                            fc
                        } else {
                            bg_color
                        }
                    } else {
                        bg_color
                    };
                    let lut = if bg == bg_color {
                        stroke_lut
                    } else {
                        build_aa_lut(stroke_color, bg)
                    };
                    pixels.push(Pixel(Point::new(x, y), lut[level as usize]));
                } else if let Some(fc) = fill_color {
                    // Inside the fill area (no stroke here)
                    let fill_cov = sdf_coverage(d_outer);
                    if fill_cov > 0.0 {
                        let fill_lut = build_aa_lut(fc, bg_color);
                        let fl = coverage_to_gray4(fill_cov);
                        if fl > 0 {
                            pixels.push(Pixel(Point::new(x, y), fill_lut[fl as usize]));
                        }
                    }
                }
            } else if let Some(fc) = fill_color {
                // Interior pixel with fill
                pixels.push(Pixel(Point::new(x, y), fc));
            }
        }
    }

    pixels
}
