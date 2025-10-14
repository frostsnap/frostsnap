use super::{Frac, Widget};
use crate::animation_speed::AnimationSpeed;
use crate::super_draw_target::SuperDrawTarget;
use crate::widget_color::ColorInterpolate;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::Rgb565,
    primitives::Rectangle,
    Pixel,
};

/// The current state of the fader
#[derive(Debug, PartialEq)]
enum FadeState {
    /// Not fading, widget draws normally
    Idle,
    /// Currently fading out
    FadingOut {
        start_time: Option<crate::Instant>,
        duration_ms: u64,
    },
    /// Currently fading in
    FadingIn {
        start_time: Option<crate::Instant>,
        duration_ms: u64,
    },
    /// Faded out completely (for new_faded_out)
    FadedOut,
}

/// A widget that can fade its child to a target color
#[derive(Debug, PartialEq)]
pub struct Fader<W> {
    pub child: W,
    state: FadeState,
    animation_speed: AnimationSpeed,
    constraints: Option<Size>,
}

impl<W: Widget<Color = Rgb565>> Fader<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            state: FadeState::Idle,
            animation_speed: AnimationSpeed::Linear,
            constraints: None,
        }
    }

    /// Create a new Fader that doesn't draw anything until start_fade_in is called
    pub fn new_faded_out(child: W) -> Self {
        Self {
            child,
            state: FadeState::FadedOut,
            animation_speed: AnimationSpeed::Linear,
            constraints: None,
        }
    }

    /// Set the animation speed curve
    pub fn set_animation_speed(&mut self, speed: AnimationSpeed) {
        self.animation_speed = speed;
    }

    /// Start fading out over the specified duration
    /// This function is monotonic - it can only make fades happen faster, never slower
    pub fn start_fade(&mut self, duration_ms: u64) {
        // If already faded out, do nothing
        if matches!(self.state, FadeState::FadedOut) {
            return;
        }

        // Check if we're already fading out with a start time
        if let FadeState::FadingOut {
            start_time: Some(_),
            duration_ms: current_duration,
        } = &mut self.state
        {
            if duration_ms < *current_duration {
                // Update to shorter duration, keeping the same start time
                *current_duration = duration_ms;
            }
            // Either updated to shorter duration or keeping existing (if new would be longer)
            return;
        }

        // Starting a new fade out (or switching from fade in/idle to fade out)
        self.state = FadeState::FadingOut {
            start_time: None, // Will be set on first draw
            duration_ms,
        };
    }

    /// Start fading in over the specified duration
    /// This function is monotonic - it can only make fades happen faster, never slower
    pub fn start_fade_in(&mut self, duration_ms: u64) {
        // If already fully visible (Idle state), do nothing
        if matches!(self.state, FadeState::Idle) {
            return;
        }

        // Check if we're already fading in with a start time
        if let FadeState::FadingIn {
            start_time: Some(_),
            duration_ms: current_duration,
        } = &mut self.state
        {
            if duration_ms < *current_duration {
                // Update to shorter duration, keeping the same start time
                *current_duration = duration_ms;
            }
            // Either updated to shorter duration or keeping existing (if new would be longer)
            return;
        }

        // Starting a new fade in (or switching from fade out/idle to fade in)
        self.state = FadeState::FadingIn {
            start_time: None, // Will be set on first draw
            duration_ms,
        };
    }

    /// Stop fading
    pub fn stop_fade(&mut self) {
        self.state = FadeState::Idle;
    }

    pub fn instant_fade(&mut self) {
        self.start_fade(0);
    }

    /// Check if fading is complete
    pub fn is_fade_complete(&self) -> bool {
        matches!(&self.state, FadeState::Idle | FadeState::FadedOut)
    }

    /// Check if the widget is currently faded out
    pub fn is_faded_out(&self) -> bool {
        matches!(self.state, FadeState::FadedOut)
    }

    /// Set the fader to faded out state
    pub fn set_faded_out(&mut self) {
        self.state = FadeState::FadedOut;
    }

    /// Check if the widget is showing (not faded out and not fading out)
    pub fn is_not_faded(&self) -> bool {
        matches!(self.state, FadeState::Idle)
    }

    pub fn is_visible(&self) -> bool {
        self.is_not_faded() || self.is_fading_in()
    }

    pub fn is_fading(&self) -> bool {
        matches!(
            self.state,
            FadeState::FadingIn { .. } | FadeState::FadingOut { .. }
        )
    }

    pub fn is_fading_in(&self) -> bool {
        matches!(self.state, FadeState::FadingIn { .. })
    }
}

/// A custom DrawTarget that intercepts pixel drawing and applies fade
pub struct FadingDrawTarget<'a, D> {
    pub target: &'a mut D,
    pub fade_progress: Frac,
    pub target_color: Rgb565,
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

        let faded_pixels = pixels.into_iter().map(|Pixel(point, color)| {
            let faded_color = match cache {
                Some((cached_source, cached_result)) if cached_source == color => {
                    // Cache hit - same source color
                    cached_result
                }
                _ => {
                    // Cache miss or first calculation
                    let calculated = color.interpolate(self.target_color, self.fade_progress);
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

impl<W: Widget<Color = Rgb565>> crate::DynWidget for Fader<W> {
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
        self.child.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.child.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        if !self.is_not_faded() {
            return None;
        }

        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        if !self.is_not_faded() {
            return;
        }

        self.child.handle_vertical_drag(prev_y, new_y, _is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}

impl<W: Widget<Color = Rgb565>> Widget for Fader<W> {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        match &mut self.state {
            FadeState::FadedOut => Ok(()),
            FadeState::Idle => self.child.draw(target, current_time),
            state => {
                // Extract common fields based on state
                let (start_time, duration_ms, is_fade_in) = match state {
                    FadeState::FadingOut {
                        start_time,
                        duration_ms,
                    } => (start_time, *duration_ms, false),
                    FadeState::FadingIn {
                        start_time,
                        duration_ms,
                    } => (start_time, *duration_ms, true),
                    _ => unreachable!(),
                };

                // Set start time on first draw
                if start_time.is_none() {
                    *start_time = Some(current_time);
                }
                let actual_start_time = start_time.unwrap();

                // Calculate fade progress using Frac (automatically clamped to [0, 1])
                let elapsed = current_time.saturating_duration_since(actual_start_time) as u32;
                let linear_progress = Frac::from_ratio(elapsed, duration_ms as u32);
                let eased_progress = self.animation_speed.apply(linear_progress);

                // For fade-in, reverse the progress (1.0 -> 0.0)
                let fade_progress = if is_fade_in {
                    Frac::ONE - eased_progress
                } else {
                    eased_progress
                };

                self.child.force_full_redraw();

                // Use SuperDrawTarget's opacity method for fading
                let mut fading_target = target.clone().opacity(Frac::ONE - fade_progress);
                self.child.draw(&mut fading_target, current_time)?;

                // Check if fade is complete
                let is_complete = if is_fade_in {
                    fade_progress == Frac::ZERO
                } else {
                    fade_progress == Frac::ONE
                };

                if is_complete {
                    self.state = if is_fade_in {
                        FadeState::Idle
                    } else {
                        FadeState::FadedOut
                    };
                }

                Ok(())
            }
        }
    }
}
