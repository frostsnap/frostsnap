use super::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    Pixel,
};

/// A widget that can fade its child to a target color
#[derive(Debug)]
pub struct Fader<W> {
    child: Option<W>,
    fade_start_time: Option<crate::Instant>,
    fade_duration_ms: u64,
    redraw_interval_ms: u64,
    last_redraw_time: Option<crate::Instant>,
    target_color: Rgb565,
    fading: bool,
}

impl<W: Widget<Color = Rgb565>> Fader<W> {
    pub fn new(child: W, target_color: Rgb565) -> Self {
        Self {
            child: Some(child),
            fade_start_time: None,
            fade_duration_ms: 0,
            redraw_interval_ms: 0,
            last_redraw_time: None,
            target_color,
            fading: false,
        }
    }

    /// Start fading to the target color over the specified duration
    pub fn start_fade(&mut self, duration_ms: u64, redraw_interval_ms: u64, target_color: Rgb565) {
        self.fading = true;
        self.fade_duration_ms = duration_ms;
        self.redraw_interval_ms = redraw_interval_ms;
        self.target_color = target_color;
        self.fade_start_time = None; // Will be set on next draw
        self.last_redraw_time = None;
    }

    /// Stop fading and optionally clear the child widget
    pub fn stop_fade(&mut self, clear_child: bool) {
        self.fading = false;
        if clear_child {
            self.child = None;
        }
    }

    /// Check if fading is complete
    pub fn is_fade_complete(&self) -> bool {
        if !self.fading {
            return false;
        }

        self.fade_start_time.is_some()
    }

    /// Get mutable reference to the inner widget
    pub fn inner_mut(&mut self) -> Option<&mut W> {
        self.child.as_mut()
    }
}

fn interpolate_color(from: Rgb565, to: Rgb565, t: f32) -> Rgb565 {
    let t = t.clamp(0.0, 1.0);
    let from_r = (from.r() as f32) * (1.0 - t);
    let from_g = (from.g() as f32) * (1.0 - t);
    let from_b = (from.b() as f32) * (1.0 - t);

    let to_r = (to.r() as f32) * t;
    let to_g = (to.g() as f32) * t;
    let to_b = (to.b() as f32) * t;

    Rgb565::new(
        (from_r + to_r) as u8,
        (from_g + to_g) as u8,
        (from_b + to_b) as u8,
    )
}

/// A custom DrawTarget that intercepts pixel drawing and applies fade
struct FadingDrawTarget<'a, D> {
    target: &'a mut D,
    fade_progress: f32,
    target_color: Rgb565,
}

impl<'a, D: DrawTarget<Color = Rgb565>> DrawTarget for FadingDrawTarget<'a, D> {
    type Color = Rgb565;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        // Cache with invalidation based on source color
        let mut cache: Option<(Rgb565, Rgb565)> = None; // (source_color, faded_color)
        
        // Intercept each pixel and interpolate its color
        let faded_pixels = pixels.into_iter().map(|Pixel(point, color)| {
            let faded_color = match cache {
                Some((cached_source, cached_result)) if cached_source == color => {
                    // Cache hit - same source color
                    cached_result
                }
                _ => {
                    // Cache miss or first calculation
                    let calculated = interpolate_color(color, self.target_color, self.fade_progress);
                    cache = Some((color, calculated));
                    calculated
                }
            };
            Pixel(point, faded_color)
        });

        self.target.draw_iter(faded_pixels)
    }
}

impl<'a, D: DrawTarget<Color = Rgb565>> Dimensions for FadingDrawTarget<'a, D> {
    fn bounding_box(&self) -> Rectangle {
        self.target.bounding_box()
    }
}

impl<W: Widget<Color = Rgb565>> Widget for Fader<W> {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if let Some(child) = &mut self.child {
            if self.fading {
                // Set fade start time if not set
                if self.fade_start_time.is_none() {
                    self.fade_start_time = Some(current_time);
                }

                // Check if we should redraw based on interval
                let should_redraw = if let Some(last_redraw) = self.last_redraw_time {
                    current_time.saturating_duration_since(last_redraw) >= self.redraw_interval_ms
                } else {
                    true // First redraw
                };

                if should_redraw {
                    // Calculate fade progress
                    let fade_progress = if let Some(start) = self.fade_start_time {
                        let elapsed = current_time.saturating_duration_since(start) as f32;
                        let progress = (elapsed / self.fade_duration_ms as f32).min(1.0);

                        progress
                    } else {
                        0.0
                    };

                    // Force redraw before drawing
                    child.force_full_redraw();

                    // Create fading draw target
                    let mut fading_target = FadingDrawTarget {
                        target,
                        fade_progress,
                        target_color: self.target_color,
                    };

                    // Draw child through fading target
                    child.draw(&mut fading_target, current_time)?;

                    // Update last redraw time
                    self.last_redraw_time = Some(current_time);
                    
                    // Check if fade is complete after drawing
                    if fade_progress >= 1.0 {
                        self.fading = false;
                        self.child = None; // Clear child after fade completes
                    }
                }
            } else {
                // Normal draw without fading
                child.draw(target, current_time)?;
            }
        }

        Ok(())
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        if let Some(child) = &mut self.child {
            child.handle_touch(point, current_time, lift_up)
        } else {
            None
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        if let Some(child) = &mut self.child {
            child.handle_vertical_drag(prev_y, new_y);
        }
    }

    fn size_hint(&self) -> Option<Size> {
        if let Some(child) = &self.child {
            child.size_hint()
        } else {
            None
        }
    }

    fn force_full_redraw(&mut self) {
        if let Some(child) = &mut self.child {
            child.force_full_redraw();
        }
    }
}
