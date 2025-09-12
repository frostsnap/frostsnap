/// Gray4TextStyle - implements embedded_graphics TextRenderer for Gray4 fonts
use super::gray4_font::{Gray4Font, GlyphInfo};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor, GrayColor},
    primitives::{Line, PrimitiveStyle, Primitive, Rectangle},
    text::{
        renderer::{CharacterStyle, TextMetrics, TextRenderer},
        Baseline, DecorationColor,
    },
    Drawable, Pixel,
};

/// Pre-calculated color cache for all 16 gray levels
#[derive(Clone)]
struct ColorCache {
    colors: [Rgb565; 16],
}

impl ColorCache {
    /// Create a new color cache with pre-blended colors
    fn new(text_color: Rgb565, background_color: Option<Rgb565>) -> Self {
        let mut colors = [Rgb565::BLACK; 16];
        
        for i in 0..16 {
            colors[i] = Self::blend_color(i as u8, text_color, background_color);
        }
        
        Self { colors }
    }
    
    /// Blend color with alpha for anti-aliasing
    fn blend_color(alpha: u8, text_color: Rgb565, background_color: Option<Rgb565>) -> Rgb565 {
        if alpha == 0 {
            // Fully transparent - use background or transparent
            background_color.unwrap_or(Rgb565::BLACK)
        } else if alpha == 15 {
            // Fully opaque
            text_color
        } else {
            // Blend between background and text color
            let bg = background_color.unwrap_or(Rgb565::BLACK);
            
            // Extract RGB components
            use embedded_graphics::pixelcolor::{IntoStorage, raw::RawU16};
            let text_raw = text_color.into_storage();
            let bg_raw = bg.into_storage();
            
            let text_r = ((text_raw >> 11) & 0x1F) as u16;
            let text_g = ((text_raw >> 5) & 0x3F) as u16;
            let text_b = (text_raw & 0x1F) as u16;
            
            let bg_r = ((bg_raw >> 11) & 0x1F) as u16;
            let bg_g = ((bg_raw >> 5) & 0x3F) as u16;
            let bg_b = (bg_raw & 0x1F) as u16;
            
            // Alpha blend
            let alpha_f = alpha as u16;
            let inv_alpha = 15 - alpha_f;
            
            let r = (text_r * alpha_f + bg_r * inv_alpha) / 15;
            let g = (text_g * alpha_f + bg_g * inv_alpha) / 15;
            let b = (text_b * alpha_f + bg_b * inv_alpha) / 15;
            
            Rgb565::from(RawU16::new(
                ((r & 0x1F) << 11) | 
                ((g & 0x3F) << 5) | 
                (b & 0x1F)
            ))
        }
    }
}

/// Text style for Gray4 fonts that implements TextRenderer
#[derive(Clone)]
pub struct Gray4TextStyle<'a> {
    /// The Gray4 font to use
    pub font: &'a Gray4Font,
    /// Pre-cached colors for all 16 gray levels
    color_cache: ColorCache,
    /// Text color (needed for CharacterStyle::set_text_color)
    text_color: Rgb565,
    /// Underline color and thickness
    underline_color: DecorationColor<Rgb565>,
    /// Strikethrough color and thickness
    strikethrough_color: DecorationColor<Rgb565>,
}

impl<'a> Gray4TextStyle<'a> {
    /// Create a new Gray4TextStyle with the given font and text color
    /// Defaults to blending with black background
    pub fn new(font: &'a Gray4Font, text_color: Rgb565) -> Self {
        let color_cache = ColorCache::new(text_color, None);
        
        Self {
            font,
            color_cache,
            text_color,
            underline_color: DecorationColor::None,
            strikethrough_color: DecorationColor::None,
        }
    }
    
    /// Create a new Gray4TextStyle with a specific background color for alpha blending
    pub fn with_background(font: &'a Gray4Font, text_color: Rgb565, background_color: Rgb565) -> Self {
        let color_cache = ColorCache::new(text_color, Some(background_color));
        
        Self {
            font,
            color_cache,
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
        let pixels = self.font.glyph_pixels(glyph)
            .map(|Pixel(point, gray)| {
                let color = self.color_cache.colors[gray.luma() as usize];
                Pixel(
                    Point::new(draw_x + point.x, draw_y + point.y),
                    color
                )
            });
        
        // Draw all pixels in one call
        target.draw_iter(pixels)
    }
}

impl<'a> TextRenderer for Gray4TextStyle<'a> {
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
        
        // Draw decorations
        let _text_width = x - position.x;
        
        // Underline
        match self.underline_color {
            DecorationColor::None => {},
            DecorationColor::TextColor => {
                let underline_y = y + self.font.baseline as i32 + 2;
                Line::new(
                    Point::new(position.x, underline_y),
                    Point::new(x, underline_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(self.text_color, 1))
                .draw(target)?;
            },
            DecorationColor::Custom(color) => {
                let underline_y = y + self.font.baseline as i32 + 2;
                Line::new(
                    Point::new(position.x, underline_y),
                    Point::new(x, underline_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(color, 1))
                .draw(target)?;
            },
        }
        
        // Strikethrough
        match self.strikethrough_color {
            DecorationColor::None => {},
            DecorationColor::TextColor => {
                let strikethrough_y = y + (self.font.line_height as i32) / 2;
                Line::new(
                    Point::new(position.x, strikethrough_y),
                    Point::new(x, strikethrough_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(self.text_color, 1))
                .draw(target)?;
            },
            DecorationColor::Custom(color) => {
                let strikethrough_y = y + (self.font.line_height as i32) / 2;
                Line::new(
                    Point::new(position.x, strikethrough_y),
                    Point::new(x, strikethrough_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(color, 1))
                .draw(target)?;
            },
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

impl<'a> CharacterStyle for Gray4TextStyle<'a> {
    type Color = Rgb565;
    
    fn set_text_color(&mut self, text_color: Option<Self::Color>) {
        if let Some(color) = text_color {
            self.text_color = color;
            // Recreate color cache with new text color (defaults to black background)
            self.color_cache = ColorCache::new(self.text_color, None);
        }
    }
    
    fn set_background_color(&mut self, background_color: Option<Self::Color>) {
        // Recreate color cache with new background color
        self.color_cache = ColorCache::new(self.text_color, background_color);
    }
    
    fn set_underline_color(&mut self, underline_color: DecorationColor<Self::Color>) {
        self.underline_color = underline_color;
    }
    
    fn set_strikethrough_color(&mut self, strikethrough_color: DecorationColor<Self::Color>) {
        self.strikethrough_color = strikethrough_color;
    }
}