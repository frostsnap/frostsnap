use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::ImageDrawable,
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::{BinaryColor, Rgb565},
    prelude::*,
    text::Text as EgText,
    Pixel,
};

use crate::{vec_framebuffer::VecFramebuffer, DynWidget, Sizing, SuperDrawTarget, Widget};

const CHAR_W: u32 = 6;
const CHAR_H: u32 = 10;
const LINE_SPACING: u32 = 2;
const LINE_H: u32 = CHAR_H + LINE_SPACING;

pub struct ScrollableDebugText {
    fb: VecFramebuffer<BinaryColor>,
    scroll_y: i32,
    visible_size: Size,
    needs_redraw: bool,
}

impl ScrollableDebugText {
    pub fn new(text: &str, screen_width: u32) -> Self {
        let chars_per_line = (screen_width / CHAR_W) as usize;

        let mut lines: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
        for raw_line in text.split('\n') {
            if raw_line.is_empty() {
                lines.push("");
                continue;
            }
            let mut start = 0;
            while start < raw_line.len() {
                let end = (start + chars_per_line).min(raw_line.len());
                lines.push(&raw_line[start..end]);
                start = end;
            }
        }

        let fb_height = (lines.len() as u32 * LINE_H).max(1);
        let mut fb = VecFramebuffer::<BinaryColor>::new(screen_width as usize, fb_height as usize);

        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        for (i, line) in lines.iter().enumerate() {
            let y = (i as u32 * LINE_H + CHAR_H) as i32;
            let _ = EgText::new(line, Point::new(1, y), style).draw(&mut fb);
        }

        Self {
            fb,
            scroll_y: 0,
            visible_size: Size::zero(),
            needs_redraw: true,
        }
    }

    fn max_scroll(&self) -> i32 {
        (self.fb.height as i32 - self.visible_size.height as i32).max(0)
    }
}

impl DynWidget for ScrollableDebugText {
    fn set_constraints(&mut self, max_size: Size) {
        self.visible_size = max_size;
    }

    fn sizing(&self) -> Sizing {
        self.visible_size.into()
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        if delta != 0 {
            self.scroll_y = (self.scroll_y - delta).clamp(0, self.max_scroll());
            self.needs_redraw = true;
        }
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl Widget for ScrollableDebugText {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if !self.needs_redraw {
            return Ok(());
        }
        self.needs_redraw = false;

        let vis_w = self.visible_size.width as usize;
        let vis_h = self.visible_size.height as usize;
        let mut visible_fb = VecFramebuffer::<Rgb565>::new(vis_w, vis_h);

        let fb_w = self.fb.width as i32;
        let fb_h = self.fb.height as i32;

        for dy in 0..vis_h as i32 {
            let src_y = self.scroll_y + dy;
            if src_y < 0 || src_y >= fb_h {
                continue;
            }
            for dx in 0..(vis_w as i32).min(fb_w) {
                if self.fb.get_pixel(Point::new(dx, src_y)) == Some(BinaryColor::On) {
                    visible_fb.draw_iter(core::iter::once(Pixel(
                        Point::new(dx, dy),
                        Rgb565::WHITE,
                    ))).unwrap();
                }
            }
        }

        visible_fb.draw(target)?;

        Ok(())
    }
}
