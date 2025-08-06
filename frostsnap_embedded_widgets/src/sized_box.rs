use super::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{PixelColor, Rgb565},
    prelude::*,
};

/// A simple widget that has a fixed size but no content
#[derive(Debug, PartialEq)]
pub struct SizedBox<C = Rgb565> {
    size: Size,
    _phantom: core::marker::PhantomData<C>,
}

impl<C> SizedBox<C> {
    pub fn new(size: Size) -> Self {
        Self { 
            size,
            _phantom: core::marker::PhantomData,
        }
    }
    
    /// Create a SizedBox with only width set (height is 0)
    pub fn width(width: u32) -> Self {
        Self {
            size: Size::new(width, 0),
            _phantom: core::marker::PhantomData,
        }
    }
    
    /// Create a SizedBox with only height set (width is 0)
    pub fn height(height: u32) -> Self {
        Self {
            size: Size::new(0, height),
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<C: PixelColor> crate::DynWidget for SizedBox<C>
{
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No-op
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}

impl<C: PixelColor> Widget for SizedBox<C> {
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        _target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Don't draw anything - this is just a placeholder widget
        Ok(())
    }

}