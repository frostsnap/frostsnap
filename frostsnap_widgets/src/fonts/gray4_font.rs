use embedded_graphics::{pixelcolor::Gray4, prelude::*};

/// Gray4 font format - 4-bit anti-aliased fonts with 16 levels of gray
/// Glyph info - stores position in packed data array
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    pub character: char,
    pub width: u8,
    pub height: u8,
    pub x_offset: i8,     // Bearing X
    pub y_offset: i8,     // Bearing Y
    pub x_advance: u8,    // How far to advance after drawing
    pub data_offset: u32, // Offset into packed pixel data
}

/// Gray4 font - packed format for efficient storage
pub struct Gray4Font {
    pub baseline: u32,
    pub line_height: u32,
    /// Packed pixel data - only contains actual glyph pixels
    /// Still 2 pixels per byte (4 bits each)
    pub packed_data: &'static [u8],
    /// Glyph lookup table
    pub glyphs: &'static [GlyphInfo],
}

impl Gray4Font {
    /// Binary search for a character
    pub fn get_glyph(&self, c: char) -> Option<&GlyphInfo> {
        self.glyphs
            .binary_search_by_key(&c, |g| g.character)
            .ok()
            .map(|idx| &self.glyphs[idx])
    }

    /// Get pixel value at (x,y) within a glyph
    pub fn get_pixel(&self, glyph: &GlyphInfo, x: u32, y: u32) -> u8 {
        if x >= glyph.width as u32 || y >= glyph.height as u32 {
            return 0;
        }

        // Calculate position in packed data
        let pixel_index = y * glyph.width as u32 + x;
        let byte_index = glyph.data_offset + (pixel_index / 2);
        let byte = self.packed_data[byte_index as usize];

        if pixel_index.is_multiple_of(2) {
            (byte >> 4) & 0x0F // High nibble
        } else {
            byte & 0x0F // Low nibble
        }
    }

    /// Get an iterator over all non-transparent pixels in a glyph
    pub fn glyph_pixels<'a>(&'a self, glyph: &'a GlyphInfo) -> GlyphPixelIterator<'a> {
        // Calculate how many bytes this glyph uses
        let total_pixels = glyph.width as usize * glyph.height as usize;
        let total_bytes = total_pixels.div_ceil(2); // Round up for odd number of pixels
        let start = glyph.data_offset as usize;
        let end = (start + total_bytes).min(self.packed_data.len());

        GlyphPixelIterator {
            data: &self.packed_data[start..end],
            width: glyph.width,
            height: glyph.height,
            byte_index: 0,
            pixel_in_byte: 0,
            x: 0,
            y: 0,
        }
    }
}

/// Iterator that yields Pixel<Gray4> for each non-transparent pixel in a glyph
pub struct GlyphPixelIterator<'a> {
    data: &'a [u8],    // Slice of packed pixel data for this glyph
    width: u8,         // Glyph width
    height: u8,        // Glyph height
    byte_index: usize, // Current byte index in data slice
    pixel_in_byte: u8, // 0 or 1 (which pixel within the current byte)
    x: u8,             // Current x position
    y: u8,             // Current y position
}

impl<'a> Iterator for GlyphPixelIterator<'a> {
    type Item = Pixel<Gray4>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.y < self.height {
            // Get current byte from packed data
            let byte = self.data[self.byte_index];

            // Extract the pixel value from the current nibble
            let value = if self.pixel_in_byte == 0 {
                (byte >> 4) & 0x0F // High nibble
            } else {
                byte & 0x0F // Low nibble
            };

            let point = Point::new(self.x as i32, self.y as i32);

            // Move to next pixel position
            self.x += 1;
            if self.x >= self.width {
                self.x = 0;
                self.y += 1;
            }

            // Move to next nibble
            self.pixel_in_byte += 1;
            if self.pixel_in_byte >= 2 {
                self.pixel_in_byte = 0;
                self.byte_index += 1;
            }

            // Return non-transparent pixels
            if value > 0 {
                let gray = Gray4::new(value);
                return Some(Pixel(point, gray));
            }
        }
        None
    }
}

// Example of how much smaller this would be:
// For ASCII (95 chars), assuming average glyph size of 15x20 pixels:
// - Each glyph: 15*20 = 300 pixels = 150 bytes (4 bits per pixel)
// - Total: 95 * 150 = 14,250 bytes (~14 KB)
// - Plus glyph info: 95 * 12 bytes = 1,140 bytes
// - Total: ~15 KB for Gray4 ASCII font (vs 524 KB for atlas!)
//
// For full Unicode (1000 chars):
// - 1000 * 150 = 150,000 bytes for pixels
// - 1000 * 12 = 12,000 bytes for glyph info
// - Total: ~162 KB (vs 524 KB for atlas!)
