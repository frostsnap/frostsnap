use super::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
};

/// A simple widget that has a fixed size but no content
#[derive(Debug)]
pub struct SizedBox {
    size: Size,
}

impl SizedBox {
    pub fn new(size: Size) -> Self {
        Self { size }
    }
}

impl Widget for SizedBox {
    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        _target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Don't draw anything - this is just a placeholder widget
        Ok(())
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<crate::graphics::widgets::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32) {
        // No-op
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}