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

    pub fn is_fading(&self) -> bool {
        self.is_fading
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

    fn load_border_pixels_if_needed(&mut self) {
        if self.border_pixels.is_empty() {
            self.border_pixels = crate::hold_to_confirm_border_data::load_border_pixels();
        }
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

        self.load_border_pixels_if_needed();
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
