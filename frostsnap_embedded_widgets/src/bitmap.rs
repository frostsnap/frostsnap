use alloc::vec::Vec;
use bincode::{Decode, Encode};
use embedded_graphics::{
    image::{Image, ImageRaw},
    pixelcolor::BinaryColor,
    prelude::*,
};

#[derive(Debug, Clone, Copy, Encode, Decode)]
enum BitmapColor {
    Binary = 0x00,
}

#[derive(Debug, Encode, Decode)]
pub struct Bitmap {
    color: BitmapColor,
    width: u32,
    pub bytes: Vec<u8>,
}

impl Bitmap {
    /// Load a bitmap from binary data
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        bincode::decode_from_slice(data, bincode::config::standard())
            .map(|(bitmap, _)| bitmap)
    }
    
    /// Get the width of the bitmap
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get the height of the bitmap (calculated from bytes and width)
    pub fn height(&self) -> u32 {
        // Each byte contains 8 pixels for binary bitmaps
        let total_pixels = self.bytes.len() * 8;
        total_pixels as u32 / self.width
    }
    
    /// Draw the bitmap at a specific position
    pub fn draw<D>(&self, target: &mut D, position: Point) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let raw_image = ImageRaw::<BinaryColor>::new(&self.bytes, self.width);
        let image = Image::new(&raw_image, position);
        image.draw(target)
    }
}