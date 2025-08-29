use crate::super_draw_target::SuperDrawTarget;
use crate::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::ImageDrawable,
    pixelcolor::PixelColor,
    prelude::*,
    primitives::Rectangle,
    Pixel,
};

/// A widget that wraps any ImageDrawable as a widget with color mapping
pub struct Image<I, C = <I as ImageDrawable>::Color>
where
    I: ImageDrawable,
    C: PixelColor,
{
    image: I,
    map_fn: fn(I::Color) -> C,
    needs_redraw: bool,
}

impl<I> Image<I, I::Color>
where
    I: ImageDrawable,
{
    /// Create a new image widget with identity color mapping
    pub fn new(image: I) -> Self {
        Self {
            image,
            map_fn: |c| c, // Identity function
            needs_redraw: true,
        }
    }
}

impl<I, C> Image<I, C>
where
    I: ImageDrawable,
    C: PixelColor,
{
    /// Create a new image widget with a custom color mapping function
    pub fn with_color_map(image: I, map_fn: fn(I::Color) -> C) -> Self {
        Self {
            image,
            map_fn,
            needs_redraw: true,
        }
    }
}

impl<I, C> crate::DynWidget for Image<I, C>
where
    I: ImageDrawable + OriginDimensions,
    C: PixelColor,
{
    fn set_constraints(&mut self, _max_size: Size) {
        // Image has a fixed size based on its content
    }

    fn sizing(&self) -> crate::Sizing {
        self.image.size().into()
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl<I, C> Widget for Image<I, C>
where
    I: ImageDrawable + OriginDimensions,
    C: crate::WidgetColor,
{
    type Color = C;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if !self.needs_redraw {
            return Ok(());
        }

        // Create a color-mapped draw target that transforms colors
        let mut mapped = ColorMappedDrawTarget {
            inner: target,
            map_fn: &self.map_fn,
            _phantom: core::marker::PhantomData,
        };

        // Draw image at origin (0, 0) through the color-mapped target
        embedded_graphics::image::Image::new(&self.image, Point::zero()).draw(&mut mapped)?;

        self.needs_redraw = false;
        Ok(())
    }
}

/// A DrawTarget wrapper that maps colors before drawing
struct ColorMappedDrawTarget<'a, D, F, CSrc, CDst> {
    inner: &'a mut D,
    map_fn: &'a F,
    _phantom: core::marker::PhantomData<(CSrc, CDst)>,
}

impl<'a, D, F, CSrc, CDst> DrawTarget for ColorMappedDrawTarget<'a, D, F, CSrc, CDst>
where
    D: DrawTarget<Color = CDst>,
    F: Fn(CSrc) -> CDst,
    CSrc: PixelColor,
    CDst: PixelColor,
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
        let mapped_colors: alloc::vec::Vec<_> =
            colors.into_iter().map(|c| (self.map_fn)(c)).collect();
        self.inner.fill_contiguous(area, mapped_colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.inner.fill_solid(area, (self.map_fn)(color))
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.inner.clear((self.map_fn)(color))
    }
}

impl<'a, D, F, CSrc, CDst> Dimensions for ColorMappedDrawTarget<'a, D, F, CSrc, CDst>
where
    D: DrawTarget<Color = CDst>,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}
