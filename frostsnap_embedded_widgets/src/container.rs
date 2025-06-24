use super::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
};

/// A container that can optionally draw a border around its child
pub struct Container<W> 
where
    W: Widget,
{
    pub child: W,
    size: Option<Size>,
    border_style: Option<PrimitiveStyle<W::Color>>,
    corner_radius: Option<Size>,
    border_needs_redraw: bool,
}

impl<W: Widget> Container<W> {
    /// Create a container that inherits size from its child
    pub fn new(child: W) -> Self {
        Self {
            child,
            size: None,
            border_style: None,
            corner_radius: None,
            border_needs_redraw: true,
        }
    }
    
    /// Create a container with an explicit size
    pub fn with_size(child: W, size: Size) -> Self {
        Self {
            child,
            size: Some(size),
            border_style: None,
            corner_radius: None,
            border_needs_redraw: true,
        }
    }
    
    /// Set the border style
    pub fn with_border(mut self, border_style: PrimitiveStyle<W::Color>) -> Self {
        self.border_style = Some(border_style);
        self
    }
    
    /// Set the corner radius for rounded borders
    pub fn with_corner_radius(mut self, corner_radius: Size) -> Self {
        self.corner_radius = Some(corner_radius);
        self
    }
    
    /// Get the effective size of the container
    fn effective_size(&self) -> Option<Size> {
        self.size.or_else(|| self.child.size_hint())
    }
}

impl<W: Widget> Widget for Container<W> {
    type Color = W::Color;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if let Some(size) = self.effective_size() {
            if self.border_needs_redraw {
                if let Some(style) = self.border_style {
                    if let Some(corner_radius) = self.corner_radius {
                        RoundedRectangle::with_equal_corners(
                            Rectangle::new(Point::zero(), size),
                            corner_radius,
                        )
                        .into_styled(style)
                        .draw(target)?;
                    } else {
                        Rectangle::new(Point::zero(), size)
                            .into_styled(style)
                            .draw(target)?;
                    }
                }
                self.border_needs_redraw = false;
            }
            
            let bounds = Rectangle::new(Point::zero(), size);
            let mut cropped = target.cropped(&bounds);
            self.child.draw(&mut cropped, current_time)?;
        } else {
            self.child.draw(target, current_time)?;
        }
        
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if let Some(size) = self.effective_size() {
            let bounds = Rectangle::new(Point::zero(), size);
            if bounds.contains(point) {
                self.child.handle_touch(point, current_time, is_release)
            } else {
                None
            }
        } else {
            self.child.handle_touch(point, current_time, is_release)
        }
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, is_release);
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.effective_size()
    }
    
    fn force_full_redraw(&mut self) {
        self.border_needs_redraw = true;
        self.child.force_full_redraw();
    }
}

impl<W: Widget> core::fmt::Debug for Container<W> 
where
    W: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Container")
            .field("child", &self.child)
            .field("size", &self.size)
            .field("has_border", &self.border_style.is_some())
            .finish()
    }
}