use super::{compressed_point::CompressedPoint, pixel_recorder::PixelRecorder, rat::Frac, Widget};
use crate::fader::FadingDrawTarget;
use crate::super_draw_target::SuperDrawTarget;
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{BinaryColor, PixelColor, Rgb565},
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
};

/// Generate both the original and mirrored pixel for a given compressed point
fn mirror_pixel<C: PixelColor>(
    pixel: CompressedPoint,
    screen_width: i32,
    color: C,
) -> [Pixel<C>; 2] {
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
    border_pixels: Vec<CompressedPoint>, // Recorded border pixels (compressed)
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

    fn record_border_pixels(&mut self, screen_size: Size) {
        const CORNER_RADIUS: u32 = 42;

        // Create a recorder clipped to only the left half to save memory.
        // We're going to draw the other half by mirroring around the y-axis.
        let mut recorder = PixelRecorder::new();
        let middle_x = screen_size.width as i32 / 2;
        let left_half_area = Rectangle::new(
            Point::new(0, 0),
            Size::new((middle_x + 1) as u32, screen_size.height),
        );
        let mut clipped_recorder = recorder.clipped(&left_half_area);

        // Draw the rounded rectangle directly to the clipped recorder
        // This will only record pixels in the left half
        use embedded_graphics::primitives::{CornerRadii, StrokeAlignment};
        let _ = RoundedRectangle::new(
            Rectangle::new(Point::new(0, 0), screen_size),
            CornerRadii::new(Size::new(CORNER_RADIUS, CORNER_RADIUS)),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(BinaryColor::Off)
                .stroke_width(self.border_width)
                .stroke_alignment(StrokeAlignment::Inside)
                .build(),
        )
        .draw(&mut clipped_recorder);

        // Sort the left-half pixels
        let mut pixels = recorder.pixels;
        pixels.sort_unstable_by_key(|cp| {
            let point = cp.to_point();
            let mut y_bucket = point.y;

            if y_bucket < self.border_width as i32 {
                y_bucket = 0;
            } else if y_bucket > (screen_size.height as i32 - self.border_width as i32 - 1) {
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

            // Draw with mirroring for fading
            let screen_width = self.constraints.unwrap().width as i32;
            let border_color = self.border_color;

            // Use flat_map to draw both original and mirrored pixels
            let pixels_iter = self
                .border_pixels
                .iter()
                .flat_map(move |&pixel| mirror_pixel(pixel, screen_width, border_color));

            fading_target.draw_iter(pixels_iter)
        } else {
            // We're drawing double the pixels (each stored pixel + its mirror)
            let total_pixels = self.border_pixels.len();
            let mut current_progress_pixels =
                (self.progress * total_pixels as u32).floor() as usize;
            let mut last_progress_pixels =
                (self.last_drawn_progress * total_pixels as u32).floor() as usize;

            let color = if current_progress_pixels > last_progress_pixels {
                self.border_color
            } else {
                core::mem::swap(&mut current_progress_pixels, &mut last_progress_pixels);
                self.background_color
            };

            let screen_width = self.constraints.unwrap().width as i32;
            let pixels_iter = self.border_pixels[last_progress_pixels..current_progress_pixels]
                .iter()
                .flat_map(move |&pixel| mirror_pixel(pixel, screen_width, color));

            target.draw_iter(pixels_iter)?;

            self.last_drawn_progress = self.progress;
            Ok(())
        }
    }
}
