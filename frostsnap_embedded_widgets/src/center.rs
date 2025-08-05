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
    last_child_rect: Option<Rectangle>,
}

impl<W> Center<W> {
    pub fn new(child: W) -> Self {
        Self { 
            child,
            last_child_rect: None,
        }
    }
}

impl<W: Widget> crate::DynWidget for Center<W> {
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if let Some(child_rect) = self.last_child_rect {
            // Check if the touch is within the child's bounds
            if child_rect.contains(point) {
                // Translate the touch point to the child's coordinate system
                let translated_point = Point::new(
                    point.x - child_rect.top_left.x,
                    point.y - child_rect.top_left.y,
                );
                self.child.handle_touch(translated_point, current_time, is_release)
            } else {
                None
            }
        } else {
            // No centering was applied, pass through as-is
            self.child.handle_touch(point, current_time, is_release)
        }
    }
    
    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
        self.child.handle_vertical_drag(start_y, current_y, _is_release);
    }

    fn force_full_redraw(&mut self) {
        self.last_child_rect = None;
        self.child.force_full_redraw()
    }
    
    fn size_hint(&self) -> Option<Size> {
        None
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
            
            // Only recalculate if we don't have a cached rect or if the bounds changed
            let child_rect = if let Some(cached_rect) = self.last_child_rect {
                if cached_rect.size == child_size && target_size == target_bounds.size {
                    // Reuse the cached rectangle
                    cached_rect
                } else {
                    // Recalculate
                    let x_offset = ((target_size.width as i32 - child_size.width as i32) / 2).max(0);
                    let y_offset = ((target_size.height as i32 - child_size.height as i32) / 2).max(0);
                    Rectangle::new(Point::new(x_offset, y_offset), child_size)
                }
            } else {
                // Calculate for the first time
                let x_offset = ((target_size.width as i32 - child_size.width as i32) / 2).max(0);
                let y_offset = ((target_size.height as i32 - child_size.height as i32) / 2).max(0);
                Rectangle::new(Point::new(x_offset, y_offset), child_size)
            };
            
            // Store the rectangle for touch handling
            self.last_child_rect = Some(child_rect);
            
            let mut cropped = target.cropped(&child_rect);
            self.child.draw(&mut cropped, current_time)?;
        } else {
            // No size hint, can't center
            self.last_child_rect = None;
            self.child.draw(target, current_time)?;
        }
        
        Ok(())
    }
}
