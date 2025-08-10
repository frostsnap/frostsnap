use alloc::boxed::Box;

use embedded_graphics::{
    draw_target::DrawTarget, framebuffer::Framebuffer, iterator::raw::RawDataSlice, pixelcolor::raw::LittleEndian, prelude::*,
};

use crate::Widget;

pub struct Buffered<W: Widget, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> {
    fb: Box<Framebuffer<W::Color, <<W as Widget>::Color as PixelColor>::Raw, LittleEndian,  WIDTH, HEIGHT, BUFFER_SIZE>>,
    pub child: W,
    needs_redraw: bool,
    needs_rebuffer: bool,
}

impl<W: Widget, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> Buffered<W, WIDTH, HEIGHT, BUFFER_SIZE> {
    pub fn new(child: W) -> Self {
        Self {
            fb: Box::new(Framebuffer::new()),
            needs_redraw: true,
            needs_rebuffer: true,
            child,
        }
    }

    pub fn rebuffer(&mut self) {
        self.needs_rebuffer = true;
    }
}

impl<W: Widget, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> crate::DynWidget for Buffered<W, WIDTH, HEIGHT, BUFFER_SIZE>
where W: Widget,
      W::Color: PixelColor + Default + From<<W::Color as PixelColor>::Raw>,
      <W::Color as PixelColor>::Raw: Into<W::Color>,
      Framebuffer<W::Color, <W::Color as PixelColor>::Raw, LittleEndian, WIDTH, HEIGHT, BUFFER_SIZE>: DrawTarget<Color=W::Color>,
      for<'a> RawDataSlice<'a, <W::Color as PixelColor>::Raw, LittleEndian>: IntoIterator<Item=<W::Color as PixelColor>::Raw>,

{
    fn sizing(&self) -> crate::Sizing {
        crate::Sizing { width: 240, height: 280 }
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, _is_release);
    }


    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl<W: Widget, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> Widget for Buffered<W, WIDTH, HEIGHT, BUFFER_SIZE>
where W: Widget,
      W::Color: PixelColor + Default + From<<W::Color as PixelColor>::Raw>,
      <W::Color as PixelColor>::Raw: Into<W::Color>,
      Framebuffer<W::Color, <W::Color as PixelColor>::Raw, LittleEndian, WIDTH, HEIGHT, BUFFER_SIZE>: DrawTarget<Color=W::Color>,
      for<'a> RawDataSlice<'a, <W::Color as PixelColor>::Raw, LittleEndian>: IntoIterator<Item=<W::Color as PixelColor>::Raw>,

{
    type Color = W::Color;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.needs_rebuffer {
            let _ = self.child.draw(self.fb.as_mut(), current_time);
            self.needs_rebuffer = false;
        }
        if self.needs_redraw {
            let _ = self.fb.as_image().draw(target);
            self.needs_redraw = false;
        }

        Ok(())
    }

}
