use super::{
    compressed_point::CompressedPointWithCoverage,
    rat::Frac,
    Widget,
};
use crate::fader::FadingDrawTarget;
use crate::sdf;
use crate::super_draw_target::SuperDrawTarget;
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{PixelColor, Rgb565},
    prelude::*,
    primitives::Rectangle,
};

/// Generate both the original and mirrored pixel for a given compressed point with coverage.
/// Uses the provided LUT to map coverage (0–15) to a blended color.
fn mirror_pixel_aa(
    pixel: CompressedPointWithCoverage,
    screen_width: i32,
    lut: &[Rgb565; 16],
) -> [Pixel<Rgb565>; 2] {
    let point = pixel.to_point();
    let mirrored_x = screen_width - 1 - point.x;
    let color = lut[pixel.coverage as usize];
    [
        Pixel(point, color),
        Pixel(Point::new(mirrored_x, point.y), color),
    ]
}

/// Generate both the original and mirrored pixel with a flat color (for erasing).
fn mirror_pixel_flat(
    pixel: CompressedPointWithCoverage,
    screen_width: i32,
    color: Rgb565,
) -> [Pixel<Rgb565>; 2] {
    let point = pixel.to_point();
    let mirrored_x = screen_width - 1 - point.x;
    [
        Pixel(point, color),
        Pixel(Point::new(mirrored_x, point.y), color),
    ]
}

#[derive(Debug, PartialEq)]
pub struct HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub child: W,
    progress: Frac, // 0 to 1 (percentage of border completed)
    last_drawn_progress: Frac,
    constraints: Option<Size>,
    sizing: crate::Sizing,
    border_pixels: Vec<CompressedPointWithCoverage>, // Recorded border pixels with AA coverage
    border_width: u32,
    border_color: C,
    background_color: C,
    // Fade state for border only
    fade_progress: Frac,
    fade_start_time: Option<crate::Instant>,
    fade_duration_ms: u64,
    is_fading: bool,
}

impl<W, C> HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub fn new(child: W, border_width: u32, border_color: C, background_color: C) -> Self {
        Self {
            child,
            progress: Frac::ZERO,
            last_drawn_progress: Frac::ZERO,
            constraints: None,
            sizing: crate::Sizing {
                width: 0,
                height: 0,
                ..Default::default()
            },
            border_pixels: Vec::new(),
            border_width,
            border_color,
            background_color,
            fade_progress: Frac::ZERO,
            fade_start_time: None,
            fade_duration_ms: 0,
            is_fading: false,
        }
    }

    pub fn set_progress(&mut self, progress: Frac) {
        self.progress = progress;
    }

    pub fn get_progress(&self) -> Frac {
        self.progress
    }

    pub fn border_width(&self) -> u32 {
        self.border_width
    }

    pub fn start_fade_out(&mut self, duration_ms: u64) {
        self.is_fading = true;
        self.fade_duration_ms = duration_ms;
        self.fade_start_time = None; // Will be set on first draw
        self.fade_progress = Frac::ZERO;
    }

    pub fn is_faded_out(&self) -> bool {
        self.is_fading && self.fade_progress == Frac::ONE
    }

    pub fn set_border_color(&mut self, color: C) {
        self.border_color = color;
        // Force redraw with new color
        self.last_drawn_progress = Frac::from_ratio(u32::MAX, u32::MAX); // Force redraw
    }

    pub fn set_background_color(&mut self, color: C) {
        self.background_color = color;
    }

    fn record_border_pixels(&mut self, screen_size: Size) {
        const CORNER_RADIUS: f32 = 42.0;

        let middle_x = screen_size.width as i32 / 2;
        let w = screen_size.width as f32;
        let h = screen_size.height as f32;
        let cx = w * 0.5;
        let cy = h * 0.5;
        let half_w = cx;
        let half_h = cy;
        let sw = self.border_width as f32;

        // For inside-aligned stroke: outer boundary is the rect edge, inner is inset
        let outer_cr = CORNER_RADIUS;
        let inner_cr = (CORNER_RADIUS - sw).max(0.0);
        let inner_half_w = half_w - sw;
        let inner_half_h = half_h - sw;

        let mut pixels = Vec::new();

        // Only record left-half pixels (will be mirrored)
        // The border_margin must account for the corner radius, not just stroke width,
        // because the rounded corners curve inward from both edges simultaneously.
        let corner_margin = CORNER_RADIUS as i32 + 2; // corner region + AA fringe
        let stroke_margin = self.border_width as i32 + 2; // straight edge region + AA fringe

        for y in 0..screen_size.height as i32 {
            for x in 0..=middle_x {
                // Skip pixels that are far from any border edge.
                // A pixel is near the border if it's:
                // - In the top/bottom corner regions (y < corner_margin or y >= height - corner_margin)
                //   AND within the corner's x range (x < corner_margin)
                // - In the straight left/right edge bands (x < stroke_margin)
                // - In the straight top/bottom edge bands (y < stroke_margin or y >= height - stroke_margin)
                let in_top_corner_region = y < corner_margin && x < corner_margin;
                let in_bottom_corner_region =
                    y >= (screen_size.height as i32 - corner_margin) && x < corner_margin;
                let in_left_edge = x < stroke_margin;
                let in_top_edge = y < stroke_margin;
                let in_bottom_edge = y >= (screen_size.height as i32 - stroke_margin);

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

                let d_outer = sdf::sdf_rounded_rect(px, py, cx, cy, half_w, half_h, outer_cr);
                let d_inner =
                    sdf::sdf_rounded_rect(px, py, cx, cy, inner_half_w, inner_half_h, inner_cr);

                // outer_cov: coverage of full shape (inside outer boundary)
                let outer_cov = sdf::sdf_coverage(d_outer);
                // inner_cov: coverage of interior (inside inner boundary)
                let inner_cov = sdf::sdf_coverage(d_inner);
                // stroke_cov: the border band = shape minus interior
                let stroke_cov = outer_cov - inner_cov;
                let stroke_cov = if stroke_cov > 0.0 { stroke_cov } else { 0.0 };

                let level = sdf::coverage_to_gray4(stroke_cov);
                if level > 0 {
                    pixels.push(CompressedPointWithCoverage::new(Point::new(x, y), level));
                }
            }
        }

        // Sort the left-half pixels.
        // The bucket margin includes the AA fringe (+2px) so that low-coverage
        // fringe pixels at the inner border edge are grouped with their adjacent
        // full-coverage border pixels, preventing visible "line" artifacts.
        let bucket_margin = self.border_width as i32 + 2;
        pixels.sort_unstable_by_key(|cp| {
            let point = cp.to_point();
            let mut y_bucket = point.y;

            if y_bucket < bucket_margin {
                y_bucket = 0;
            } else if y_bucket > (screen_size.height as i32 - bucket_margin - 1) {
                y_bucket = i32::MAX;
            }

            // For left-half pixels, closer to middle should come first
            let x_distance = middle_x - point.x;
            let final_distance = if point.y > screen_size.height as i32 / 2 {
                -x_distance
            } else {
                x_distance
            };

            (y_bucket, final_distance)
        });

        pixels.shrink_to_fit();

        // Store the sorted left-half pixels
        self.border_pixels = pixels;
        self.constraints = Some(screen_size); // Store screen size for mirroring
    }
}

impl<W, C> crate::DynWidget for HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);

        // Give child less space to account for the border
        let child_max_size = Size::new(
            max_size.width.saturating_sub(2 * self.border_width),
            max_size.height.saturating_sub(2 * self.border_width),
        );
        self.child.set_constraints(child_max_size);

        // Update our cached sizing
        let child_sizing = self.child.sizing();
        self.sizing = crate::Sizing {
            width: child_sizing.width + 2 * self.border_width,
            height: child_sizing.height + 2 * self.border_width,
            ..Default::default()
        };

        self.record_border_pixels(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<super::KeyTouch> {
        // Always forward to child
        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        // Always forward to child
        self.child.handle_vertical_drag(prev_y, new_y, _is_release);
    }

    fn force_full_redraw(&mut self) {
        self.last_drawn_progress = Frac::ZERO;
        // Also propagate to child
        self.child.force_full_redraw();
    }
}

// Only implement Widget for Rgb565 since we need fading support
impl<W> Widget for HoldToConfirmBorder<W, Rgb565>
where
    W: Widget<Color = Rgb565>,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let offset = Point::new(self.border_width as i32, self.border_width as i32);
        let child_size = self.child.sizing();
        let child_area = Rectangle::new(offset, Size::new(child_size.width, child_size.height));
        let mut cropped = target.clone().crop(child_area);
        self.child.draw(&mut cropped, current_time)?;

        // Don't draw anything if fully faded out
        if self.is_faded_out() {
            return Ok(());
        }

        // Build AA LUT for border_color over background_color
        let aa_lut = sdf::build_aa_lut(self.border_color, self.background_color);

        // Create fading target if we're fading
        if self.is_fading {
            let start_time = self.fade_start_time.get_or_insert(current_time);
            let elapsed = current_time.saturating_duration_since(*start_time);
            self.fade_progress = Frac::from_ratio(elapsed as u32, self.fade_duration_ms as u32);

            let mut fading_target = FadingDrawTarget {
                target,
                fade_progress: self.fade_progress,
                target_color: self.background_color,
            };

            // Draw with mirroring for fading, using AA LUT for coverage
            let screen_width = self.constraints.unwrap().width as i32;

            // Use flat_map to draw both original and mirrored pixels
            let pixels_iter = self
                .border_pixels
                .iter()
                .flat_map(move |&pixel| mirror_pixel_aa(pixel, screen_width, &aa_lut));

            fading_target.draw_iter(pixels_iter)
        } else {
            // We're drawing double the pixels (each stored pixel + its mirror)
            let total_pixels = self.border_pixels.len();
            let mut current_progress_pixels =
                (self.progress * total_pixels as u32).floor() as usize;
            let mut last_progress_pixels =
                (self.last_drawn_progress * total_pixels as u32).floor() as usize;

            let erasing = current_progress_pixels < last_progress_pixels;
            if erasing {
                core::mem::swap(&mut current_progress_pixels, &mut last_progress_pixels);
            }

            let screen_width = self.constraints.unwrap().width as i32;

            if erasing {
                // When erasing, draw background color for all coverage levels
                let bg_color = self.background_color;
                let pixels_iter = self.border_pixels[last_progress_pixels..current_progress_pixels]
                    .iter()
                    .flat_map(move |&pixel| mirror_pixel_flat(pixel, screen_width, bg_color));
                target.draw_iter(pixels_iter)?;
            } else {
                // When drawing, use AA LUT for smooth edges
                let pixels_iter = self.border_pixels[last_progress_pixels..current_progress_pixels]
                    .iter()
                    .flat_map(move |&pixel| mirror_pixel_aa(pixel, screen_width, &aa_lut));
                target.draw_iter(pixels_iter)?;
            }

            self.last_drawn_progress = self.progress;
            Ok(())
        }
    }
}
