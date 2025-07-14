use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Point,
    pixelcolor::BinaryColor,
    prelude::*,
};

/// A DrawTarget implementation that records pixel positions instead of drawing them.
/// Useful for pre-computing pixel locations for animations.
pub struct PixelRecorder {
    pub pixels: Vec<Point>,
}

impl PixelRecorder {
    pub fn new() -> Self {
        Self { pixels: Vec::new() }
    }
}

impl DrawTarget for PixelRecorder {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            self.pixels.push(pixel.0);
        }
        Ok(())
    }
}

impl OriginDimensions for PixelRecorder {
    fn size(&self) -> Size {
        // Return a large size to ensure all pixels are recorded
        Size::new(1000, 1000)
    }
}