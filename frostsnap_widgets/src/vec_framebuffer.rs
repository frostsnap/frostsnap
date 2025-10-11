//! Vec-based framebuffer implementation for dynamic sizing
//! This is a translation of embedded-graphics framebuffer that uses Vec instead of const generics

use alloc::vec::Vec;
use core::convert::Infallible;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawData, RawU1, RawU16, RawU2, RawU4, RawU8},
        BinaryColor, Gray2, Gray4, Gray8, PixelColor, Rgb565,
    },
    primitives::Rectangle,
    Pixel,
};

/// A framebuffer with dynamic dimensions using Vec storage
#[derive(Clone, PartialEq)]
pub struct VecFramebuffer<C>
where
    C: PixelColor,
{
    /// Raw pixel data stored in a Vec
    pub data: Vec<u8>,
    /// Width of the framebuffer in pixels
    pub width: usize,
    /// Height of the framebuffer in pixels  
    pub height: usize,
    _phantom: core::marker::PhantomData<C>,
}

impl<C> VecFramebuffer<C>
where
    C: PixelColor,
{
    /// Calculate the required buffer size in bytes for the given dimensions
    #[inline]
    pub fn buffer_size(width: usize, height: usize) -> usize {
        let bits_per_pixel = C::Raw::BITS_PER_PIXEL;
        let total_bits = width * height * bits_per_pixel;
        // Round up to nearest byte
        total_bits.div_ceil(8)
    }

    /// Create a new framebuffer with the given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        let buffer_size = Self::buffer_size(width, height);
        Self {
            data: vec![0; buffer_size],
            width,
            height,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Get the raw data as a slice
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the raw data as a mutable slice
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

// Implement OriginDimensions trait
impl<C> OriginDimensions for VecFramebuffer<C>
where
    C: PixelColor,
{
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

/// Iterator over colors in a VecFramebuffer
pub struct ContiguousPixels<'a, C>
where
    C: PixelColor,
    RawDataSlice<'a, C::Raw, LittleEndian>: IntoIterator<Item = C::Raw>,
{
    iter: <RawDataSlice<'a, C::Raw, LittleEndian> as IntoIterator>::IntoIter,
}

impl<'a, C> Iterator for ContiguousPixels<'a, C>
where
    C: PixelColor + From<C::Raw>,
    RawDataSlice<'a, C::Raw, LittleEndian>: IntoIterator<Item = C::Raw>,
{
    type Item = C;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|raw| raw.into())
    }
}

impl<C> VecFramebuffer<C>
where
    C: PixelColor,
    for<'a> RawDataSlice<'a, C::Raw, LittleEndian>: IntoIterator<Item = C::Raw>,
{
    /// Returns an iterator over all colors in the framebuffer
    pub fn contiguous_pixels(&self) -> ContiguousPixels<'_, C> {
        ContiguousPixels {
            iter: RawDataSlice::<C::Raw, LittleEndian>::new(&self.data).into_iter(),
        }
    }
}

// Implementation for Rgb565 (16-bit color)
impl VecFramebuffer<Rgb565> {
    /// Set a pixel at the given point
    #[inline]
    pub fn set_pixel(&mut self, point: Point, color: Rgb565) {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index * 2;

                let raw: RawU16 = color.into();
                let value = raw.into_inner();

                // Little endian: low byte first
                self.data[byte_index] = (value & 0xFF) as u8;
                self.data[byte_index + 1] = ((value >> 8) & 0xFF) as u8;
            }
        }
    }

    /// Get a pixel at the given point
    #[inline]
    pub fn get_pixel(&self, point: Point) -> Option<Rgb565> {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index * 2;

                // Little endian: low byte first
                let value =
                    self.data[byte_index] as u16 | ((self.data[byte_index + 1] as u16) << 8);

                let raw = RawU16::new(value);
                return Some(raw.into());
            }
        }
        None
    }

    /// Clear the framebuffer with the given color
    pub fn clear(&mut self, color: Rgb565) {
        let raw: RawU16 = color.into();
        let value = raw.into_inner();
        let byte0 = (value & 0xFF) as u8;
        let byte1 = ((value >> 8) & 0xFF) as u8;

        for chunk in self.data.chunks_exact_mut(2) {
            chunk[0] = byte0;
            chunk[1] = byte1;
        }
    }

    /// Fill a rectangular region with a color
    pub fn fill_rect(&mut self, rect: Rectangle, color: Rgb565) {
        let start_x = rect.top_left.x.max(0) as usize;
        let start_y = rect.top_left.y.max(0) as usize;
        let end_x = ((rect.top_left.x + rect.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((rect.top_left.y + rect.size.height as i32).min(self.height as i32)) as usize;

        let raw: RawU16 = color.into();
        let value = raw.into_inner();
        let byte0 = (value & 0xFF) as u8;
        let byte1 = ((value >> 8) & 0xFF) as u8;

        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index * 2;

                self.data[byte_index] = byte0;
                self.data[byte_index + 1] = byte1;
            }
        }
    }
}

impl DrawTarget for VecFramebuffer<Rgb565> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point, color);
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let start_x = area.top_left.x.max(0) as usize;
        let start_y = area.top_left.y.max(0) as usize;
        let end_x = ((area.top_left.x + area.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((area.top_left.y + area.size.height as i32).min(self.height as i32)) as usize;

        let mut colors_iter = colors.into_iter();

        for y in start_y..end_y {
            for x in start_x..end_x {
                if let Some(color) = colors_iter.next() {
                    let pixel_index = y * self.width + x;
                    let byte_index = pixel_index * 2;

                    let raw: RawU16 = color.into();
                    let value = raw.into_inner();

                    self.data[byte_index] = (value & 0xFF) as u8;
                    self.data[byte_index + 1] = ((value >> 8) & 0xFF) as u8;
                } else {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_rect(*area, color);
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        <VecFramebuffer<Rgb565>>::clear(self, color);
        Ok(())
    }
}

// Implementation for Gray8 (8-bit grayscale)
impl VecFramebuffer<Gray8> {
    /// Set a pixel at the given point
    #[inline]
    pub fn set_pixel(&mut self, point: Point, color: Gray8) {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let raw: RawU8 = color.into();
                self.data[pixel_index] = raw.into_inner();
            }
        }
    }

    /// Get a pixel at the given point
    #[inline]
    pub fn get_pixel(&self, point: Point) -> Option<Gray8> {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let raw = RawU8::new(self.data[pixel_index]);
                return Some(raw.into());
            }
        }
        None
    }

    /// Clear the framebuffer with the given color
    pub fn clear(&mut self, color: Gray8) {
        let raw: RawU8 = color.into();
        let value = raw.into_inner();
        self.data.fill(value);
    }

    /// Fill a rectangular region with a color
    pub fn fill_rect(&mut self, rect: Rectangle, color: Gray8) {
        let start_x = rect.top_left.x.max(0) as usize;
        let start_y = rect.top_left.y.max(0) as usize;
        let end_x = ((rect.top_left.x + rect.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((rect.top_left.y + rect.size.height as i32).min(self.height as i32)) as usize;

        let raw: RawU8 = color.into();
        let value = raw.into_inner();

        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel_index = y * self.width + x;
                self.data[pixel_index] = value;
            }
        }
    }
}

impl DrawTarget for VecFramebuffer<Gray8> {
    type Color = Gray8;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point, color);
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let start_x = area.top_left.x.max(0) as usize;
        let start_y = area.top_left.y.max(0) as usize;
        let end_x = ((area.top_left.x + area.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((area.top_left.y + area.size.height as i32).min(self.height as i32)) as usize;

        let mut colors_iter = colors.into_iter();

        for y in start_y..end_y {
            let row_start = y * self.width + start_x;
            let row_end = y * self.width + end_x;

            for pixel_index in row_start..row_end {
                if let Some(color) = colors_iter.next() {
                    let raw: RawU8 = color.into();
                    self.data[pixel_index] = raw.into_inner();
                } else {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let start_x = area.top_left.x.max(0) as usize;
        let start_y = area.top_left.y.max(0) as usize;
        let end_x = ((area.top_left.x + area.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((area.top_left.y + area.size.height as i32).min(self.height as i32)) as usize;

        let raw: RawU8 = color.into();
        let value = raw.into_inner();

        // For Gray8, we can optimize by filling entire rows at once
        for y in start_y..end_y {
            let row_start = y * self.width + start_x;
            let row_end = y * self.width + end_x;
            self.data[row_start..row_end].fill(value);
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        <VecFramebuffer<Gray8>>::clear(self, color);
        Ok(())
    }
}

// Implementation for Gray4 (4-bit grayscale)
impl VecFramebuffer<Gray4> {
    /// Set a pixel at the given point
    #[inline]
    pub fn set_pixel(&mut self, point: Point, color: Gray4) {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 2;
                let nibble_shift = if pixel_index.is_multiple_of(2) { 4 } else { 0 };

                let raw: RawU4 = color.into();
                let value = raw.into_inner();

                // Clear the nibble
                let mask = !(0xF << nibble_shift);
                self.data[byte_index] &= mask;

                // Set the new value
                self.data[byte_index] |= (value & 0xF) << nibble_shift;
            }
        }
    }

    /// Get a pixel at the given point
    #[inline]
    pub fn get_pixel(&self, point: Point) -> Option<Gray4> {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 2;
                let nibble_shift = if pixel_index.is_multiple_of(2) { 4 } else { 0 };

                let value = (self.data[byte_index] >> nibble_shift) & 0xF;
                let raw = RawU4::new(value);
                return Some(raw.into());
            }
        }
        None
    }

    /// Clear the framebuffer with the given color
    pub fn clear(&mut self, color: Gray4) {
        let raw: RawU4 = color.into();
        let value = raw.into_inner();

        // If both nibbles are the same, we can use fill
        let fill_byte = (value << 4) | value;
        self.data.fill(fill_byte);

        // Handle odd width case where the last pixel might not be paired
        if !(self.width * self.height).is_multiple_of(2) {
            let last_pixel_index = self.width * self.height - 1;
            let byte_index = last_pixel_index / 2;
            let nibble_shift = if last_pixel_index.is_multiple_of(2) {
                4
            } else {
                0
            };
            let mask = !(0xF << nibble_shift);
            self.data[byte_index] &= mask;
            self.data[byte_index] |= (value & 0xF) << nibble_shift;
        }
    }

    /// Fill a rectangular region with a color
    pub fn fill_rect(&mut self, rect: Rectangle, color: Gray4) {
        let start_x = rect.top_left.x.max(0) as usize;
        let start_y = rect.top_left.y.max(0) as usize;
        let end_x = ((rect.top_left.x + rect.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((rect.top_left.y + rect.size.height as i32).min(self.height as i32)) as usize;

        let raw: RawU4 = color.into();
        let value = raw.into_inner();

        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 2;
                let nibble_shift = if pixel_index.is_multiple_of(2) { 4 } else { 0 };

                let mask = !(0xF << nibble_shift);
                self.data[byte_index] &= mask;
                self.data[byte_index] |= (value & 0xF) << nibble_shift;
            }
        }
    }
}

impl DrawTarget for VecFramebuffer<Gray4> {
    type Color = Gray4;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point, color);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_rect(*area, color);
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        <VecFramebuffer<Gray4>>::clear(self, color);
        Ok(())
    }
}

// Implementation for Gray2 (2-bit grayscale)
impl VecFramebuffer<Gray2> {
    /// Set a pixel at the given point
    #[inline]
    pub fn set_pixel(&mut self, point: Point, color: Gray2) {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 4;
                let bit_index = 6 - ((pixel_index % 4) * 2); // 2 bits per pixel

                let raw: RawU2 = color.into();
                let value = raw.into_inner();

                // Clear the 2 bits
                let mask = !(0b11 << bit_index);
                self.data[byte_index] &= mask;

                // Set the new value
                self.data[byte_index] |= (value & 0b11) << bit_index;
            }
        }
    }

    /// Get a pixel at the given point
    #[inline]
    pub fn get_pixel(&self, point: Point) -> Option<Gray2> {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 4;
                let bit_index = 6 - ((pixel_index % 4) * 2);

                let value = (self.data[byte_index] >> bit_index) & 0b11;
                let raw = RawU2::new(value);
                return Some(raw.into());
            }
        }
        None
    }

    /// Clear the framebuffer with the given color
    pub fn clear(&mut self, color: Gray2) {
        let raw: RawU2 = color.into();
        let value = raw.into_inner();

        // Create a byte with all 4 pixels set to the same value
        let fill_byte = (value << 6) | (value << 4) | (value << 2) | value;
        self.data.fill(fill_byte);

        // Handle any remaining pixels if width*height is not divisible by 4
        let total_pixels = self.width * self.height;
        let remainder = total_pixels % 4;
        if remainder != 0 {
            let last_byte_index = total_pixels / 4;
            let mut last_byte = 0u8;
            for i in 0..remainder {
                let bit_index = 6 - (i * 2);
                last_byte |= (value & 0b11) << bit_index;
            }
            self.data[last_byte_index] = last_byte;
        }
    }

    /// Fill a rectangular region with a color
    pub fn fill_rect(&mut self, rect: Rectangle, color: Gray2) {
        let start_x = rect.top_left.x.max(0) as usize;
        let start_y = rect.top_left.y.max(0) as usize;
        let end_x = ((rect.top_left.x + rect.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((rect.top_left.y + rect.size.height as i32).min(self.height as i32)) as usize;

        let raw: RawU2 = color.into();
        let value = raw.into_inner();

        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 4;
                let bit_index = 6 - ((pixel_index % 4) * 2);

                let mask = !(0b11 << bit_index);
                self.data[byte_index] &= mask;
                self.data[byte_index] |= (value & 0b11) << bit_index;
            }
        }
    }
}

impl DrawTarget for VecFramebuffer<Gray2> {
    type Color = Gray2;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point, color);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_rect(*area, color);
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        <VecFramebuffer<Gray2>>::clear(self, color);
        Ok(())
    }
}

// Implementation for BinaryColor (1-bit monochrome)
impl VecFramebuffer<BinaryColor> {
    /// Convert pixel coordinates to byte index and bit position
    #[inline]
    fn pixel_to_bit_index(&self, x: usize, y: usize) -> (usize, u8) {
        let pixel_index = y * self.width + x;
        let byte_index = pixel_index >> 3; // Equivalent to / 8
        let bit_index = 7 - ((pixel_index & 7) as u8); // Equivalent to % 8, MSB first
        (byte_index, bit_index)
    }

    /// Set a pixel at the given point
    #[inline]
    pub fn set_pixel(&mut self, point: Point, color: BinaryColor) {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let (byte_index, bit_index) = self.pixel_to_bit_index(x, y);

                match color {
                    BinaryColor::On => self.data[byte_index] |= 1 << bit_index,
                    BinaryColor::Off => self.data[byte_index] &= !(1 << bit_index),
                }
            }
        }
    }

    /// Get a pixel at the given point
    #[inline]
    pub fn get_pixel(&self, point: Point) -> Option<BinaryColor> {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let (byte_index, bit_index) = self.pixel_to_bit_index(x, y);

                let value = (self.data[byte_index] >> bit_index) & 1;
                let raw = RawU1::new(value);
                return Some(raw.into());
            }
        }
        None
    }

    /// Clear the framebuffer with the given color
    pub fn clear(&mut self, color: BinaryColor) {
        let raw: RawU1 = color.into();
        let fill_byte = if raw.into_inner() != 0 { 0xFF } else { 0x00 };
        self.data.fill(fill_byte);
    }

    /// Iterate over all pixels that are set to On
    ///
    /// NOTE: This could potentially be optimized by iterating byte-by-byte and skipping
    /// empty bytes (0x00), but this would require division operations to convert byte
    /// indices back to pixel coordinates. Benchmarking should be done first to verify
    /// that such an optimization actually improves performance for typical use cases.
    pub fn on_pixels(&self) -> impl Iterator<Item = Point> + '_ {
        let width = self.width;
        let height = self.height;

        (0..height).flat_map(move |y| {
            (0..width).filter_map(move |x| {
                let (byte_index, bit_index) = self.pixel_to_bit_index(x, y);
                let bit = ((self.data[byte_index] >> bit_index) & 1) != 0;
                if bit {
                    Some(Point::new(x as i32, y as i32))
                } else {
                    None
                }
            })
        })
    }

    /// Fill a rectangular region with a color
    pub fn fill_rect(&mut self, rect: Rectangle, color: BinaryColor) {
        let start_x = rect.top_left.x.max(0) as usize;
        let start_y = rect.top_left.y.max(0) as usize;
        let end_x = ((rect.top_left.x + rect.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((rect.top_left.y + rect.size.height as i32).min(self.height as i32)) as usize;

        let raw: RawU1 = color.into();
        let bit_value = raw.into_inner();

        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel_index = y * self.width + x;
                let byte_index = pixel_index / 8;
                let bit_index = 7 - (pixel_index % 8);

                if bit_value != 0 {
                    self.data[byte_index] |= 1 << bit_index;
                } else {
                    self.data[byte_index] &= !(1 << bit_index);
                }
            }
        }
    }
}

impl DrawTarget for VecFramebuffer<BinaryColor> {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point, color);
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_rect(*area, color);
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        <VecFramebuffer<BinaryColor>>::clear(self, color);
        Ok(())
    }
}

use embedded_graphics::{
    image::{ImageDrawable, ImageRaw},
    Drawable,
};

// Implement ImageDrawable for VecFramebuffer
impl<C> ImageDrawable for VecFramebuffer<C>
where
    C: PixelColor + From<<C as PixelColor>::Raw>,
    for<'a> ImageRaw<'a, C, LittleEndian>: ImageDrawable<Color = C>,
{
    type Color = C;

    fn draw<D>(&self, target: &mut D) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Create an ImageRaw from our data and draw it
        let raw_image = ImageRaw::<C, LittleEndian>::new(&self.data, self.width as u32);
        embedded_graphics::image::Image::new(&raw_image, Point::zero()).draw(target)
    }

    fn draw_sub_image<D>(
        &self,
        target: &mut D,
        area: &embedded_graphics::primitives::Rectangle,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // For sub-image drawing, we can use the same approach
        // The area parameter would allow us to draw only a portion if needed
        let raw_image = ImageRaw::<C, LittleEndian>::new(&self.data, self.width as u32);
        embedded_graphics::image::Image::new(&raw_image, area.top_left).draw(target)
    }
}

// Performance notes:
// 1. The clear() method is optimized for Gray8 and BinaryColor using fill()
// 2. Batch pixel setting could use SIMD instructions where available
// 3. Consider alignment for better cache performance
// 4. The bit manipulation for sub-byte pixels could potentially be optimized with lookup tables
// 5. Consider unsafe variants for hot paths where bounds checking is redundant
// 6. The fill_rect could be optimized to copy bytes directly for aligned regions
// 7. For 16/24/32-bit pixels, we could use slice::copy_from_slice for bulk operations
