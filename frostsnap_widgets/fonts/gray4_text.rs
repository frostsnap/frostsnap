/// Optimized text widget for Gray4 fonts with anti-aliased rendering
use super::gray4_font::{Gray4Font, GlyphInfo};
use crate::{
    super_draw_target::SuperDrawTarget,
    DynWidget, Instant, Sizing, Widget,
};
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor, IntoStorage},
    primitives::{Line, PrimitiveStyle, Primitive, Rectangle},
    text::{Alignment, Baseline},
    Drawable,
};

/// Distance in pixels between the bottom of the text and the underline
const UNDERLINE_DISTANCE: i32 = 2;

/// A text widget optimized for Gray4 fonts with anti-aliased rendering
#[derive(Clone)]
pub struct Gray4Text {
    text: String,
    font: &'static Gray4Font,
    color: Rgb565,
    baseline: Baseline,
    alignment: Alignment,
    underline_color: Option<Rgb565>,
    cached_size: Size,
    drawn: bool,
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
            drawn: false,
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
    
    /// Get the text content
    pub fn text(&self) -> &str {
        &self.text
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
        color_cache: &[Option<Rgb565>; 16],
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
            
            // Process row directly without intermediate Vec allocation
            let mut x = 0u8;
            while x < glyph.width {
                // Get the value at current position
                let value = self.font.get_pixel(glyph, x as u32, y as u32);
                
                if value == 0 {
                    // Skip transparent pixels
                    x += 1;
                    continue;
                }
                
                // Find the run length of same value
                let start_x = x;
                x += 1;
                while x < glyph.width {
                    let next_value = self.font.get_pixel(glyph, x as u32, y as u32);
                    if next_value != value {
                        break;
                    }
                    x += 1;
                }
                
                // Get color from cache (should always be cached at this point)
                let color = color_cache[value as usize].unwrap_or_else(|| {
                    // Fallback in case we missed caching it
                    self.blend_color(value)
                });
                
                let run_length = (x - start_x) as usize;
                
                // Always use fill_contiguous for better batching
                let area = Rectangle::new(
                    Point::new(draw_x + start_x as i32, row_y),
                    Size::new(run_length as u32, 1)
                );
                // Create an iterator of the same color repeated
                let colors = core::iter::repeat(color).take(run_length);
                target.fill_contiguous(&area, colors)?;
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
        // Pre-calculate all 16 possible blended colors for this text
        let mut color_cache = [None; 16];
        for i in 0..16 {
            color_cache[i] = Some(self.blend_color(i as u8));
        }
        
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
                // Use the optimized renderer with the pre-calculated color cache
                self.draw_glyph_optimized(Point::new(x, y), glyph, &color_cache, target)?;
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
    
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag handling needed
    }
    
    fn force_full_redraw(&mut self) {
        self.drawn = false;
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
        if !self.drawn {
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
            
            self.drawn = true;
        }
        
        Ok(())
    }
}