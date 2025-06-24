use super::Widget;
use crate::{bitmap::Bitmap, palette::PALETTE, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
    Pixel,
};

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-64x64.bin");

/// A welcome screen widget showing the Frostsnap logo and getting started text
pub struct Welcome {
    drawn: bool,
}

impl Welcome {
    pub fn new() -> Self {
        Self {
            drawn: false,
        }
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Welcome {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: Instant,
    ) -> Result<(), D::Error> {
        if self.drawn {
            return Ok(());
        }

        let bounds = target.bounding_box();
        let center_x = bounds.size.width / 2;
        
        // Clear background
        target.clear(PALETTE.background)?;
        
        // Load and draw logo
        if let Ok(bitmap) = Bitmap::from_bytes(LOGO_DATA) {
            // Create a temporary binary target for the bitmap
            let logo_width = bitmap.width();
            let logo_height = bitmap.height();
            
            // Center the logo horizontally, place it in upper portion
            let logo_x = (center_x as i32) - (logo_width as i32 / 2);
            let logo_y = 40; // Position logo with some top margin
            
            // Draw bitmap pixel by pixel, converting BinaryColor to Rgb565
            // We'll draw it row by row
            for y in 0..logo_height {
                for x in 0..logo_width {
                    let byte_index = ((y * logo_width + x) / 8) as usize;
                    let bit_index = ((y * logo_width + x) % 8) as usize;
                    
                    if byte_index < bitmap.bytes.len() {
                        let byte = bitmap.bytes[byte_index];
                        let bit = (byte >> (7 - bit_index)) & 1;
                        
                        if bit == 1 {
                            // Draw primary color pixel for the logo
                            let pixel = Pixel(
                                Point::new(logo_x + x as i32, logo_y + y as i32),
                                PALETTE.primary, // Use primary color for the logo
                            );
                            target.draw_iter(core::iter::once(pixel))?;
                        }
                    }
                }
            }
        }
        
        // Draw welcome text using embedded_graphics directly for Rgb565
        let text_y = 140; // Below the logo
        
        use embedded_graphics::text::{Alignment, Baseline, Text as EgText, TextStyleBuilder};
        use u8g2_fonts::U8g2TextStyle;
        use crate::FONT_SMALL;
        
        let character_style = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Top)
            .build();
            
        // First line of text - use secondary/gray color
        let text_style = U8g2TextStyle::new(FONT_SMALL, PALETTE.on_surface_variant);
        EgText::with_text_style(
            "Get started with your",
            Point::new(center_x as i32, text_y),
            text_style.clone(),
            character_style,
        )
        .draw(target)?;
        
        // Second line
        EgText::with_text_style(
            "Frostsnap at",
            Point::new(center_x as i32, text_y + 20),
            text_style,
            character_style,
        )
        .draw(target)?;
        
        // URL line with deeper blue link color
        let url_style = U8g2TextStyle::new(FONT_SMALL, PALETTE.primary_container);
        EgText::with_text_style(
            "frostsnap.com/start",
            Point::new(center_x as i32, text_y + 40),
            url_style,
            character_style,
        )
        .draw(target)?;
        
        self.drawn = true;
        Ok(())
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // Welcome screen doesn't respond to drags
    }

    fn size_hint(&self) -> Option<Size> {
        // Take full screen
        None
    }
}