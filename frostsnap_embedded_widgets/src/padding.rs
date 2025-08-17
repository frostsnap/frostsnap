use super::Widget;
use crate::{super_draw_target::SuperDrawTarget, Instant};
use core::ops::{Deref, DerefMut};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};

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

impl<W: Widget> crate::DynWidget for Padding<W> {
    fn set_constraints(&mut self, max_size: Size) {
        // Reduce max size by padding
        let padded_width = max_size.width.saturating_sub(self.left + self.right);
        let padded_height = max_size.height.saturating_sub(self.top + self.bottom);
        self.child
            .set_constraints(Size::new(padded_width, padded_height));
    }

    fn sizing(&self) -> crate::Sizing {
        let child_sizing = self.child.sizing();
        crate::Sizing {
            width: child_sizing.width + self.left + self.right,
            height: child_sizing.height + self.top + self.bottom,
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Child has no size hint - check if touch is within padded area
        if point.x >= self.left as i32 && point.y >= self.top as i32 {
            // Adjust the touch point by padding offsets
            let adjusted_point = Point::new(point.x - self.left as i32, point.y - self.top as i32);
            return self
                .child
                .handle_touch(adjusted_point, current_time, is_release);
        }

        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Pass vertical drag to child with adjusted y values
        let adjusted_prev_y = prev_y.map(|y| y.saturating_sub(self.top));
        let adjusted_new_y = new_y.saturating_sub(self.top);

        self.child
            .handle_vertical_drag(adjusted_prev_y, adjusted_new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}

impl<W: Widget> Widget for Padding<W> {
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // SuperDrawTarget doesn't expose bounding_box directly, so we work with relative coordinates
        // The padding creates an offset from the current drawing area
        let padded_area = Rectangle::new(
            Point::new(self.left as i32, self.top as i32),
            self.sizing().into(),
        );

        // Draw the child in the padded area
        let mut cropped_target = target.clone().crop(padded_area);
        self.child.draw(&mut cropped_target, current_time)?;

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
