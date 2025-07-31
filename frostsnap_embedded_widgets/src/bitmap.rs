use alloc::{vec, vec::Vec};
use bincode::{Decode, Encode};
use embedded_graphics::{
    image::{Image, ImageRaw},
    pixelcolor::BinaryColor,
    prelude::*,
};

#[derive(Debug, Clone, Copy, Encode, Decode)]
enum ImageColor {
    Binary = 0x00,
}

#[derive(Debug, Encode, Decode)]
pub struct EncodedImage {
    color: ImageColor,
    width: u32,
    pub bytes: Vec<u8>,
}

impl EncodedImage {
    /// Load an encoded image from binary data
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        bincode::decode_from_slice(data, bincode::config::standard())
            .map(|(image, _)| image)
    }
    
    /// Get the width of the image
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get the height of the image (calculated from bytes and width)
    pub fn height(&self) -> u32 {
        // Each byte contains 8 pixels for binary images
        let total_pixels = self.bytes.len() * 8;
        total_pixels as u32 / self.width
    }
}

/// A pure bitmap for tracking binary pixels
#[derive(Clone)]
pub struct Bitmap {
    pub width: u32,
    pub bytes: Vec<u8>,
}

impl Bitmap {
    /// Create a new bitmap with all pixels set to the default color
    pub fn new(size: Size, default: BinaryColor) -> Self {
        let bytes_per_row = (size.width + 7) / 8; // Round up to nearest byte
        let total_bytes = bytes_per_row * size.height;
        let fill_byte = if default == BinaryColor::On { 0xFF } else { 0x00 };
        Self {
            width: size.width,
            bytes: vec![fill_byte; total_bytes as usize],
        }
    }
    
    /// Get the width of the bitmap
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get the height of the bitmap
    pub fn height(&self) -> u32 {
        let bytes_per_row = (self.width + 7) / 8;
        (self.bytes.len() as u32) / bytes_per_row
    }
    
    /// Set a pixel at the given position
    pub fn set_pixel(&mut self, x: u32, y: u32, color: BinaryColor) {
        if x >= self.width {
            return;
        }
        
        let bytes_per_row = (self.width + 7) / 8;
        let byte_index = (y * bytes_per_row + x / 8) as usize;
        let bit_index = 7 - (x % 8); // MSB first
        
        if byte_index < self.bytes.len() {
            if color == BinaryColor::On {
                self.bytes[byte_index] |= 1 << bit_index;
            } else {
                self.bytes[byte_index] &= !(1 << bit_index);
            }
        }
    }
    
    /// Clear all pixels in the bitmap
    pub fn clear(&mut self) {
        self.bytes.fill(0);
    }
    
    /// Get a pixel at the given position
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<BinaryColor> {
        if x >= self.width {
            return None;
        }
        
        let bytes_per_row = (self.width + 7) / 8;
        let byte_index = (y * bytes_per_row + x / 8) as usize;
        let bit_index = 7 - (x % 8); // MSB first
        
        if byte_index < self.bytes.len() {
            let bit = (self.bytes[byte_index] >> bit_index) & 1;
            Some(if bit == 1 { BinaryColor::On } else { BinaryColor::Off })
        } else {
            None
        }
    }
    
    /// Iterate over all pixels that are set to On
    pub fn on_pixels(&self) -> impl Iterator<Item = Point> + '_ {
        let width = self.width;
        let bytes_per_row = (width + 7) / 8;
        
        self.bytes.iter()
            .enumerate()
            .flat_map(move |(byte_idx, &byte)| {
                let row = (byte_idx as u32) / bytes_per_row;
                let col_offset = (byte_idx as u32 % bytes_per_row) * 8;
                
                (0..8u8).filter_map(move |bit| {
                    if byte & (0x80 >> bit) != 0 {
                        let x = col_offset + bit as u32;
                        if x < width {
                            return Some(Point::new(x as i32, row as i32));
                        }
                    }
                    None
                })
            })
    }
}

impl From<EncodedImage> for Bitmap {
    fn from(encoded: EncodedImage) -> Self {
        Self {
            width: encoded.width,
            bytes: encoded.bytes,
        }
    }
}

/// A widget wrapper for Bitmap that implements the Widget trait
pub struct BitmapWidget {
    bitmap: Bitmap,
    needs_redraw: bool,
}

impl BitmapWidget {
    pub fn new(bitmap: Bitmap) -> Self {
        Self {
            bitmap,
            needs_redraw: true,
        }
    }
    
    pub fn width(&self) -> u32 {
        self.bitmap.width()
    }
    
    pub fn height(&self) -> u32 {
        self.bitmap.height()
    }
}

impl crate::Widget for BitmapWidget {
    type Color = BinaryColor;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.needs_redraw {
            let raw_image = ImageRaw::<BinaryColor>::new(&self.bitmap.bytes, self.bitmap.width);
            let image = Image::new(&raw_image, Point::zero());
            image.draw(target)?;
            self.needs_redraw = false;
        }
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}
    
    fn size_hint(&self) -> Option<Size> {
        Some(Size::new(self.bitmap.width(), self.bitmap.height()))
    }
    
    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}
