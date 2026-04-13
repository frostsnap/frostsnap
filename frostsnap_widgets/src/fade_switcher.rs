use crate::animation_speed::AnimationSpeed;
use crate::super_draw_target::SuperDrawTarget;
use crate::{DynWidget, Fader, Instant, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

#[derive(Clone, Copy)]
pub struct FadeConfig {
    pub duration_ms: u32,
    pub speed: AnimationSpeed,
}

impl FadeConfig {
    pub const DEFAULT: Self = Self {
        duration_ms: 750,
        speed: AnimationSpeed::Linear,
    };

    pub const fn new(duration_ms: u32) -> Self {
        Self {
            duration_ms,
            speed: AnimationSpeed::Linear,
        }
    }

    pub fn with_speed(mut self, speed: AnimationSpeed) -> Self {
        self.speed = speed;
        self
    }
}

impl Default for FadeConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A widget that smoothly fades between widgets of the same type
pub struct FadeSwitcher<T>
where
    T: Widget<Color = Rgb565>,
{
    current: Fader<T>,
    prev: Option<Fader<T>>,
    fade_in: FadeConfig,
    fade_out: FadeConfig,
    constraints: Option<Size>,
    pub shrink_to_fit: bool,
}

impl<T> FadeSwitcher<T>
where
    T: Widget<Color = Rgb565>,
{
    pub fn new(initial: T) -> Self {
        let child = Fader::new_faded_out(initial);
        Self {
            current: child,
            prev: None,
            fade_in: FadeConfig::DEFAULT,
            fade_out: FadeConfig::DEFAULT,
            constraints: None,
            shrink_to_fit: false,
        }
    }

    /// Configure the FadeSwitcher to shrink to fit the first child
    pub fn with_shrink_to_fit(mut self) -> Self {
        self.shrink_to_fit = true;
        self
    }

    pub fn with_fade_config(mut self, config: FadeConfig) -> Self {
        self.fade_in = config;
        self.fade_out = config;
        self
    }

    pub fn with_fade_in(mut self, config: FadeConfig) -> Self {
        self.fade_in = config;
        self
    }

    pub fn with_fade_out(mut self, config: FadeConfig) -> Self {
        self.fade_out = config;
        self
    }

    /// Switch to a new widget with a fade transition
    pub fn switch_to(&mut self, widget: T) {
        let mut new_fader = Fader::new_faded_out(widget);
        // Set constraints on the new fader
        if let Some(constraints) = self.constraints {
            new_fader.set_constraints(constraints);
        }

        let mut prev_fader = core::mem::replace(&mut self.current, new_fader);
        if self.prev.is_none() {
            prev_fader.start_fade_with(self.fade_out);
            self.prev = Some(prev_fader);
        }
    }

    pub fn instant_switch_to(&mut self, widget: T) {
        self.switch_to(widget);
        if let Some(prev) = &mut self.prev {
            prev.instant_fade();
        }
    }

    pub fn instant_fade(&mut self) {
        self.current.instant_fade();
    }

    /// Check if the fade transition is fully complete (no fade in progress)
    pub fn is_idle(&self) -> bool {
        self.prev.is_none() && self.current.is_not_faded()
    }

    /// Get a reference to the current widget
    pub fn current(&self) -> &T {
        &self.current.child
    }

    /// Get a mutable reference to the current widget
    pub fn current_mut(&mut self) -> &mut T {
        &mut self.current.child
    }
}

impl<T> crate::DynWidget for FadeSwitcher<T>
where
    T: Widget<Color = Rgb565>,
{
    fn set_constraints(&mut self, max_size: Size) {
        let constraints = if self.shrink_to_fit {
            self.current.set_constraints(max_size);

            self.current.sizing().into()
        } else {
            self.current.set_constraints(max_size);
            max_size
        };

        self.constraints = Some(constraints);
        if let Some(ref mut prev) = self.prev {
            prev.set_constraints(constraints);
        }
    }

    fn sizing(&self) -> crate::Sizing {
        let size = self.constraints.unwrap();
        crate::Sizing {
            width: size.width,
            height: size.height,
            dirty_rect: None,
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Only handle touch for the current widget
        self.current.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Only handle drag for the current widget
        self.current.handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.current.force_full_redraw();
        if let Some(prev) = &mut self.prev {
            prev.force_full_redraw();
        }
    }
}

impl<T> Widget for FadeSwitcher<T>
where
    T: Widget<Color = Rgb565>,
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
        // Draw the previous widget if it's still fading out
        if let Some(prev) = &mut self.prev {
            prev.draw(target, current_time)?;

            // Remove it once fully faded
            if prev.is_faded_out() {
                self.prev = None;
            }
        }

        if self.prev.is_some() {
            return Ok(());
        }

        if self.current.is_faded_out() {
            self.current.start_fade_in_with(self.fade_in);
        }

        self.current.draw(target, current_time)?;

        Ok(())
    }
}
