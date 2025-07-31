use crate::{Widget, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
};

/// A widget that switches between left and right widgets based on show_right flag
pub struct Either<L, R> {
    pub left: L,
    pub right: R,
    pub show_right: bool,
}

impl<L, R> Either<L, R> {
    /// Create an Either showing the left widget by default
    pub fn new(left: L, right: R) -> Self {
        Self {
            left,
            right,
            show_right: false,
        }
    }
    
    /// Switch to show the left widget
    pub fn show_left(&mut self) {
        self.show_right = false;
    }
    
    /// Switch to show the right widget
    pub fn show_right(&mut self) {
        self.show_right = true;
    }
    
    /// Check if showing the left widget
    pub fn is_showing_left(&self) -> bool {
        !self.show_right
    }
    
    /// Check if showing the right widget
    pub fn is_showing_right(&self) -> bool {
        self.show_right
    }
}

impl<L, R, C> Widget for Either<L, R>
where
    L: Widget<Color = C>,
    R: Widget<Color = C>,
    C: PixelColor,
{
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        if self.show_right {
            self.right.draw(target, current_time)
        } else {
            self.left.draw(target, current_time)
        }
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if self.show_right {
            self.right.handle_touch(point, current_time, is_release)
        } else {
            self.left.handle_touch(point, current_time, is_release)
        }
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if self.show_right {
            self.right.handle_vertical_drag(prev_y, new_y, is_release)
        } else {
            self.left.handle_vertical_drag(prev_y, new_y, is_release)
        }
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Return the size hint from the currently active widget
        if self.show_right {
            self.right.size_hint()
        } else {
            self.left.size_hint()
        }
    }
    
    fn force_full_redraw(&mut self) {
        // Force redraw on both widgets to ensure clean switching
        self.left.force_full_redraw();
        self.right.force_full_redraw();
    }
}