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
    child: W,
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
        // Get the size of the child widget
        if let Some(child_size) = self.child.size_hint() {
            // Get the size of the target
            let target_bounds = target.bounding_box();
            let target_size = target_bounds.size;
            
            // Calculate centering offset
            let x_offset = ((target_size.width as i32 - child_size.width as i32) / 2).max(0);
            let y_offset = ((target_size.height as i32 - child_size.height as i32) / 2).max(0);
            
            // Draw the child centered
            let offset = Point::new(x_offset, y_offset);
            let mut translated = target.translated(offset);
            self.child.draw(&mut translated, current_time)?;
        } else {
            // If child has no size hint, just draw it normally
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
        // Adjust touch point based on centering offset
        if let Some(child_size) = self.child.size_hint() {
            // Calculate the same offset as in draw
            let target_size = Size::new(240, 280); // TODO: This is hardcoded, should get from somewhere
            let x_offset = ((target_size.width as i32 - child_size.width as i32) / 2).max(0);
            let y_offset = ((target_size.height as i32 - child_size.height as i32) / 2).max(0);
            
            // Adjust the touch point
            let adjusted_point = Point::new(point.x - x_offset, point.y - y_offset);
            
            // Check if touch is within child bounds
            let child_bounds = Rectangle::new(Point::zero(), child_size);
            if child_bounds.contains(adjusted_point) {
                return self.child.handle_touch(adjusted_point, current_time, is_release);
            }
        }
        None
    }
    
    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32) {
        // For now, just pass through to child
        self.child.handle_vertical_drag(start_y, current_y);
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Center widget takes all available space
        None
    }
}