use crate::super_draw_target::SuperDrawTarget;
use crate::{DynWidget, Fader, Instant, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

/// A widget that smoothly fades between widgets of the same type
pub struct FadeSwitcher<T>
where
    T: Widget<Color = Rgb565>,
{
    current: Fader<T>,
    prev: Option<Fader<T>>,
    fade_duration_ms: u32,
    fade_redraw_interval_ms: u64,
    bg_color: Rgb565,
    constraints: Option<Size>,
}

impl<T> FadeSwitcher<T>
where
    T: Widget<Color = Rgb565>,
{
    /// Create a new FadeSwitcher with an initial widget
    pub fn new(
        initial: T,
        fade_duration_ms: u32,
        fade_redraw_interval_ms: u64,
        bg_color: Rgb565,
    ) -> Self {
        let mut child = Fader::new_faded_out(initial);
        child.start_fade_in(fade_duration_ms as _, fade_redraw_interval_ms, bg_color);
        Self {
            current: child,
            prev: None,
            fade_duration_ms,
            fade_redraw_interval_ms,
            bg_color,
            constraints: None,
        }
    }

    /// Switch to a new widget with a fade transition
    pub fn switch_to(&mut self, mut widget: T) {
        // Apply constraints to the new widget if we have them
        if let Some(constraints) = self.constraints {
            widget.set_constraints(constraints);
        }

        let mut new_fader = Fader::new_faded_out(widget);
        // Set constraints on the new fader
        if let Some(constraints) = self.constraints {
            new_fader.set_constraints(constraints);
        }

        let mut prev_fader = core::mem::replace(&mut self.current, new_fader);
        if self.prev.is_none() {
            // we only care about fading out the old widget if it was ever drawn. An existing `self.prev` means it wasn't.
            prev_fader.start_fade(
                self.fade_duration_ms as u64,
                self.fade_redraw_interval_ms,
                self.bg_color,
            );
            self.prev = Some(prev_fader);
        }
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
        // Store the constraints
        self.constraints = Some(max_size);
        // Apply to current and previous widgets
        self.current.set_constraints(max_size);
        if let Some(ref mut prev) = self.prev {
            prev.set_constraints(max_size);
        }
    }

    fn sizing(&self) -> crate::Sizing {
        self.current.sizing()
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
            if prev.is_faded_out() && self.current.is_faded_out() {
                self.current.start_fade_in(
                    self.fade_duration_ms as u64,
                    self.fade_redraw_interval_ms,
                    self.bg_color,
                );
                self.prev = None;
            }
        }

        if self.prev.is_some() {
            return Ok(());
        }

        self.current.draw(target, current_time)?;

        Ok(())
    }
}
