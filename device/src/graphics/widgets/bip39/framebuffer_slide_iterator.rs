use embedded_graphics::pixelcolor::raw::RawU2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlideDirection {
    Left,
    Right,
}

/// Iterator that yields pixels from two framebuffers to create a sliding animation effect.
///
/// This iterator takes pixels from two framebuffers based on the specified direction
/// and pixel counts, yielding them row by row. It's designed to work with Gray2
/// pixel format where each byte contains 4 pixels (2 bits per pixel).
pub struct FramebufferSlideIterator<'a> {
    // Raw data from both framebuffers
    fb1_data: &'a [u8],
    fb2_data: &'a [u8],
    // Number of pixels to take from each framebuffer
    fb1_pixels: usize,
    fb2_pixels: usize,
    // Width of the framebuffers
    width: usize,
    // Current position
    current_row: usize,
    current_pixel_in_row: usize,
    // Direction of slide
    direction: SlideDirection,
}

impl<'a> FramebufferSlideIterator<'a> {
    /// Creates a new sliding animation iterator.
    ///
    /// # Arguments
    /// * `fb1_data` - Raw data from the first framebuffer (from .data() call)
    /// * `fb2_data` - Raw data from the second framebuffer (from .data() call)
    /// * `fb1_pixels` - Number of pixels to take from the first framebuffer
    /// * `fb2_pixels` - Number of pixels to take from the second framebuffer  
    /// * `direction` - Direction of the slide animation
    ///
    /// # Panics
    /// Panics if fb1_pixels + fb2_pixels != width
    pub fn new(
        fb1_data: &'a [u8],
        fb2_data: &'a [u8],
        fb1_pixels: usize,
        fb2_pixels: usize,
        width: usize,
        direction: SlideDirection,
    ) -> Self {
        assert_eq!(
            fb1_pixels + fb2_pixels,
            width,
            "Sum of pixels from both framebuffers must equal width"
        );

        Self {
            fb1_data,
            fb2_data,
            fb1_pixels,
            fb2_pixels,
            width,
            current_row: 0,
            current_pixel_in_row: 0,
            direction,
        }
    }

    /// Gets a pixel from the framebuffer data at the specified position.
    ///
    /// For Gray2 format, each byte contains 4 pixels (2 bits each).
    /// Pixels are packed from MSB to LSB within each byte.
    fn get_pixel_from_data(data: &[u8], pixel_index: usize) -> RawU2 {
        let byte_index = pixel_index / 4;
        let pixel_in_byte = pixel_index % 4;

        if byte_index >= data.len() {
            return RawU2::new(0);
        }

        let byte = data[byte_index];
        // Extract 2 bits for the pixel (MSB first)
        let shift = 6 - (pixel_in_byte * 2);
        let pixel_value = (byte >> shift) & 0b11;

        RawU2::new(pixel_value)
    }
}

impl<'a> Iterator for FramebufferSlideIterator<'a> {
    type Item = RawU2;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we've processed all pixels
        if self.current_row * self.width + self.current_pixel_in_row >= self.fb1_data.len() * 4 {
            return None;
        }

        let pixel = match self.direction {
            SlideDirection::Left => {
                // When sliding left, next content (fb2) appears from right
                // Layout: [current content][next content]
                if self.current_pixel_in_row < self.fb1_pixels {
                    // Take from fb1, but skip fb2_pixels from the start
                    let fb1_pixel_index =
                        self.current_row * self.width + self.current_pixel_in_row + self.fb2_pixels;
                    Self::get_pixel_from_data(self.fb1_data, fb1_pixel_index)
                } else {
                    // Take from fb2, starting from the beginning
                    let fb2_pixel_index = self.current_row * self.width
                        + (self.current_pixel_in_row - self.fb1_pixels);
                    Self::get_pixel_from_data(self.fb2_data, fb2_pixel_index)
                }
            }
            SlideDirection::Right => {
                // When sliding right, next content (fb2) appears from left
                // Layout: [next content][current content]
                if self.current_pixel_in_row < self.fb2_pixels {
                    // Take from fb2, starting from the end minus fb2_pixels
                    let fb2_pixel_index = self.current_row * self.width
                        + (self.width - self.fb2_pixels)
                        + self.current_pixel_in_row;
                    Self::get_pixel_from_data(self.fb2_data, fb2_pixel_index)
                } else {
                    // Take from fb1, starting from the beginning
                    let fb1_pixel_index = self.current_row * self.width
                        + (self.current_pixel_in_row - self.fb2_pixels);
                    Self::get_pixel_from_data(self.fb1_data, fb1_pixel_index)
                }
            }
        };

        // Move to next pixel
        self.current_pixel_in_row += 1;
        if self.current_pixel_in_row >= self.width {
            self.current_pixel_in_row = 0;
            self.current_row += 1;
        }

        Some(pixel)
    }
}

/// Creates an iterator that slides between two framebuffers.
///
/// This is a convenience function that creates a `FramebufferSlideIterator`.
/// 
/// # Arguments
/// * `current_data` - The current framebuffer data
/// * `next_data` - The next framebuffer data  
/// * `width` - The width of the framebuffers
/// * `next_pixels` - How many pixels to take from the next framebuffer (0 to width)
/// * `direction` - The slide direction
pub fn slide_framebuffers<'a>(
    current_data: &'a [u8],
    next_data: &'a [u8],
    width: usize,
    next_pixels: usize,
    direction: SlideDirection,
) -> impl Iterator<Item = RawU2> + 'a {
    let current_pixels = width.saturating_sub(next_pixels);
    
    // fb1 is always current, fb2 is always next
    // The iterator handles the direction logic
    FramebufferSlideIterator::new(current_data, next_data, current_pixels, next_pixels, width, direction)
}
