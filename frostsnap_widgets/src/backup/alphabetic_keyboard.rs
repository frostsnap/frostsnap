use crate::{palette::PALETTE, super_draw_target::SuperDrawTarget, Widget, FONT_LARGE};
use alloc::boxed::Box;
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    image::Image,
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU4},
        Gray4, GrayColor, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
};
use embedded_iconoir::{
    prelude::IconoirNewIcon,
    size32px::navigation::{NavArrowLeft, NavArrowRight},
};
use frost_backup::bip39_words::ValidLetters;
use frostsnap_fonts::Gray4Font;

const FRAMEBUFFER_WIDTH: u32 = 240;
const TOTAL_COLS: usize = 4;
const KEY_WIDTH: u32 = FRAMEBUFFER_WIDTH / TOTAL_COLS as u32;
const KEY_HEIGHT: u32 = 50;
const TOTAL_ROWS: usize = 7;
const FRAMEBUFFER_HEIGHT: u32 = TOTAL_ROWS as u32 * KEY_HEIGHT;

type Fb = Framebuffer<
    Gray4,
    RawU4,
    LittleEndian,
    { FRAMEBUFFER_WIDTH as usize },
    { FRAMEBUFFER_HEIGHT as usize },
    { buffer_size::<Gray4>(FRAMEBUFFER_WIDTH as usize, FRAMEBUFFER_HEIGHT as usize) },
>;

/// Draw a single character from a Gray4Font into a Gray4 framebuffer, centered in a cell.
fn draw_char_to_framebuffer(
    fb: &mut Fb,
    font: &'static Gray4Font,
    ch: char,
    cell_x: i32,
    cell_y: i32,
) {
    let glyph = match font.get_glyph(ch) {
        Some(g) => g,
        None => return,
    };

    // Center the glyph in the cell
    let glyph_center_x = cell_x + KEY_WIDTH as i32 / 2;
    let glyph_center_y = cell_y + KEY_HEIGHT as i32 / 2;

    // Position the glyph baseline-centered vertically
    let draw_x = glyph_center_x - glyph.x_advance as i32 / 2 + glyph.x_offset as i32;
    let draw_y = glyph_center_y - font.line_height as i32 / 2 + glyph.y_offset as i32;

    for Pixel(point, gray) in font.glyph_pixels(glyph) {
        let px = draw_x + point.x;
        let py = draw_y + point.y;

        if px >= 0
            && py >= 0
            && (px as u32) < FRAMEBUFFER_WIDTH
            && (py as u32) < FRAMEBUFFER_HEIGHT
        {
            if gray.luma() > 0 {
                let _ = Pixel(Point::new(px, py), gray).draw(fb);
            }
        }
    }
}

#[derive(Debug)]
pub struct AlphabeticKeyboard {
    scroll_position: i32,
    framebuffer: Box<Fb>,
    needs_redraw: bool,
    enabled_keys: ValidLetters,
    visible_height: u32,
    current_word_index: usize,
}

impl Default for AlphabeticKeyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl AlphabeticKeyboard {
    pub fn new() -> Self {
        let mut keyboard = Self {
            framebuffer: Box::new(Fb::new()),
            scroll_position: 0,
            needs_redraw: true,
            enabled_keys: ValidLetters::default(),
            visible_height: 0,
            current_word_index: 0,
        };

        keyboard.render_compact_keyboard();
        keyboard
    }

    pub fn scroll(&mut self, amount: i32) {
        let num_rendered = if self.enabled_keys.count_enabled() == 0 {
            ValidLetters::all_valid().count_enabled()
        } else {
            self.enabled_keys.count_enabled()
        };
        let rows_needed = num_rendered.div_ceil(TOTAL_COLS);
        let keyboard_buffer_height = rows_needed * KEY_HEIGHT as usize;

        let max_scroll = keyboard_buffer_height.saturating_sub(self.visible_height as usize);
        let new_scroll_position = (self.scroll_position - amount).clamp(0, max_scroll as i32);
        self.needs_redraw = self.needs_redraw || new_scroll_position != self.scroll_position;
        self.scroll_position = new_scroll_position;
    }

    pub fn reset_scroll(&mut self) {
        if self.scroll_position != 0 {
            self.scroll_position = 0;
            self.needs_redraw = true;
        }
    }

    /// Render only enabled letters in a compact grid layout.
    fn render_compact_keyboard(&mut self) {
        let _ = self.framebuffer.clear(Gray4::new(0));

        let keys_to_render = if self.enabled_keys.count_enabled() == 0 {
            ValidLetters::all_valid()
        } else {
            self.enabled_keys
        };

        for (idx, c) in keys_to_render.iter_valid().enumerate() {
            let row = idx / TOTAL_COLS;
            let col = idx % TOTAL_COLS;
            let cell_x = col as i32 * KEY_WIDTH as i32;
            let cell_y = row as i32 * KEY_HEIGHT as i32;

            draw_char_to_framebuffer(&mut self.framebuffer, FONT_LARGE, c, cell_x, cell_y);
        }
    }

    pub fn set_valid_keys(&mut self, valid_letters: ValidLetters) {
        self.enabled_keys = valid_letters;
        self.scroll_position = 0;
        self.render_compact_keyboard();
        self.needs_redraw = true;
    }

    pub fn set_current_word_index(&mut self, index: usize) {
        if self.current_word_index != index {
            self.current_word_index = index;
            self.needs_redraw = true;
        }
    }
}

impl crate::DynWidget for AlphabeticKeyboard {
    fn set_constraints(&mut self, max_size: Size) {
        self.visible_height = max_size.height;
    }

    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: FRAMEBUFFER_WIDTH,
            height: self.visible_height,
            ..Default::default()
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        use crate::{Key, KeyTouch};

        if self.enabled_keys.count_enabled() == 0 {
            let screen_width = FRAMEBUFFER_WIDTH;
            let screen_height = self.visible_height;

            if point.x < (screen_width / 2) as i32 && self.current_word_index > 0 {
                let rect =
                    Rectangle::new(Point::new(0, 0), Size::new(screen_width / 2, screen_height));
                return Some(KeyTouch::new(Key::NavBack, rect));
            } else if point.x >= (screen_width / 2) as i32 && self.current_word_index < 24 {
                let rect = Rectangle::new(
                    Point::new((screen_width / 2) as i32, 0),
                    Size::new(screen_width / 2, screen_height),
                );
                return Some(KeyTouch::new(Key::NavForward, rect));
            }
        }

        let col = (point.x / KEY_WIDTH as i32) as usize;
        let row = ((point.y + self.scroll_position) / KEY_HEIGHT as i32) as usize;

        if col < TOTAL_COLS {
            let idx = row * TOTAL_COLS + col;
            if let Some(key) = self.enabled_keys.nth_enabled(idx) {
                let x = col as i32 * KEY_WIDTH as i32;
                let y = row as i32 * KEY_HEIGHT as i32 - self.scroll_position;
                let rect = Rectangle::new(Point::new(x, y), Size::new(KEY_WIDTH, KEY_HEIGHT));
                return Some(KeyTouch::new(Key::Keyboard(key), rect));
            }
        }
        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        self.scroll(delta);
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl Widget for AlphabeticKeyboard {
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

        let bounds = target.bounding_box();

        if self.enabled_keys.count_enabled() == 0 {
            let left_arrow = NavArrowLeft::new(PALETTE.on_background);
            let right_arrow = NavArrowRight::new(PALETTE.on_background);

            let screen_width = bounds.size.width;
            let screen_height = bounds.size.height;
            let icon_size = 32;
            let padding = 10;

            Rectangle::new(Point::zero(), bounds.size)
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.background)
                        .build(),
                )
                .draw(target)?;

            if self.current_word_index > 0 {
                let left_point = Point::new(padding, (screen_height / 2 - icon_size / 2) as i32);
                Image::new(&left_arrow, left_point).draw(target)?;
            }

            if self.current_word_index < 24 {
                let right_point = Point::new(
                    (screen_width - icon_size - padding as u32) as i32,
                    (screen_height / 2 - icon_size / 2) as i32,
                );
                Image::new(&right_arrow, right_point).draw(target)?;
            }
        } else {
            let color_lut = {
                use crate::{ColorInterpolate, Frac};
                let mut lut = [PALETTE.background; 16];
                for i in 1..16u8 {
                    let alpha = Frac::from_ratio(i as u32, 15);
                    lut[i as usize] =
                        PALETTE.background.interpolate(PALETTE.primary_container, alpha);
                }
                lut
            };

            let content_height = ((self.framebuffer.size().height as i32 - self.scroll_position)
                .max(0) as u32)
                .min(bounds.size.height);

            if content_height > 0 {
                let skip_pixels =
                    (self.scroll_position.max(0) as usize) * FRAMEBUFFER_WIDTH as usize;

                let framebuffer_pixels =
                    RawDataSlice::<RawU4, LittleEndian>::new(self.framebuffer.data())
                        .into_iter()
                        .skip(skip_pixels)
                        .take(FRAMEBUFFER_WIDTH as usize * content_height as usize)
                        .map(|r| color_lut[Gray4::from(r).luma() as usize]);

                let padding_pixels = core::iter::repeat_n(
                    PALETTE.background,
                    FRAMEBUFFER_WIDTH as usize * (bounds.size.height - content_height) as usize,
                );

                target.fill_contiguous(
                    &Rectangle::new(Point::zero(), bounds.size),
                    framebuffer_pixels.chain(padding_pixels),
                )?;
            }
        }

        self.needs_redraw = false;
        Ok(())
    }
}
