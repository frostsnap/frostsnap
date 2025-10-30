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

/// A widget that wraps any ImageDrawable as a widget
pub struct Image<I> {
    image: I,
    needs_redraw: bool,
}

impl<I> Image<I> {
    /// Create a new image widget
    pub fn new(image: I) -> Self {
        Self {
            image,
            needs_redraw: true,
        }
    }
}

impl<I> crate::DynWidget for Image<I>
where
    I: OriginDimensions,
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

impl<I> Widget for Image<I>
where
    I: ImageDrawable + OriginDimensions,
    I::Color: crate::WidgetColor,
{
    type Color = I::Color;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if !self.needs_redraw {
            return Ok(());
        }

        // Draw image at origin (0, 0)
        embedded_graphics::image::Image::new(&self.image, Point::zero()).draw(target)?;

        self.needs_redraw = false;
        Ok(())
    }
}

// Specialized Widget impl for GrayToAlpha that uses SuperDrawTarget's background_color
impl<I, C> Widget for Image<GrayToAlpha<I, C>>
where
    I: ImageDrawable + OriginDimensions,
    I::Color: embedded_graphics::pixelcolor::GrayColor,
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

        let background_color = target.background_color();

        let mut mapped = GrayToAlphaDrawTarget {
            inner: target,
            foreground_color: self.image.foreground_color,
            background_color,
            _phantom: core::marker::PhantomData,
        };

        embedded_graphics::image::Image::new(&self.image.image, Point::zero()).draw(&mut mapped)?;

        self.needs_redraw = false;
        Ok(())
    }
}

/// Wraps a grayscale ImageDrawable and blends it with a foreground color
///
/// The grayscale luma value is interpreted as alpha - dark pixels (0) use the foreground color,
/// light pixels (255) use the background color from SuperDrawTarget, and intermediate values
/// are interpolated between the two.
pub struct GrayToAlpha<I, C> {
    image: I,
    foreground_color: C,
}

impl<I, C> GrayToAlpha<I, C>
where
    I: ImageDrawable,
    I::Color: embedded_graphics::pixelcolor::GrayColor,
    C: PixelColor,
{
    /// Create a new grayscale to alpha blended image
    ///
    /// # Arguments
    /// * `image` - The grayscale image source
    /// * `foreground_color` - Color to use for dark pixels (luma 0)
    ///
    /// Light pixels (luma 255) will use the background color from SuperDrawTarget at draw time.
    pub fn new(image: I, foreground_color: C) -> Self {
        Self {
            image,
            foreground_color,
        }
    }
}

impl<I, C> OriginDimensions for GrayToAlpha<I, C>
where
    I: ImageDrawable + OriginDimensions,
    I::Color: embedded_graphics::pixelcolor::GrayColor,
    C: PixelColor,
{
    fn size(&self) -> Size {
        self.image.size()
    }
}

/// A DrawTarget wrapper that converts grayscale to color via alpha blending
struct GrayToAlphaDrawTarget<'a, D, CSrc, C> {
    inner: &'a mut D,
    foreground_color: C,
    background_color: C,
    _phantom: core::marker::PhantomData<CSrc>,
}

impl<'a, D, CSrc, C> DrawTarget for GrayToAlphaDrawTarget<'a, D, CSrc, C>
where
    D: DrawTarget<Color = C>,
    CSrc: embedded_graphics::pixelcolor::GrayColor,
    C: crate::ColorInterpolate,
{
    type Color = CSrc;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.inner
            .draw_iter(pixels.into_iter().map(|Pixel(point, gray)| {
                let intensity = gray.luma();
                let frac = crate::Frac::from_ratio(intensity as u32, 255);
                let color = self
                    .foreground_color
                    .interpolate(self.background_color, frac);
                Pixel(point, color)
            }))
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mapped_colors = colors.into_iter().map(|gray| {
            let intensity = gray.luma();
            let frac = crate::Frac::from_ratio(intensity as u32, 255);
            self.foreground_color
                .interpolate(self.background_color, frac)
        });
        self.inner.fill_contiguous(area, mapped_colors)
    }

    fn fill_solid(&mut self, area: &Rectangle, gray: Self::Color) -> Result<(), Self::Error> {
        let intensity = gray.luma();
        let frac = crate::Frac::from_ratio(intensity as u32, 255);
        let color = self
            .foreground_color
            .interpolate(self.background_color, frac);
        self.inner.fill_solid(area, color)
    }

    fn clear(&mut self, gray: Self::Color) -> Result<(), Self::Error> {
        let intensity = gray.luma();
        let frac = crate::Frac::from_ratio(intensity as u32, 255);
        let color = self
            .foreground_color
            .interpolate(self.background_color, frac);
        self.inner.clear(color)
    }
}

impl<'a, D, CSrc, C> Dimensions for GrayToAlphaDrawTarget<'a, D, CSrc, C>
where
    D: DrawTarget<Color = C>,
    CSrc: embedded_graphics::pixelcolor::GrayColor,
    C: PixelColor,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}
