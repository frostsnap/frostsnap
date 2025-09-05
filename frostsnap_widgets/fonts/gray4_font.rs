/// Gray4 font format - 4-bit anti-aliased fonts with 16 levels of gray

/// Glyph info - stores position in packed data array
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    pub character: char,
    pub width: u8,
    pub height: u8,
    pub x_offset: i8,  // Bearing X
    pub y_offset: i8,  // Bearing Y  
    pub x_advance: u8, // How far to advance after drawing
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
        self.glyphs.binary_search_by_key(&c, |g| g.character)
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
        
        if pixel_index % 2 == 0 {
            (byte >> 4) & 0x0F  // High nibble
        } else {
            byte & 0x0F         // Low nibble
        }
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