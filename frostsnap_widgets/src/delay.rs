use crate::super_draw_target::SuperDrawTarget;
use crate::{DynWidget, Instant, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
};

/// Delays drawing a child widget until a specified duration has elapsed since the first draw call.
///
/// Once the delay has elapsed, the child draws unconditionally on all subsequent frames.
/// The delay fires only once — there is no reset mechanism.
pub struct Delay<W> {
    pub child: W,
    delay_ms: u64,
    first_draw_at: Option<Instant>,
    active: bool,
}

impl<W> Delay<W> {
    pub fn new(child: W, delay_ms: u64) -> Self {
        Self {
            child,
            delay_ms,
            first_draw_at: None,
            active: false,
        }
    }
}

impl<W: Widget> DynWidget for Delay<W> {
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.child.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if self.active {
            self.child.handle_touch(point, current_time, is_release)
        } else {
            None
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if self.active {
            self.child.handle_vertical_drag(prev_y, new_y, is_release);
        }
    }

    fn force_full_redraw(&mut self) {
        if self.active {
            self.child.force_full_redraw();
        }
    }
}

impl<W: Widget> Widget for Delay<W> {
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if !self.active {
            let first_draw = *self.first_draw_at.get_or_insert(current_time);
            let elapsed = current_time.saturating_duration_since(first_draw);
            self.active = elapsed >= self.delay_ms;
        }

        if self.active {
            self.child.draw(target, current_time)?;
        }

        Ok(())
    }
}
