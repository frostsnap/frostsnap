use crate::{
    palette::PALETTE, DynWidget, Instant, KeyTouch, Sizing, SuperDrawTarget, Widget, WidgetColor,
};
use core::ops::{Deref, DerefMut};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

/// A temporary hack widget that clears the screen once before drawing its child
#[derive(Debug)]
pub struct OneTimeClearHack<W> {
    child: W,
    needs_clear: bool,
}

impl<W> OneTimeClearHack<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            needs_clear: true,
        }
    }

    pub fn inner(&self) -> &W {
        &self.child
    }

    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.child
    }
}

impl<W> Deref for OneTimeClearHack<W> {
    type Target = W;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl<W> DerefMut for OneTimeClearHack<W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl<W> DynWidget for OneTimeClearHack<W>
where
    W: DynWidget,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size);
    }

    fn sizing(&self) -> Sizing {
        self.child.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        self.child
            .handle_vertical_drag(start_y, current_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.needs_clear = true;
        self.child.force_full_redraw();
    }
}

impl<W> Widget for OneTimeClearHack<W>
where
    W: Widget,
    W::Color: WidgetColor + From<Rgb565>,
{
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Clear the screen once if needed
        if self.needs_clear {
            target.clear(PALETTE.background.into())?;
            self.needs_clear = false;
        }

        // Draw the child
        self.child.draw(target, current_time)
    }
}
