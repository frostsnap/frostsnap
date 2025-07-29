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