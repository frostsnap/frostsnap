use super::Widget;
use crate::super_draw_target::SuperDrawTarget;
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

impl<C: PixelColor> crate::DynWidget for SizedBox<C> {
    fn set_constraints(&mut self, _max_size: Size) {
        // SizedBox has a fixed size, ignores constraints
    }

    fn sizing(&self) -> crate::Sizing {
        self.size.into()
    }

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
}

impl<C: crate::WidgetColor> Widget for SizedBox<C> {
    type Color = C;

    fn draw<D>(
        &mut self,
        _target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Don't draw anything - this is just a placeholder widget
        Ok(())
    }
}
