use super::Widget;
use crate::Instant;
use crate::prelude::FreeCrop;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};
use core::ops::{Deref, DerefMut};

/// A widget that adds padding around its child
#[derive(PartialEq)]
pub struct Padding<W: Widget> {
    pub child: W,
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

impl<W: Widget> Padding<W> {
    /// Create padding with all sides having the same value
    pub fn all(padding: u32, child: W) -> Self {
        Self {
            child,
            top: padding,
            bottom: padding,
            left: padding,
            right: padding,
        }
    }
    
    /// Create padding with symmetric values (vertical and horizontal)
    pub fn symmetric(vertical: u32, horizontal: u32, child: W) -> Self {
        Self {
            child,
            top: vertical,
            bottom: vertical,
            left: horizontal,
            right: horizontal,
        }
    }
    
    /// Create padding with only specific sides
    pub fn only(child: W) -> PaddingBuilder<W> {
        PaddingBuilder {
            child,
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
        }
    }
    
    /// Create padding with all sides specified
    pub fn new(top: u32, bottom: u32, left: u32, right: u32, child: W) -> Self {
        Self {
            child,
            top,
            bottom,
            left,
            right,
        }
    }
}

/// Builder for creating padding with only specific sides
pub struct PaddingBuilder<W: Widget> {
    child: W,
    top: u32,
    bottom: u32,
    left: u32,
    right: u32,
}

impl<W: Widget> PaddingBuilder<W> {
    pub fn top(mut self, value: u32) -> Self {
        self.top = value;
        self
    }
    
    pub fn bottom(mut self, value: u32) -> Self {
        self.bottom = value;
        self
    }
    
    pub fn left(mut self, value: u32) -> Self {
        self.left = value;
        self
    }
    
    pub fn right(mut self, value: u32) -> Self {
        self.right = value;
        self
    }
    
    pub fn build(self) -> Padding<W> {
        Padding {
            child: self.child,
            top: self.top,
            bottom: self.bottom,
            left: self.left,
            right: self.right,
        }
    }
}

impl<W: Widget> crate::DynWidget for Padding<W> 
{
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Child has no size hint - check if touch is within padded area
        if point.x >= self.left as i32 && point.y >= self.top as i32 {
            // Adjust the touch point by padding offsets
            let adjusted_point = Point::new(
                point.x - self.left as i32,
                point.y - self.top as i32,
            );
            return self.child.handle_touch(adjusted_point, current_time, is_release);
        }

        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Pass vertical drag to child with adjusted y values
        let adjusted_prev_y = prev_y.map(|y| y.saturating_sub(self.top));
        let adjusted_new_y = new_y.saturating_sub(self.top);
        
        self.child.handle_vertical_drag(adjusted_prev_y, adjusted_new_y, is_release);
    }

    fn size_hint(&self) -> Option<Size> {
        // Return the child's size plus padding
        self.child.size_hint().map(|child_size| {
            Size::new(
                child_size.width + self.left + self.right,
                child_size.height + self.top + self.bottom,
            )
        })
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}

impl<W: Widget> Widget for Padding<W> {
    type Color = W::Color;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        let target_bounds = target.bounding_box();
        let padded_origin = Point::new(
            target_bounds.top_left.x + self.left as i32,
            target_bounds.top_left.y + self.top as i32,
        );
        let padded_size = Size::new(
            target_bounds.size.width.saturating_sub(self.left + self.right),
            target_bounds.size.height.saturating_sub(self.top + self.bottom),
        );
        let padded_area = Rectangle::new(padded_origin, padded_size);

        // Create a cropped target with reduced area
        let mut cropped = target.free_cropped(&padded_area);

        // Draw the child in the reduced area
        self.child.draw(&mut cropped, current_time)?;

        Ok(())
    }
    
}

impl<W: Widget> Deref for Padding<W> {
    type Target = W;
    
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl<W: Widget> DerefMut for Padding<W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
