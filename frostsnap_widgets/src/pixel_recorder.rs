use crate::compressed_point::CompressedPoint;
use alloc::vec::Vec;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::BinaryColor, prelude::*};

/// A DrawTarget implementation that records pixel positions instead of drawing them.
/// Useful for pre-computing pixel locations for animations.
pub struct PixelRecorder {
    pub pixels: Vec<CompressedPoint>,
}

impl Default for PixelRecorder {
    fn default() -> Self {
        Self::new()
    }
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
            self.pixels.push(CompressedPoint::new(pixel.0));
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
