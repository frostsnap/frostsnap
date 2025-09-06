use crate::vec_framebuffer::VecFramebuffer;
use alloc::vec::Vec;
use bincode::{Decode, Encode};
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};

#[derive(Debug, Clone, Copy, Encode, Decode)]
enum ImageColor {
    Binary = 0x00,
}

#[derive(Debug, Encode, Decode)]
pub struct EncodedImage {
    color: ImageColor,
    width: u32,
    pub bytes: Vec<u8>, // TODO: use Cow<'static, [u8]>
}

impl EncodedImage {
    /// Load an encoded image from binary data
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        bincode::decode_from_slice(data, bincode::config::standard()).map(|(image, _)| image)
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

impl From<EncodedImage> for VecFramebuffer<BinaryColor> {
    fn from(encoded: EncodedImage) -> Self {
        // Convert from row-by-row with padding to sequential storage
        let width = encoded.width as usize;
        let height = encoded.height() as usize;
        let mut framebuffer = VecFramebuffer::new(width, height);

        let bytes_per_row = width.div_ceil(8);

        for y in 0..height {
            for x in 0..width {
                let byte_index = y * bytes_per_row + x / 8;
                let bit_index = 7 - (x % 8);

                if byte_index < encoded.bytes.len() {
                    let bit = (encoded.bytes[byte_index] >> bit_index) & 1;
                    let color = if bit == 1 {
                        BinaryColor::On
                    } else {
                        BinaryColor::Off
                    };
                    VecFramebuffer::<BinaryColor>::set_pixel(
                        &mut framebuffer,
                        Point::new(x as i32, y as i32),
                        color,
                    );
                }
            }
        }

        framebuffer
    }
}

impl From<&EncodedImage> for VecFramebuffer<BinaryColor> {
    fn from(encoded: &EncodedImage) -> Self {
        // Convert from row-by-row with padding to sequential storage
        let width = encoded.width as usize;
        let height = encoded.height() as usize;
        let mut framebuffer = VecFramebuffer::new(width, height);

        let bytes_per_row = width.div_ceil(8);

        for y in 0..height {
            for x in 0..width {
                let byte_index = y * bytes_per_row + x / 8;
                let bit_index = 7 - (x % 8);

                if byte_index < encoded.bytes.len() {
                    let bit = (encoded.bytes[byte_index] >> bit_index) & 1;
                    let color = if bit == 1 {
                        BinaryColor::On
                    } else {
                        BinaryColor::Off
                    };
                    VecFramebuffer::<BinaryColor>::set_pixel(
                        &mut framebuffer,
                        Point::new(x as i32, y as i32),
                        color,
                    );
                }
            }
        }

        framebuffer
    }
}
