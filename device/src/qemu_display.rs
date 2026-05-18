//! ESP32-S3 QEMU virtual RGB panel backend.

use core::{convert::Infallible, marker::PhantomData, ptr};

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::{
        raw::{RawData, RawU16},
        Rgb565,
    },
    primitives::Rectangle,
    Pixel,
};

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 280;

const FRAMEBUFFER_BASE: usize = 0x2000_0000;
const RGB_QEMU_BASE: usize = 0x2100_0000;
const RGB565_BPP: u32 = 16;

#[repr(C)]
struct RgbQemuRegisters {
    version: u32,
    size: u32,
    update_from: u32,
    update_to: u32,
    update_content: *const u16,
    update_st: u32,
    bpp: u32,
}

pub struct VirtualDisplay<'a> {
    dirty: bool,
    _lifetime: PhantomData<&'a mut ()>,
}

impl<'a> VirtualDisplay<'a> {
    pub fn new() -> Self {
        let mut display = Self {
            dirty: true,
            _lifetime: PhantomData,
        };
        display.configure_panel();
        display
    }

    fn configure_panel(&mut self) {
        let regs = Self::regs();
        unsafe {
            ptr::write_volatile(&mut (*regs).size, coord(WIDTH as u16, HEIGHT as u16));
            ptr::write_volatile(&mut (*regs).bpp, RGB565_BPP);
        }
    }

    pub fn take_dirty(&mut self) -> bool {
        core::mem::replace(&mut self.dirty, false)
    }

    pub fn flush_if_dirty(&mut self) {
        if !self.take_dirty() {
            return;
        }

        let regs = Self::regs();
        unsafe {
            ptr::write_volatile(&mut (*regs).update_from, coord(0, 0));
            ptr::write_volatile(&mut (*regs).update_to, coord(WIDTH as u16, HEIGHT as u16));
            ptr::write_volatile(&mut (*regs).update_content, FRAMEBUFFER_BASE as *const u16);
            ptr::write_volatile(&mut (*regs).update_st, 1);
            while ptr::read_volatile(&(*regs).update_st) & 1 == 1 {}
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn regs() -> *mut RgbQemuRegisters {
        RGB_QEMU_BASE as *mut RgbQemuRegisters
    }

    fn pixel_ptr(x: usize, y: usize) -> *mut u16 {
        (FRAMEBUFFER_BASE as *mut u16).wrapping_add(y * WIDTH + x)
    }

    fn set_pixel(&mut self, point: embedded_graphics::prelude::Point, color: Rgb565) {
        let (Ok(x), Ok(y)) = (usize::try_from(point.x), usize::try_from(point.y)) else {
            return;
        };
        if x >= WIDTH || y >= HEIGHT {
            return;
        }

        let raw: RawU16 = color.into();
        unsafe {
            ptr::write_volatile(Self::pixel_ptr(x, y), raw.into_inner());
        }
        self.mark_dirty();
    }
}

impl OriginDimensions for VirtualDisplay<'_> {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for VirtualDisplay<'_> {
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
        let end_x = ((area.top_left.x + area.size.width as i32).min(WIDTH as i32)) as usize;
        let end_y = ((area.top_left.y + area.size.height as i32).min(HEIGHT as i32)) as usize;
        let mut colors = colors.into_iter();

        for y in start_y..end_y {
            for x in start_x..end_x {
                let Some(color) = colors.next() else {
                    self.mark_dirty();
                    return Ok(());
                };
                let raw: RawU16 = color.into();
                unsafe {
                    ptr::write_volatile(Self::pixel_ptr(x, y), raw.into_inner());
                }
            }
        }

        self.mark_dirty();
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let start_x = area.top_left.x.max(0) as usize;
        let start_y = area.top_left.y.max(0) as usize;
        let end_x = ((area.top_left.x + area.size.width as i32).min(WIDTH as i32)) as usize;
        let end_y = ((area.top_left.y + area.size.height as i32).min(HEIGHT as i32)) as usize;
        let raw: RawU16 = color.into();
        let color = raw.into_inner();

        for y in start_y..end_y {
            for x in start_x..end_x {
                unsafe {
                    ptr::write_volatile(Self::pixel_ptr(x, y), color);
                }
            }
        }

        self.mark_dirty();
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_solid(&Rectangle::new(Default::default(), self.size()), color)
    }
}

fn coord(x: u16, y: u16) -> u32 {
    ((x as u32) << 16) | y as u32
}
