use crate::{
    palette::PALETTE, ColorInterpolate, DynWidget, Frac, Instant, Sizing, SuperDrawTarget, Widget,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    pixelcolor::{Gray8, GrayColor, Rgb565},
    prelude::*,
};
use tinybmp::Bmp;

/// A widget that displays a grayscale BMP with color mapping
///
/// Loads an 8-bit grayscale BMP and renders it with smooth anti-aliased blending
/// between a foreground color (for dark pixels) and background color (for light pixels).
pub struct BmpImage {
    bmp: Bmp<'static, Gray8>,
    foreground_color: Rgb565,
    max_size: Size,
    needs_redraw: bool,
}

impl BmpImage {
    /// Create a new BmpImage from raw BMP data
    ///
    /// # Arguments
    /// * `bmp_data` - Raw bytes of an 8-bit grayscale BMP file (must have 'static lifetime)
    /// * `foreground_color` - Color to use for dark pixels (grayscale value 0)
    ///
    /// The background color is automatically taken from PALETTE.background.
    /// Color blending creates smooth anti-aliasing between foreground and background.
    pub fn new(bmp_data: &'static [u8], foreground_color: Rgb565) -> Self {
        let bmp = Bmp::<Gray8>::from_slice(bmp_data).expect("Failed to load BMP");

        Self {
            bmp,
            foreground_color,
            max_size: Size::zero(),
            needs_redraw: true,
        }
    }
}

impl DynWidget for BmpImage {
    fn set_constraints(&mut self, max_size: Size) {
        self.max_size = max_size;
    }

    fn sizing(&self) -> Sizing {
        let size = self.bmp.bounding_box().size;
        Sizing {
            width: size.width,
            height: size.height,
            dirty_rect: None,
        }
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl Widget for BmpImage {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if !self.needs_redraw {
            return Ok(());
        }

        // Draw each pixel with color mapping
        for pixel in self.bmp.pixels() {
            let Pixel(point, gray) = pixel;
            // Blend between foreground and background color based on grayscale intensity
            // White (255) = background, Black (0) = foreground color
            let intensity = gray.luma();
            // Convert intensity (0-255) to Frac (0.0-1.0)
            let frac = Frac::from_ratio(intensity as u32, 255);
            let color = self.foreground_color.interpolate(PALETTE.background, frac);
            target.draw_iter(core::iter::once(Pixel(point, color)))?;
        }

        self.needs_redraw = false;
        Ok(())
    }
}
