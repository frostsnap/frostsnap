use super::Widget;
use crate::Instant;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    prelude::*,
    primitives::Rectangle,
};

/// A widget that centers its child both horizontally and vertically
pub struct Center<W> {
    pub child: W,
}

impl<W> Center<W> {
    pub fn new(child: W) -> Self {
        Self { child }
    }
}

impl<W: Widget> Widget for Center<W> {
    type Color = W::Color;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        if let Some(child_size) = self.child.size_hint() {
            let target_bounds = target.bounding_box();
            let target_size = target_bounds.size;
            
            let x_offset = ((target_size.width as i32 - child_size.width as i32) / 2).max(0);
            let y_offset = ((target_size.height as i32 - child_size.height as i32) / 2).max(0);
            
            let offset = Point::new(x_offset, y_offset);
            let child_rect = Rectangle::new(offset, child_size);
            let mut cropped = target.cropped(&child_rect);
            self.child.draw(&mut cropped, current_time)?;
        } else {
            self.child.draw(target, current_time)?;
        }
        
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
        self.child.handle_vertical_drag(start_y, current_y, _is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw()
    }
    
    fn size_hint(&self) -> Option<Size> {
        None
    }
}
