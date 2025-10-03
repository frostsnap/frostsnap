/// Gray4TextStyle - implements embedded_graphics TextRenderer for Gray4 fonts
use super::gray4_font::{GlyphInfo, Gray4Font};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{GrayColor, Rgb565, RgbColor},
    primitives::{Line, Primitive, PrimitiveStyle, Rectangle},
    text::{
        renderer::{CharacterStyle, TextMetrics, TextRenderer},
        Baseline, DecorationColor,
    },
    Drawable, Pixel,
};

/// Pre-calculated color cache for all 16 gray levels
#[derive(Clone, Copy, Debug, PartialEq)]
struct ColorCache {
    colors: [Rgb565; 16],
}

impl ColorCache {
    /// Create a new color cache with pre-blended colors
    fn new(text_color: Rgb565, background_color: Rgb565) -> Self {
        let mut colors = [Rgb565::BLACK; 16];

        for (i, color) in colors.iter_mut().enumerate() {
            *color = Self::blend_color(i as u8, text_color, background_color);
        }

        Self { colors }
    }

    /// Blend color with alpha for anti-aliasing
    fn blend_color(alpha: u8, text_color: Rgb565, background_color: Rgb565) -> Rgb565 {
        use crate::{ColorInterpolate, Frac};

        // Convert alpha (0-15) to Frac (0.0-1.0) for interpolation
        let alpha_frac = Frac::from_ratio(alpha as u32, 15);

        // Interpolate from background to text color
        background_color.interpolate(text_color, alpha_frac)
    }
}

/// Text style for Gray4 fonts that implements TextRenderer
#[derive(Clone)]
pub struct Gray4TextStyle {
    /// The Gray4 font to use
    pub font: &'static Gray4Font,
    /// Pre-cached colors for all 16 gray levels
    color_cache: ColorCache,
    /// Background color used for alpha blending
    background_color: Rgb565,
    /// Text color (needed for CharacterStyle::set_text_color)
    text_color: Rgb565,
    /// Underline color and thickness
    underline_color: DecorationColor<Rgb565>,
    /// Strikethrough color and thickness
    strikethrough_color: DecorationColor<Rgb565>,
}

impl Gray4TextStyle {
    /// Create a new Gray4TextStyle with the given font and text color
    /// Defaults to black background for alpha blending
    pub fn new(font: &'static Gray4Font, text_color: Rgb565) -> Self {
        Self {
            font,
            color_cache: ColorCache::new(text_color, Rgb565::BLACK),
            background_color: Rgb565::BLACK,
            text_color,
            underline_color: DecorationColor::None,
            strikethrough_color: DecorationColor::None,
        }
    }

    /// Set the underline color
    pub fn with_underline_color(mut self, underline_color: DecorationColor<Rgb565>) -> Self {
        self.underline_color = underline_color;
        self
    }

    /// Draw a single glyph with anti-aliasing
    fn draw_glyph<D>(
        &self,
        position: Point,
        glyph: &GlyphInfo,
        target: &mut D,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        // Calculate glyph position with bearing offsets
        let draw_x = position.x + glyph.x_offset as i32;
        let draw_y = position.y + glyph.y_offset as i32;

        // Get iterator of pixels and map Gray4 to Rgb565 with correct position
        let pixels = self.font.glyph_pixels(glyph).map(|Pixel(point, gray)| {
            let color = self.color_cache.colors[gray.luma() as usize];
            Pixel(Point::new(draw_x + point.x, draw_y + point.y), color)
        });

        // Draw all pixels in one call
        target.draw_iter(pixels)
    }
}

impl TextRenderer for Gray4TextStyle {
    type Color = Rgb565;

    fn draw_string<D>(
        &self,
        text: &str,
        position: Point,
        baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let y_offset = match baseline {
            Baseline::Top => 0,
            Baseline::Bottom => -(self.font.line_height as i32),
            Baseline::Middle => -(self.font.line_height as i32 / 2),
            Baseline::Alphabetic => -(self.font.baseline as i32),
        };

        let mut x = position.x;
        let y = position.y + y_offset;

        for ch in text.chars() {
            if let Some(glyph) = self.font.get_glyph(ch) {
                self.draw_glyph(Point::new(x, y), glyph, target)?;
                x += glyph.x_advance as i32;
            } else if ch == ' ' {
                // Space character
                x += (self.font.line_height / 4) as i32;
            } else {
                // Unknown character - use placeholder width
                x += (self.font.line_height / 3) as i32;
            }
        }

        // Underline
        match self.underline_color {
            DecorationColor::None => {}
            DecorationColor::TextColor => {
                let underline_y = y + self.font.baseline as i32 + 2;
                Line::new(
                    Point::new(position.x, underline_y),
                    Point::new(x, underline_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(self.text_color, 1))
                .draw(target)?;
            }
            DecorationColor::Custom(color) => {
                let underline_y = y + self.font.baseline as i32 + 2;
                Line::new(
                    Point::new(position.x, underline_y),
                    Point::new(x, underline_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(color, 1))
                .draw(target)?;
            }
        }

        // Strikethrough
        match self.strikethrough_color {
            DecorationColor::None => {}
            DecorationColor::TextColor => {
                let strikethrough_y = y + (self.font.line_height as i32) / 2;
                Line::new(
                    Point::new(position.x, strikethrough_y),
                    Point::new(x, strikethrough_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(self.text_color, 1))
                .draw(target)?;
            }
            DecorationColor::Custom(color) => {
                let strikethrough_y = y + (self.font.line_height as i32) / 2;
                Line::new(
                    Point::new(position.x, strikethrough_y),
                    Point::new(x, strikethrough_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(color, 1))
                .draw(target)?;
            }
        }

        Ok(Point::new(x, position.y))
    }

    fn draw_whitespace<D>(
        &self,
        width: u32,
        position: Point,
        _baseline: Baseline,
        _target: &mut D,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Just advance the position - no drawing needed for whitespace
        Ok(Point::new(position.x + width as i32, position.y))
    }

    fn measure_string(&self, text: &str, position: Point, _baseline: Baseline) -> TextMetrics {
        let mut width = 0u32;

        for ch in text.chars() {
            if let Some(glyph) = self.font.get_glyph(ch) {
                width += glyph.x_advance as u32;
            } else if ch == ' ' {
                width += self.font.line_height / 4;
            } else {
                width += self.font.line_height / 3;
            }
        }

        TextMetrics {
            bounding_box: Rectangle::new(position, Size::new(width, self.font.line_height)),
            next_position: Point::new(position.x + width as i32, position.y),
        }
    }

    fn line_height(&self) -> u32 {
        self.font.line_height
    }
}

impl CharacterStyle for Gray4TextStyle {
    type Color = Rgb565;

    fn set_text_color(&mut self, text_color: Option<Self::Color>) {
        if let Some(color) = text_color {
            if self.text_color != color {
                self.text_color = color;
                // Rebuild cache with new text color
                self.color_cache = ColorCache::new(self.text_color, self.background_color);
            }
        }
    }

    fn set_background_color(&mut self, background_color: Option<Self::Color>) {
        let bg_color = background_color.unwrap_or(Rgb565::BLACK);
        // Only rebuild cache if background actually changed
        if self.background_color != bg_color {
            self.background_color = bg_color;
            self.color_cache = ColorCache::new(self.text_color, bg_color);
        }
    }

    fn set_underline_color(&mut self, underline_color: DecorationColor<Self::Color>) {
        self.underline_color = underline_color;
    }

    fn set_strikethrough_color(&mut self, strikethrough_color: DecorationColor<Self::Color>) {
        self.strikethrough_color = strikethrough_color;
    }
}
