use super::Widget;
use crate::Instant;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::PixelColor,
    primitives::Rectangle,
    Pixel,
};

/// A widget that maps colors from one color space to another.
/// This allows widgets that draw in one color space (e.g., Rgb565) to be rendered
/// to targets that expect a different color space (e.g., Gray2).
pub struct ColorMap<W, C, F> {
    child: W,
    map_fn: F,
    _phantom: core::marker::PhantomData<C>,
}

impl<W, C, F> ColorMap<W, C, F> {
    pub fn new(child: W, map_fn: F) -> Self {
        Self {
            child,
            map_fn,
            _phantom: core::marker::PhantomData,
        }
    }
}

/// A DrawTarget wrapper that maps colors before drawing
struct MappedDrawTarget<'a, D, F, CSrc> {
    inner: &'a mut D,
    map_fn: &'a F,
    _phantom: core::marker::PhantomData<CSrc>,
}

impl<'a, D, F, CSrc> DrawTarget for MappedDrawTarget<'a, D, F, CSrc>
where
    D: DrawTarget,
    F: Fn(CSrc) -> D::Color,
    CSrc: PixelColor,
{
    type Color = CSrc;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.inner.draw_iter(
            pixels
                .into_iter()
                .map(|Pixel(point, color)| Pixel(point, (self.map_fn)(color))),
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        // This is less efficient but correct - we could optimize with unsafe transmute if needed
        let mapped_colors: alloc::vec::Vec<_> = colors
            .into_iter()
            .map(|c| (self.map_fn)(c))
            .collect();
        self.inner.fill_contiguous(area, mapped_colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.inner.fill_solid(area, (self.map_fn)(color))
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.inner.clear((self.map_fn)(color))
    }
}

impl<'a, D, F, CSrc> Dimensions for MappedDrawTarget<'a, D, F, CSrc>
where
    D: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}

impl<W, C, F> Widget for ColorMap<W, C, F>
where
    W: Widget,
    C: PixelColor,
    F: Fn(W::Color) -> C,
{
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        let mut mapped = MappedDrawTarget {
            inner: target,
            map_fn: &self.map_fn,
            _phantom: core::marker::PhantomData,
        };
        self.child.draw(&mut mapped, current_time)?;
        Ok(())
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32) {
        self.child.handle_vertical_drag(start_y, current_y)
    }

    fn size_hint(&self) -> Option<Size> {
        self.child.size_hint()
    }
}

// Example usage:
// let widget = Text::new("Hello");
// let gray_widget = ColorMap::new(widget, |rgb: Rgb565| {
//     // Convert RGB to grayscale
//     let r = ((rgb.r() as u16 * 77) >> 8) as u8;
//     let g = ((rgb.g() as u16 * 151) >> 8) as u8;
//     let b = ((rgb.b() as u16 * 28) >> 8) as u8;
//     let gray = (r + g + b) >> 6; // 0-3 for Gray2
//     Gray2::new(gray)
// });