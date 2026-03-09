//! Vec-based framebuffer implementation for dynamic sizing
//! This is a translation of embedded-graphics framebuffer that uses Vec instead of const generics

use alloc::vec::Vec;
use core::convert::Infallible;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    image::{ImageDrawable, ImageRaw},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawData, RawU1, RawU16, RawU2, RawU4, RawU8},
        BinaryColor, Gray2, Gray4, Gray8, PixelColor, Rgb565,
    },
    primitives::Rectangle,
    Drawable, Pixel,
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

impl<C: PixelColor> Default for VecFramebuffer<C> {
    fn default() -> Self {
        Self::new(0, 0)
    }
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

/// Trait for pixel types that can be packed into a byte buffer.
/// Enables generic `DrawTarget` and pixel operations on `VecFramebuffer<C>`.
pub trait FramebufferColor: PixelColor + Into<Self::Raw> + From<Self::Raw> {
    fn write_pixel(data: &mut [u8], pixel_index: usize, color: Self);
    fn read_pixel(data: &[u8], pixel_index: usize) -> Self;
    fn fill_data(data: &mut [u8], total_pixels: usize, color: Self);
}

impl FramebufferColor for Rgb565 {
    #[inline]
    fn write_pixel(data: &mut [u8], pixel_index: usize, color: Self) {
        let byte_index = pixel_index * 2;
        let raw: RawU16 = color.into();
        let value = raw.into_inner();
        data[byte_index] = (value & 0xFF) as u8;
        data[byte_index + 1] = ((value >> 8) & 0xFF) as u8;
    }

    #[inline]
    fn read_pixel(data: &[u8], pixel_index: usize) -> Self {
        let byte_index = pixel_index * 2;
        let value = data[byte_index] as u16 | ((data[byte_index + 1] as u16) << 8);
        RawU16::new(value).into()
    }

    fn fill_data(data: &mut [u8], _total_pixels: usize, color: Self) {
        let raw: RawU16 = color.into();
        let value = raw.into_inner();
        let byte0 = (value & 0xFF) as u8;
        let byte1 = ((value >> 8) & 0xFF) as u8;
        for chunk in data.chunks_exact_mut(2) {
            chunk[0] = byte0;
            chunk[1] = byte1;
        }
    }
}

impl FramebufferColor for Gray8 {
    #[inline]
    fn write_pixel(data: &mut [u8], pixel_index: usize, color: Self) {
        let raw: RawU8 = color.into();
        data[pixel_index] = raw.into_inner();
    }

    #[inline]
    fn read_pixel(data: &[u8], pixel_index: usize) -> Self {
        RawU8::new(data[pixel_index]).into()
    }

    fn fill_data(data: &mut [u8], _total_pixels: usize, color: Self) {
        let raw: RawU8 = color.into();
        data.fill(raw.into_inner());
    }
}

impl FramebufferColor for Gray4 {
    #[inline]
    fn write_pixel(data: &mut [u8], pixel_index: usize, color: Self) {
        let byte_index = pixel_index / 2;
        let nibble_shift = if pixel_index.is_multiple_of(2) { 4 } else { 0 };
        let raw: RawU4 = color.into();
        let value = raw.into_inner();
        let mask = !(0xF << nibble_shift);
        data[byte_index] &= mask;
        data[byte_index] |= (value & 0xF) << nibble_shift;
    }

    #[inline]
    fn read_pixel(data: &[u8], pixel_index: usize) -> Self {
        let byte_index = pixel_index / 2;
        let nibble_shift = if pixel_index.is_multiple_of(2) { 4 } else { 0 };
        let value = (data[byte_index] >> nibble_shift) & 0xF;
        RawU4::new(value).into()
    }

    fn fill_data(data: &mut [u8], total_pixels: usize, color: Self) {
        let raw: RawU4 = color.into();
        let value = raw.into_inner();
        let fill_byte = (value << 4) | value;
        data.fill(fill_byte);

        if !total_pixels.is_multiple_of(2) {
            let last_pixel_index = total_pixels - 1;
            let byte_index = last_pixel_index / 2;
            let nibble_shift = if last_pixel_index.is_multiple_of(2) {
                4
            } else {
                0
            };
            let mask = !(0xF << nibble_shift);
            data[byte_index] &= mask;
            data[byte_index] |= (value & 0xF) << nibble_shift;
        }
    }
}

impl FramebufferColor for Gray2 {
    #[inline]
    fn write_pixel(data: &mut [u8], pixel_index: usize, color: Self) {
        let byte_index = pixel_index / 4;
        let bit_index = 6 - ((pixel_index % 4) * 2);
        let raw: RawU2 = color.into();
        let value = raw.into_inner();
        let mask = !(0b11 << bit_index);
        data[byte_index] &= mask;
        data[byte_index] |= (value & 0b11) << bit_index;
    }

    #[inline]
    fn read_pixel(data: &[u8], pixel_index: usize) -> Self {
        let byte_index = pixel_index / 4;
        let bit_index = 6 - ((pixel_index % 4) * 2);
        let value = (data[byte_index] >> bit_index) & 0b11;
        RawU2::new(value).into()
    }

    fn fill_data(data: &mut [u8], total_pixels: usize, color: Self) {
        let raw: RawU2 = color.into();
        let value = raw.into_inner();
        let fill_byte = (value << 6) | (value << 4) | (value << 2) | value;
        data.fill(fill_byte);

        let remainder = total_pixels % 4;
        if remainder != 0 {
            let last_byte_index = total_pixels / 4;
            let mut last_byte = 0u8;
            for i in 0..remainder {
                let bit_index = 6 - (i * 2);
                last_byte |= (value & 0b11) << bit_index;
            }
            data[last_byte_index] = last_byte;
        }
    }
}

impl FramebufferColor for BinaryColor {
    #[inline]
    fn write_pixel(data: &mut [u8], pixel_index: usize, color: Self) {
        let byte_index = pixel_index >> 3;
        let bit_index = 7 - (pixel_index & 7);
        match color {
            BinaryColor::On => data[byte_index] |= 1 << bit_index,
            BinaryColor::Off => data[byte_index] &= !(1 << bit_index),
        }
    }

    #[inline]
    fn read_pixel(data: &[u8], pixel_index: usize) -> Self {
        let byte_index = pixel_index >> 3;
        let bit_index = 7 - (pixel_index & 7);
        let value = (data[byte_index] >> bit_index) & 1;
        RawU1::new(value).into()
    }

    fn fill_data(data: &mut [u8], _total_pixels: usize, color: Self) {
        let raw: RawU1 = color.into();
        let fill_byte = if raw.into_inner() != 0 { 0xFF } else { 0x00 };
        data.fill(fill_byte);
    }
}

// Generic implementations for VecFramebuffer<C: FramebufferColor>

impl<C: FramebufferColor> VecFramebuffer<C> {
    #[inline]
    pub fn set_pixel(&mut self, point: Point, color: C) {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                C::write_pixel(&mut self.data, pixel_index, color);
            }
        }
    }

    #[inline]
    pub fn get_pixel(&self, point: Point) -> Option<C> {
        if let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) {
            if x < self.width && y < self.height {
                let pixel_index = y * self.width + x;
                return Some(C::read_pixel(&self.data, pixel_index));
            }
        }
        None
    }

    pub fn clear(&mut self, color: C) {
        C::fill_data(&mut self.data, self.width * self.height, color);
    }

    pub fn fill_rect(&mut self, rect: Rectangle, color: C) {
        let start_x = rect.top_left.x.max(0) as usize;
        let start_y = rect.top_left.y.max(0) as usize;
        let end_x = ((rect.top_left.x + rect.size.width as i32).min(self.width as i32)) as usize;
        let end_y = ((rect.top_left.y + rect.size.height as i32).min(self.height as i32)) as usize;

        for y in start_y..end_y {
            for x in start_x..end_x {
                let pixel_index = y * self.width + x;
                C::write_pixel(&mut self.data, pixel_index, color);
            }
        }
    }
}

impl<C: FramebufferColor> DrawTarget for VecFramebuffer<C> {
    type Color = C;
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
        VecFramebuffer::clear(self, color);
        Ok(())
    }
}

impl<C: PixelColor> OriginDimensions for &mut VecFramebuffer<C> {
    fn size(&self) -> Size {
        (**self).size()
    }
}

impl<C: FramebufferColor> DrawTarget for &mut VecFramebuffer<C> {
    type Color = C;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        (**self).draw_iter(pixels)
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        (**self).fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        VecFramebuffer::clear(self, color);
        Ok(())
    }
}

// BinaryColor-specific method
impl VecFramebuffer<BinaryColor> {
    /// Iterate over all pixels that are set to On
    pub fn on_pixels(&self) -> impl Iterator<Item = Point> + '_ {
        let width = self.width;
        let height = self.height;

        (0..height).flat_map(move |y| {
            (0..width).filter_map(move |x| {
                let pixel_index = y * width + x;
                let byte_index = pixel_index >> 3;
                let bit_index = 7 - (pixel_index & 7);
                let bit = ((self.data[byte_index] >> bit_index) & 1) != 0;
                if bit {
                    Some(Point::new(x as i32, y as i32))
                } else {
                    None
                }
            })
        })
    }
}

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
        let raw_image = ImageRaw::<C, LittleEndian>::new(&self.data, self.width as u32);
        embedded_graphics::image::Image::new(&raw_image, area.top_left).draw(target)
    }
}
