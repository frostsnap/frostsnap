use crate::Widget;
use crate::{super_draw_target::SuperDrawTarget, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};

/// A widget that centers its child both horizontally and vertically
#[derive(PartialEq)]
pub struct Center<W> {
    pub child: W,
    constraints: Option<Size>,
    child_rect: Rectangle,
    sizing: crate::Sizing,
}

impl<W> Center<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            constraints: None,
            child_rect: Rectangle::zero(),
            sizing: crate::Sizing::default(),
        }
    }
}

impl<W: Widget> crate::DynWidget for Center<W> {
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
        self.child.set_constraints(max_size);

        // Calculate centered position for child
        let child_sizing = self.child.sizing();
        let child_size: Size = child_sizing.into();
        let x_offset = ((max_size.width as i32 - child_size.width as i32) / 2).max(0);
        let y_offset = ((max_size.height as i32 - child_size.height as i32) / 2).max(0);
        self.child_rect = Rectangle::new(Point::new(x_offset, y_offset), child_size);

        // Calculate our sizing with the child's dirty_rect offset by the centering position
        let mut child_dirty = child_sizing.dirty_rect();
        child_dirty.top_left += self.child_rect.top_left;
        let dirty_rect = Some(child_dirty);

        self.sizing = crate::Sizing {
            width: max_size.width,
            height: max_size.height,
            dirty_rect,
        };
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Check if the touch is within the child's bounds
        if self.child_rect.contains(point) {
            // Translate the touch point to the child's coordinate system
            let translated_point = Point::new(
                point.x - self.child_rect.top_left.x,
                point.y - self.child_rect.top_left.y,
            );
            if let Some(mut key_touch) =
                self.child
                    .handle_touch(translated_point, current_time, is_release)
            {
                // Translate the KeyTouch rectangle back to parent coordinates
                key_touch.translate(self.child_rect.top_left);
                Some(key_touch)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
        self.child
            .handle_vertical_drag(start_y, current_y, _is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw()
    }
}

impl<W: Widget> Widget for Center<W> {
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.constraints.unwrap();

        self.child
            .draw(&mut target.clone().crop(self.child_rect), current_time)?;

        Ok(())
    }
}
