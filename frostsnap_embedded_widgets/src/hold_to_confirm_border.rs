use super::{compressed_point::CompressedPoint, pixel_recorder::PixelRecorder, rat::Frac, Widget};
use crate::fader::FadingDrawTarget;
use crate::prelude::*;
use crate::super_draw_target::SuperDrawTarget;
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{BinaryColor, PixelColor, Rgb565},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
};

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
    border_pixels_recorded: bool,
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
            border_pixels_recorded: false,
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

        let mut recorder = PixelRecorder::new();

        // Draw the rounded rectangle to the recorder
        use embedded_graphics::primitives::CornerRadii;
        let _ = RoundedRectangle::new(
            Rectangle::new(Point::new(0, 0), screen_size),
            CornerRadii::new(Size::new(CORNER_RADIUS, CORNER_RADIUS)),
        )
        .into_styled(PrimitiveStyle::with_stroke(
            BinaryColor::Off,
            self.border_width,
        ))
        .draw(&mut recorder);

        // Get the pixels and shrink capacity BEFORE sorting
        recorder.pixels.shrink_to_fit();
        let mut pixels = recorder.pixels;

        // Sort by (y/3, distance from middle x)
        let middle_x = screen_size.width as i32 / 2;
        pixels.sort_by_key(|cp| {
            let point = cp.to_point();
            let mut y_bucket = point.y;

            if y_bucket < self.border_width as i32 {
                y_bucket = 0;
            } else if y_bucket > (screen_size.height - self.border_width) as i32 {
                y_bucket = i32::MAX;
            }
            let mut x_distance = (point.x - middle_x).abs();
            if point.y > screen_size.height as i32 / 2 {
                x_distance = -x_distance;
            }

            (y_bucket, x_distance)
        });

        // Pixels are already compressed
        self.border_pixels = pixels;
        self.border_pixels_recorded = true;
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

        // Record border pixels now that we know the size
        if !self.border_pixels_recorded {
            // Use the full size for border recording (includes the border)
            self.record_border_pixels(max_size);
        }
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

            fading_target.draw_iter(
                self.border_pixels[..]
                    .iter()
                    .map(|&pixel| Pixel(pixel.to_point(), self.border_color)),
            )
        } else {
            let total_pixels = self.border_pixels.len();
            let current_progress_pixels = (self.progress * total_pixels as u32).floor() as usize;
            let last_progress_pixels =
                (self.last_drawn_progress * total_pixels as u32).floor() as usize;

            if current_progress_pixels > last_progress_pixels {
                target.draw_iter(
                    self.border_pixels[last_progress_pixels..current_progress_pixels]
                        .iter()
                        .map(|&pixel| Pixel(pixel.to_point(), self.border_color)),
                )?;
            } else if current_progress_pixels < last_progress_pixels {
                target.draw_iter(
                    self.border_pixels[current_progress_pixels..last_progress_pixels]
                        .iter()
                        .map(|&pixel| Pixel(pixel.to_point(), self.background_color)),
                )?;
            }

            self.last_drawn_progress = self.progress;
            Ok(())
        }
    }
}
