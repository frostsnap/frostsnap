use crate::{DynWidget, Instant, Key, KeyTouch, Sizing, SuperDrawTarget, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};

/// A widget that listens for touch events and converts them to KeyTouch events
pub struct TouchListener<W> {
    child: W,
    on_touch: fn(Point, Instant, bool, &mut W) -> Option<Key>,
    sizing: Option<Sizing>,
}

impl<W> TouchListener<W> {
    /// Create a new TouchListener that wraps a child widget
    pub fn new(child: W, on_touch: fn(Point, Instant, bool, &mut W) -> Option<Key>) -> Self {
        Self {
            child,
            on_touch,
            sizing: None,
        }
    }
}

impl<W> DynWidget for TouchListener<W>
where
    W: DynWidget,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size);
        self.sizing = Some(self.child.sizing());
    }

    fn sizing(&self) -> Sizing {
        self.sizing.expect("set_constraints must be called before sizing")
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        // Call our handler with the child
        if let Some(key) = (self.on_touch)(point, current_time, is_release, &mut self.child) {
            // Create the KeyTouch with our bounds
            let bounds = Rectangle::new(Point::zero(), self.sizing.unwrap().into());
            Some(KeyTouch::new(key, bounds))
        } else {
            // Also pass through to child in case it has its own handling
            self.child.handle_touch(point, current_time, is_release)
        }
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(start_y, current_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}

impl<W> Widget for TouchListener<W>
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