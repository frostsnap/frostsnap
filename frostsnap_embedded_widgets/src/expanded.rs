use crate::{super_draw_target::SuperDrawTarget, DynWidget, Instant, Sizing, Widget};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
};

/// A widget that expands to fill available space in a flex container (Row or Column)
///
/// Similar to Flutter's Expanded widget, this wraps a child widget and returns true
/// from the `flex()` method, indicating it should expand to fill available space.
///
/// # Example
/// ```ignore
/// let row = Row::new((
///     Text::new("Fixed"),
///     Expanded::new(Text::new("This will expand")),
///     Text::new("Fixed"),
/// ));
/// ```
pub struct Expanded<W> {
    pub child: W,
}

impl<W> Expanded<W> {
    /// Create a new Expanded widget wrapping the given child
    pub fn new(child: W) -> Self {
        Self { child }
    }
}

impl<W: DynWidget> DynWidget for Expanded<W> {
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size);
    }

    fn sizing(&self) -> Sizing {
        self.child.sizing()
    }

    fn flex(&self) -> bool {
        true // This is the key difference - always return true for flex
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}

impl<W> Widget for Expanded<W>
where
    W: Widget,
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
        self.child.draw(target, current_time)
    }
}
