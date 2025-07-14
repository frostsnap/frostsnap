use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::PixelColor,
    prelude::*,
    primitives::Rectangle,
    Pixel,
};

/// Records both pixel positions and their colors
pub struct ColorPixelRecorder<C: PixelColor> {
    pub pixels: Vec<(Point, C)>,
}

impl<C: PixelColor> ColorPixelRecorder<C> {
    pub fn new() -> Self {
        Self {
            pixels: Vec::new(),
        }
    }
}

impl<C: PixelColor> DrawTarget for ColorPixelRecorder<C> {
    type Color = C;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.pixels.push((point, color));
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut colors_iter = colors.into_iter();
        for y in area.top_left.y..area.top_left.y + area.size.height as i32 {
            for x in area.top_left.x..area.top_left.x + area.size.width as i32 {
                if let Some(color) = colors_iter.next() {
                    self.pixels.push((Point::new(x, y), color));
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        for y in area.top_left.y..area.top_left.y + area.size.height as i32 {
            for x in area.top_left.x..area.top_left.x + area.size.width as i32 {
                self.pixels.push((Point::new(x, y), color));
            }
        }
        Ok(())
    }

    fn clear(&mut self, _color: Self::Color) -> Result<(), Self::Error> {
        // For recording purposes, we don't actually clear anything
        // This could be implemented if needed by recording a full-screen fill
        Ok(())
    }
}

impl<C: PixelColor> Dimensions for ColorPixelRecorder<C> {
    fn bounding_box(&self) -> Rectangle {
        // Return a large bounding box since we're just recording
        Rectangle::new(Point::new(0, 0), Size::new(u32::MAX, u32::MAX))
    }
}