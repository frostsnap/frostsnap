/// Optimized text widget for Gray4 fonts with anti-aliased rendering
use crate::{
    gray4_fonts::gray4_font::{Gray4Font, GlyphInfo},
    super_draw_target::SuperDrawTarget,
    DynWidget, Instant, Sizing, Widget,
};
use alloc::string::String;
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor, IntoStorage},
    primitives::{Line, PrimitiveStyle, Primitive, Rectangle},
    text::{Alignment, Baseline},
    Drawable, Pixel,
};

/// Distance in pixels between the bottom of the text and the underline
const UNDERLINE_DISTANCE: i32 = 2;

/// A text widget optimized for Gray4 fonts with anti-aliased rendering
pub struct Gray4Text {
    text: String,
    font: &'static Gray4Font,
    color: Rgb565,
    baseline: Baseline,
    alignment: Alignment,
    underline_color: Option<Rgb565>,
    cached_size: Size,
}

impl Gray4Text {
    /// Create a new Gray4Text widget
    pub fn new<T: Into<String>>(text: T, font: &'static Gray4Font, color: Rgb565) -> Self {
        let text_string = text.into();
        
        // Calculate size
        let cached_size = Self::measure_text(&text_string, font);
        
        Self {
            text: text_string,
            font,
            color,
            baseline: Baseline::Top,
            alignment: Alignment::Left,
            underline_color: None,
            cached_size,
        }
    }
    
    /// Set the text baseline
    pub fn with_baseline(mut self, baseline: Baseline) -> Self {
        self.baseline = baseline;
        self
    }
    
    /// Set the text alignment
    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
    
    /// Add an underline
    pub fn with_underline(mut self, color: Rgb565) -> Self {
        self.underline_color = Some(color);
        self.cached_size.height += UNDERLINE_DISTANCE as u32 + 1;
        self
    }
    
    /// Measure text dimensions
    fn measure_text(text: &str, font: &Gray4Font) -> Size {
        let mut width = 0u32;
        
        for ch in text.chars() {
            if let Some(glyph) = font.get_glyph(ch) {
                width += glyph.x_advance as u32;
            } else if ch == ' ' {
                width += font.line_height / 4;
            } else {
                width += font.line_height / 3;
            }
        }
        
        Size::new(width, font.line_height)
    }
    
    /// Blend color with alpha for anti-aliasing
    fn blend_color(&self, alpha: u8) -> Rgb565 {
        if alpha == 0 {
            Rgb565::BLACK
        } else if alpha == 15 {
            self.color
        } else {
            // Extract RGB components from RGB565
            let color_raw = self.color.into_storage();
            let r = ((color_raw >> 11) & 0x1F) as u16;
            let g = ((color_raw >> 5) & 0x3F) as u16;
            let b = (color_raw & 0x1F) as u16;
            
            // Apply alpha blending
            let alpha_f = alpha as u16;
            let r_blended = (r * alpha_f / 15) as u16;
            let g_blended = (g * alpha_f / 15) as u16;
            let b_blended = (b * alpha_f / 15) as u16;
            
            // Reconstruct RGB565
            use embedded_graphics::pixelcolor::raw::RawU16;
            Rgb565::from(RawU16::new(
                ((r_blended & 0x1F) << 11) | 
                ((g_blended & 0x3F) << 5) | 
                (b_blended & 0x1F)
            ))
        }
    }
    
    /// Draw a glyph using optimized row-based rendering
    fn draw_glyph_optimized<D>(
        &self,
        position: Point,
        glyph: &GlyphInfo,
        target: &mut SuperDrawTarget<D, Rgb565>,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        // Calculate glyph position with bearing offsets
        let draw_x = position.x + glyph.x_offset as i32;
        let draw_y = position.y + glyph.y_offset as i32;
        
        // Process each row of the glyph
        for y in 0..glyph.height {
            let row_y = draw_y + y as i32;
            
            // Extract the entire row of Gray4 values from packed data
            let mut row_values = Vec::with_capacity(glyph.width as usize);
            for x in 0..glyph.width {
                let value = self.font.get_pixel(glyph, x as u32, y as u32);
                row_values.push(value);
            }
            
            // Render the row by finding runs of same values
            let mut x = 0;
            while x < row_values.len() {
                let start_x = x;
                let value = row_values[x];
                
                // Find run of same value
                while x < row_values.len() && row_values[x] == value {
                    x += 1;
                }
                
                // Draw the run
                if value > 0 {
                    let color = self.blend_color(value);
                    let run_length = x - start_x;
                    
                    // Use fill_contiguous for runs of 3+ pixels
                    if run_length >= 3 {
                        let area = Rectangle::new(
                            Point::new(draw_x + start_x as i32, row_y),
                            Size::new(run_length as u32, 1)
                        );
                        // Create an iterator of the same color repeated
                        let colors = core::iter::repeat(color).take(run_length);
                        target.fill_contiguous(&area, colors)?;
                    } else {
                        // For short runs, just draw pixels
                        for px in start_x..x {
                            target.draw_iter(core::iter::once(Pixel(
                                Point::new(draw_x + px as i32, row_y),
                                color
                            )))?;
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Draw the text string using optimized rendering
    fn draw_string_optimized<D>(
        &self,
        text: &str,
        position: Point,
        baseline: Baseline,
        target: &mut SuperDrawTarget<D, Rgb565>,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
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
                // Use the optimized renderer
                self.draw_glyph_optimized(Point::new(x, y), glyph, target)?;
                x += glyph.x_advance as i32;
            } else if ch == ' ' {
                x += (self.font.line_height / 4) as i32;
            } else {
                x += (self.font.line_height / 3) as i32;
            }
        }
        
        Ok(Point::new(x, position.y))
    }
}

impl DynWidget for Gray4Text {
    fn set_constraints(&mut self, _max_size: Size) {
        // Text has fixed size based on content
    }
    
    fn sizing(&self) -> Sizing {
        self.cached_size.into()
    }
}

impl Widget for Gray4Text {
    type Color = Rgb565;
    
    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Calculate position based on alignment
        let position = match self.alignment {
            Alignment::Left => Point::zero(),
            Alignment::Center => {
                Point::new(-(self.cached_size.width as i32) / 2, 0)
            }
            Alignment::Right => {
                Point::new(-(self.cached_size.width as i32), 0)
            }
        };
        
        // Use the OPTIMIZED rendering path!
        self.draw_string_optimized(&self.text, position, self.baseline, target)?;
        
        // Draw underline if enabled
        if let Some(underline_color) = self.underline_color {
            let bbox_width = self.cached_size.width as i32;
            let underline_y = match self.baseline {
                Baseline::Top => self.font.line_height as i32 + UNDERLINE_DISTANCE,
                Baseline::Bottom => UNDERLINE_DISTANCE,
                Baseline::Middle => (self.font.line_height as i32) / 2 + UNDERLINE_DISTANCE,
                Baseline::Alphabetic => self.font.baseline as i32 + UNDERLINE_DISTANCE,
            };
            
            Line::new(
                Point::new(position.x, underline_y),
                Point::new(position.x + bbox_width, underline_y),
            )
            .into_styled(PrimitiveStyle::with_stroke(underline_color, 1))
            .draw(target)?;
        }
        
        Ok(())
    }
}