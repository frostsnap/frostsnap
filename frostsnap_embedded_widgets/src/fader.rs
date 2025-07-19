use super::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    Pixel,
};

/// The current state of the fader
#[derive(Debug)]
enum FadeState {
    /// Not fading, widget draws normally
    Idle,
    /// Currently fading out to a color
    FadingOut {
        start_time: crate::Instant,
        duration_ms: u64,
        target_color: Rgb565,
    },
    /// Currently fading in from a color
    FadingIn {
        start_time: crate::Instant,
        duration_ms: u64,
        from_color: Rgb565,
    },
    /// Faded out completely (for new_faded_out)
    FadedOut,
}

/// A widget that can fade its child to a target color
#[derive(Debug)]
pub struct Fader<W> {
    child: W,
    state: FadeState,
    redraw_interval_ms: u64,
    last_redraw_time: Option<crate::Instant>,
}

impl<W: Widget<Color = Rgb565>> Fader<W> {
    pub fn new(child: W, _target_color: Rgb565) -> Self {
        Self {
            child,
            state: FadeState::Idle,
            redraw_interval_ms: 0,
            last_redraw_time: None,
        }
    }
    
    /// Create a new Fader that doesn't draw anything until start_fade_in is called
    pub fn new_faded_out(child: W) -> Self {
        Self {
            child,
            state: FadeState::FadedOut,
            redraw_interval_ms: 0,
            last_redraw_time: None,
        }
    }

    /// Start fading to the target color over the specified duration
    pub fn start_fade(&mut self, duration_ms: u64, redraw_interval_ms: u64, target_color: Rgb565) {
        // We'll set the actual start time on first draw to avoid timing issues
        self.state = FadeState::FadingOut {
            start_time: crate::Instant::from_millis(0), // Placeholder, will be set on draw
            duration_ms,
            target_color,
        };
        self.redraw_interval_ms = redraw_interval_ms;
        self.last_redraw_time = None;
    }
    
    /// Start fading in from the specified color
    pub fn start_fade_in(&mut self, duration_ms: u64, redraw_interval_ms: u64, fade_from_color: Rgb565) {
        self.state = FadeState::FadingIn {
            start_time: crate::Instant::from_millis(0), // Placeholder, will be set on draw
            duration_ms,
            from_color: fade_from_color,
        };
        self.redraw_interval_ms = redraw_interval_ms;
        self.last_redraw_time = None;
    }

    /// Stop fading
    pub fn stop_fade(&mut self) {
        self.state = FadeState::Idle;
    }

    /// Check if fading is complete
    pub fn is_fade_complete(&self) -> bool {
        match &self.state {
            FadeState::Idle | FadeState::FadedOut => true,
            _ => false,
        }
    }
    
    /// Check if the widget is currently faded out
    pub fn is_faded_out(&self) -> bool {
        matches!(self.state, FadeState::FadedOut)
    }

    /// Get mutable reference to the inner widget
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.child
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
                    let calculated =
                        interpolate_color(color, self.target_color, self.fade_progress);
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
        match &mut self.state {
            FadeState::FadedOut => {
                // Don't draw anything when faded out
                Ok(())
            }
            FadeState::Idle => {
                // Normal draw without fading
                self.child.draw(target, current_time)
            }
            state => {
                // Extract common fields based on state
                let (start_time, duration_ms, target_color, is_fade_in) = match state {
                    FadeState::FadingOut { start_time, duration_ms, target_color } => 
                        (start_time, *duration_ms, *target_color, false),
                    FadeState::FadingIn { start_time, duration_ms, from_color } => 
                        (start_time, *duration_ms, *from_color, true),
                    _ => unreachable!(),
                };
                
                // Update start time if this is the first draw
                if start_time.as_millis() == 0 {
                    *start_time = current_time;
                }

                // Check if we should redraw based on interval
                let should_redraw = if let Some(last_redraw) = self.last_redraw_time {
                    current_time.saturating_duration_since(last_redraw) >= self.redraw_interval_ms
                } else {
                    true // First redraw
                };

                if should_redraw {
                    // Calculate fade progress
                    let elapsed = current_time.saturating_duration_since(*start_time) as f32;
                    let mut fade_progress = (elapsed / duration_ms as f32).min(1.0);
                    
                    // For fade-in, reverse the progress (1.0 -> 0.0)
                    if is_fade_in {
                        fade_progress = 1.0 - fade_progress;
                    }

                    // Force redraw before drawing
                    self.child.force_full_redraw();

                    // Create fading draw target
                    let mut fading_target = FadingDrawTarget {
                        target,
                        fade_progress,
                        target_color,
                    };

                    // Draw child through fading target
                    self.child.draw(&mut fading_target, current_time)?;

                    // Update last redraw time
                    self.last_redraw_time = Some(current_time);

                    // Check if fade is complete
                    let is_complete = if is_fade_in {
                        fade_progress <= 0.0
                    } else {
                        fade_progress >= 1.0
                    };
                    
                    if is_complete {
                        self.state = if is_fade_in {
                            FadeState::Idle
                        } else {
                            FadeState::FadedOut
                        };
                    }
                }

                Ok(())
            }
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        self.child.handle_vertical_drag(prev_y, new_y);
    }

    fn size_hint(&self) -> Option<Size> {
        self.child.size_hint()
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}
