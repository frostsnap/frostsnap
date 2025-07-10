use super::Widget;
use crate::{Instant, palette::PALETTE};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
    Pixel,
};

// Custom DrawTarget that records pixel positions
struct PixelRecorder {
    pixels: Vec<Point>,
}

impl PixelRecorder {
    fn new() -> Self {
        Self {
            pixels: Vec::new(),
        }
    }
}

impl DrawTarget for PixelRecorder {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            self.pixels.push(pixel.0);
        }
        Ok(())
    }
}

impl OriginDimensions for PixelRecorder {
    fn size(&self) -> Size {
        // Return a large size to avoid clipping
        Size::new(1000, 1000)
    }
}

#[derive(Debug)]
pub struct HoldToConfirmWidget<W> {
    child: W,
    enabled: bool,
    progress: f32, // 0.0 to 1.0 (percentage of border completed)
    completed: bool,
    holding: bool,
    last_update: Option<Instant>,
    last_drawn_progress: f32,
    screen_size: Size,
    border_pixels: Vec<Point>, // Recorded border pixels
    border_drawn: bool,        // Track if static border has been drawn
    hold_duration_ms: f32,     // Milliseconds required to confirm
}

impl<W: Widget> HoldToConfirmWidget<W> {
    pub fn new(child: W, hold_duration_ms: f32) -> Self {
        // Get size from child widget - panic if it doesn't provide one
        let screen_size = child.size_hint()
            .expect("HoldToConfirmWidget requires a child widget that provides size_hint()");
        
        Self {
            child,
            enabled: false,
            progress: 0.0,
            completed: false,
            holding: false,
            last_update: None,
            last_drawn_progress: -1.0,
            screen_size,
            border_pixels: Vec::new(),
            border_drawn: false,
            hold_duration_ms,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
        self.completed = false;
        self.progress = 0.0;
        self.holding = false;
        self.last_update = None;
        self.last_drawn_progress = -1.0;
        self.border_drawn = false;

        // Record border pixels if not already done
        if self.border_pixels.is_empty() {
            self.record_border_pixels();
        }
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.completed = false;
        self.progress = 0.0;
        self.holding = false;
        self.last_update = None;
        self.last_drawn_progress = -1.0;
        self.border_drawn = false;
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
    
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn reset(&mut self) {
        self.completed = false;
        self.progress = 0.0;
        self.holding = false;
        self.last_update = None;
        self.last_drawn_progress = -1.0;
    }

    fn record_border_pixels(&mut self) {
        const CORNER_RADIUS: u32 = 42;
        const BORDER_WIDTH: u32 = 6;

        let mut recorder = PixelRecorder::new();

        // Draw the rounded rectangle to the recorder
        use embedded_graphics::primitives::CornerRadii;
        let _ = RoundedRectangle::new(
            Rectangle::new(Point::new(0, 0), self.screen_size),
            CornerRadii::new(Size::new(CORNER_RADIUS, CORNER_RADIUS)),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, BORDER_WIDTH))
        .draw(&mut recorder);

        // Get the pixels and sort them
        let mut pixels = recorder.pixels;

        // Sort by (y/3, distance from middle x)
        let middle_x = self.screen_size.width as i32 / 2;
        pixels.sort_by_key(|point| {
            let y_bucket = point.y / 4;
            let mut x_distance = (point.x - middle_x).abs() as i32;
            if point.y > self.screen_size.height as i32 / 2 {
                x_distance = -x_distance;
            }
            (y_bucket, x_distance)
        });

        self.border_pixels = pixels;
    }

    fn update_progress(&mut self, current_time: Instant) {
        // Only process if we're actively holding or decaying
        if !self.holding && self.progress == 0.0 {
            return;
        }

        if let Some(last_time) = self.last_update {
            if let Some(duration) = current_time.checked_duration_since(last_time) {
                let elapsed_ms = duration.to_millis() as f32;

                // Skip if no time has passed
                if elapsed_ms == 0.0 {
                    return;
                }

                if self.holding && !self.completed {
                    // Build up progress: complete in hold_duration_ms
                    let increment = elapsed_ms / self.hold_duration_ms;
                    self.progress = (self.progress + increment).min(1.0);

                    if self.progress >= 1.0 {
                        self.completed = true;
                        self.progress = 1.0;
                    }
                } else if !self.holding && self.progress > 0.0 && !self.completed {
                    // Reduce progress: decay in 1000ms
                    let decrement = elapsed_ms / 1000.0;
                    self.progress = (self.progress - decrement).max(0.0);

                    // Clear last_update when we reach 0 to stop updates
                    if self.progress == 0.0 {
                        self.last_update = None;
                        return;
                    }
                }

                // Update the timer for next frame
                self.last_update = Some(current_time);
            }
        }
    }

    fn draw_border<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        const SNAKE_LENGTH: usize = 60; // Length of the snake in pixels

        if !self.enabled {
            return Ok(());
        }

        // Draw static border only once
        if !self.border_drawn {
            target.draw_iter(
                self.border_pixels
                    .iter()
                    .map(|&point| Pixel(point, PALETTE.surface_variant))
            )?;
            self.border_drawn = true;
        }

        // Handle progress changes
        if !self.border_pixels.is_empty() {
            let total_pixels = self.border_pixels.len();
            let current_progress_pixels = (self.progress * total_pixels as f32) as usize;
            let last_progress_pixels = (self.last_drawn_progress * total_pixels as f32) as usize;

            // If progress decreased, restore the border color
            if current_progress_pixels < last_progress_pixels {
                // Restore border color for pixels from current_progress_pixels to last_progress_pixels
                target.draw_iter(
                    self.border_pixels[current_progress_pixels..last_progress_pixels]
                        .iter()
                        .map(|&point| Pixel(point, PALETTE.surface_variant))
                )?;
            }

            // Draw progress snake if we have progress
            if self.progress > 0.0 {
                // Calculate snake start and end positions
                let snake_end = current_progress_pixels.min(total_pixels);
                let snake_start = snake_end.saturating_sub(SNAKE_LENGTH);

                // Draw the snake using a slice
                target.draw_iter(
                    self.border_pixels[snake_start..snake_end]
                        .iter()
                        .map(|&point| Pixel(point, PALETTE.primary))
                )?;
            }
        }

        self.last_drawn_progress = self.progress;
        Ok(())
    }
}

impl<W: Widget> Widget for HoldToConfirmWidget<W> {
    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Always draw the child widget first
        self.child.draw(target, current_time)?;

        // Only update and draw border if enabled
        if self.enabled {
            // Only update progress if we have started (holding or have progress)
            if self.holding || self.progress > 0.0 {
                self.update_progress(current_time);
            }

            // Always draw border when enabled
            self.draw_border(target)?;
        }

        Ok(())
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<super::KeyTouch> {
        if !self.enabled {
            // Forward to child if disabled
            return self.child.handle_touch(point, current_time, lift_up);
        }

        // When enabled, intercept all touch events
        if lift_up {
            self.holding = false;
            // Don't clear last_update here - we need it for decay animation
        } else {
            // If already completed, do nothing
            if self.completed {
                return None;
            }
            
            if !self.holding {
                // Starting a new hold - initialize timer
                self.holding = true;
                self.last_update = Some(current_time);
            }
        }

        // Don't forward touch events when enabled
        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        if !self.enabled {
            // Forward to child if disabled
            self.child.handle_vertical_drag(prev_y, new_y);
        }
        // When enabled, don't forward drag events
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.screen_size)
    }
}
