use crate::{image::Image, palette::PALETTE, vec_framebuffer::VecFramebuffer};
use embedded_graphics::{
    geometry::Dimensions,
    pixelcolor::{Gray8, GrayColor, Rgb565, RgbColor},
    prelude::*,
};
use tinybmp::Bmp;

/// Blend between two Rgb565 colors based on intensity (0-255)
/// intensity=0 returns color_a, intensity=255 returns color_b
fn blend_rgb565(color_a: Rgb565, color_b: Rgb565, intensity: u8) -> Rgb565 {
    // Extract RGB components (5-6-5 bit format)
    let r1 = color_a.r();
    let g1 = color_a.g();
    let b1 = color_a.b();

    let r2 = color_b.r();
    let g2 = color_b.g();
    let b2 = color_b.b();

    // Blend using intensity (0-255) as alpha
    // Convert to wider type to avoid overflow
    let alpha = intensity as u16;
    let inv_alpha = 255u16 - alpha;

    let r = ((r1 as u16 * inv_alpha + r2 as u16 * alpha) / 255) as u8;
    let g = ((g1 as u16 * inv_alpha + g2 as u16 * alpha) / 255) as u8;
    let b = ((b1 as u16 * inv_alpha + b2 as u16 * alpha) / 255) as u8;

    Rgb565::new(r, g, b)
}

/// A widget that displays a grayscale BMP with color mapping
///
/// Loads an 8-bit grayscale BMP and renders it with smooth anti-aliased blending
/// between a foreground color (for dark pixels) and background color (for light pixels).
#[derive(frostsnap_macros::Widget)]
pub struct BmpImage {
    #[widget_delegate]
    image: Image<VecFramebuffer<Rgb565>, Rgb565>,
}

impl BmpImage {
    /// Create a new BmpImage from raw BMP data
    ///
    /// # Arguments
    /// * `bmp_data` - Raw bytes of an 8-bit grayscale BMP file
    /// * `foreground_color` - Color to use for dark pixels (grayscale value 0)
    ///
    /// The background color is automatically taken from PALETTE.background.
    /// Color blending creates smooth anti-aliasing between foreground and background.
    pub fn new(bmp_data: &[u8], foreground_color: Rgb565) -> Self {
        let bmp = Bmp::<Gray8>::from_slice(bmp_data).expect("Failed to load BMP");

        // Get dimensions
        let width = bmp.bounding_box().size.width as usize;
        let height = bmp.bounding_box().size.height as usize;

        // Create RGB565 framebuffer with color mapping already applied
        let mut rgb_framebuffer = VecFramebuffer::new(width, height);

        // Convert grayscale BMP to RGB with color blending
        for pixel in bmp.pixels() {
            let Pixel(point, gray) = pixel;
            // Blend between foreground and background color based on grayscale intensity
            // White (255) = background, Black (0) = foreground color
            let intensity = gray.luma();
            let color = blend_rgb565(foreground_color, PALETTE.background, intensity);
            VecFramebuffer::<Rgb565>::set_pixel(&mut rgb_framebuffer, point, color);
        }

        // Create image widget with the pre-colored framebuffer
        let image = Image::new(rgb_framebuffer);

        Self { image }
    }
}
