use alloc::{boxed::Box, string::ToString};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU1},
        BinaryColor, Rgb565,
    },
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use u8g2_fonts::U8g2TextStyle;

use super::{key_touch::KeyTouch, FONT_LARGE};
use crate::{bip39_words::ValidLetters, graphics::palette::COLORS};

// Constants for framebuffer and keyboard dimensions
const FRAMEBUFFER_WIDTH: u32 = 240;
const TOTAL_COLS: usize = 4;
const KEY_WIDTH: u32 = FRAMEBUFFER_WIDTH / TOTAL_COLS as u32;
const KEY_HEIGHT: u32 = 50;
const TOTAL_ROWS: usize = 7;
const FRAMEBUFFER_HEIGHT: u32 = TOTAL_ROWS as u32 * KEY_HEIGHT;
const KEYBOARD_COLOR: Rgb565 = Rgb565::new(25, 52, 26);

const KEYBOARD_KEYS: [[char; TOTAL_COLS]; TOTAL_ROWS] = [
    ['A', 'B', 'C', 'D'],
    ['E', 'F', 'G', 'H'],
    ['I', 'J', 'K', 'L'],
    ['M', 'N', 'O', 'P'],
    ['Q', 'R', 'S', 'T'],
    ['U', 'V', 'W', 'X'],
    ['Y', 'Z', ' ', ' '],
];

type Fb = Framebuffer<
    BinaryColor,
    RawU1,
    LittleEndian,
    { FRAMEBUFFER_WIDTH as usize },
    { FRAMEBUFFER_HEIGHT as usize },
    { buffer_size::<BinaryColor>(FRAMEBUFFER_WIDTH as usize, FRAMEBUFFER_HEIGHT as usize) },
>;

/// A custom iterator that only recalculates which key-cell
/// we're in at the start of each KEY_WIDTH span.
struct KeySpanIterator<I> {
    raw_pixels: I,
    key_colors: [Rgb565; TOTAL_ROWS * TOTAL_COLS],
    row_offset: usize,

    width: usize,
    height: usize,
    key_width: usize,
    key_height: usize,

    current_color: Rgb565,
    next_boundary: usize,
    idx: usize,
}

impl<I> KeySpanIterator<I>
where
    I: Iterator<Item = RawU1>,
{
    fn new(
        raw_pixels: I,
        key_colors: [Rgb565; TOTAL_ROWS * TOTAL_COLS],
        row_offset: usize,
    ) -> Self {
        let width = FRAMEBUFFER_WIDTH as usize;
        let height = FRAMEBUFFER_HEIGHT as usize;
        let key_width = KEY_WIDTH as usize;
        let key_height = KEY_HEIGHT as usize;

        let mut iter = KeySpanIterator {
            raw_pixels,
            key_colors,
            row_offset,

            width,
            height,
            key_width,
            key_height,

            current_color: KEYBOARD_COLOR,
            next_boundary: 0,
            idx: 0,
        };

        iter.update_color_and_boundary();
        iter
    }

    /// Recompute `current_color` for the cell containing `idx`,
    /// and set `next_boundary` to the index at which that cell ends.
    fn update_color_and_boundary(&mut self) {
        let x = self.idx % self.width;
        let raw_y = (self.idx / self.width + self.row_offset) % self.height;

        let row = raw_y / self.key_height;
        let col = x / self.key_width;
        let color = if row < TOTAL_ROWS && col < TOTAL_COLS {
            self.key_colors[row * TOTAL_COLS + col]
        } else {
            KEYBOARD_COLOR
        };
        self.current_color = color;

        // compute the scanline boundary for this cell
        let row_start = (self.idx / self.width) * self.width;
        let next_x_boundary = ((x / self.key_width + 1) * self.key_width).min(self.width);
        self.next_boundary = row_start + next_x_boundary;
    }
}

impl<I> Iterator for KeySpanIterator<I>
where
    I: Iterator<Item = RawU1>,
{
    type Item = Rgb565;

    fn next(&mut self) -> Option<Rgb565> {
        let bit = self.raw_pixels.next()?;
        let out = if bit == RawU1::new(1) {
            self.current_color
        } else {
            COLORS.background
        };

        self.idx += 1;
        if self.idx >= self.next_boundary {
            self.update_color_and_boundary();
        }
        Some(out)
    }
}

#[derive(Debug)]
pub struct AlphabeticKeyboard {
    scroll_position: i32,                          // Current scroll offset
    framebuffer: Box<Fb>,                          // Boxed framebuffer
    needs_redraw: bool,                            // Flag to trigger redraw
    keyspace: Rectangle,                           // Area where keys are drawn
    key_colors: [Rgb565; TOTAL_ROWS * TOTAL_COLS], // Direct color lookup for each key
}

impl AlphabeticKeyboard {
    pub fn new() -> Self {
        let keyspace = Rectangle::new(
            Point::zero(),
            Size {
                width: FRAMEBUFFER_WIDTH,
                height: FRAMEBUFFER_HEIGHT,
            },
        );

        let mut keyboard = Self {
            framebuffer: Box::new(Fb::new()),
            scroll_position: 0,
            needs_redraw: true,
            keyspace,
            key_colors: [KEYBOARD_COLOR; TOTAL_ROWS * TOTAL_COLS],
        };

        keyboard.render_full_keyboard();
        keyboard
    }

    pub fn scroll(&mut self, amount: i32) {
        let height = FRAMEBUFFER_HEIGHT as i32;
        self.scroll_position = (self.scroll_position - amount).rem_euclid(height);
        self.needs_redraw = true;
    }

    fn render_full_keyboard(&mut self) {
        let mut keyspace_fb = self.framebuffer.cropped(&self.keyspace);
        let character_style = U8g2TextStyle::new(FONT_LARGE, BinaryColor::On);

        for (row_index, row) in KEYBOARD_KEYS.iter().enumerate() {
            for (col_index, &key) in row.iter().enumerate() {
                let x = col_index as i32 * KEY_WIDTH as i32;
                let y = row_index as i32 * KEY_HEIGHT as i32;
                let position = Point::new(x + (KEY_WIDTH as i32 / 2), y + (KEY_HEIGHT as i32 / 2));

                let _ = Text::with_text_style(
                    &key.to_string(),
                    position,
                    character_style.clone(),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(&mut keyspace_fb);
            }
        }
    }

    pub fn draw(&mut self, target: &mut impl DrawTarget<Color = Rgb565>) {
        if !self.needs_redraw {
            return;
        }

        let size = target.bounding_box().size;
        let width = FRAMEBUFFER_WIDTH as usize;
        let frame_height = FRAMEBUFFER_HEIGHT as usize;

        let pixels_visible = (size.height as usize) * width;
        let start = ((self.scroll_position as usize) * width) % (frame_height * width);
        let until_end = frame_height * width - start;

        let make_iter = |skip: usize, take: usize| {
            let row_offset = skip / width;
            let raw = RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
                .into_iter()
                .skip(skip)
                .take(take);
            KeySpanIterator::new(raw, self.key_colors, row_offset)
        };

        if pixels_visible <= until_end {
            let span = make_iter(start, pixels_visible);
            let _ = target.fill_contiguous(&Rectangle::new(Point::zero(), size), span);
        } else {
            let first_rows = until_end / width;
            let first_size = Size::new(FRAMEBUFFER_WIDTH, first_rows as u32);
            let span1 = make_iter(start, until_end);
            let _ = target.fill_contiguous(&Rectangle::new(Point::zero(), first_size), span1);

            let second_rows = size.height - first_rows as u32;
            let span2 = make_iter(0, pixels_visible - until_end);
            let _ = target.fill_contiguous(
                &Rectangle::new(
                    Point::new(0, first_rows as i32),
                    Size::new(FRAMEBUFFER_WIDTH, second_rows),
                ),
                span2,
            );
        }

        self.needs_redraw = false;
    }

    pub fn handle_touch(&self, mut point: Point) -> Option<KeyTouch> {
        if !self.keyspace.contains(point) {
            return None;
        }
        point -= self.keyspace.top_left;
        let fb_y = (point.y + self.scroll_position).rem_euclid(FRAMEBUFFER_HEIGHT as i32);
        let col = (point.x / KEY_WIDTH as i32) as usize;
        let row = (fb_y / KEY_HEIGHT as i32) as usize;

        if col < TOTAL_COLS && row < TOTAL_ROWS {
            let idx = row * TOTAL_COLS + col;
            // If this key is currently greyed out (disabled), ignore the touch
            if self.key_colors[idx] != KEYBOARD_COLOR {
                return None;
            }
            let key = KEYBOARD_KEYS[row][col];
            let pos_in_fb = row as i32 * KEY_HEIGHT as i32;
            let screen_y = if pos_in_fb >= self.scroll_position {
                pos_in_fb - self.scroll_position
            } else {
                (FRAMEBUFFER_HEIGHT as i32 - self.scroll_position) + pos_in_fb
            };
            let rect = Rectangle::new(
                Point::new(
                    col as i32 * KEY_WIDTH as i32,
                    screen_y + self.keyspace.top_left.y,
                ),
                Size::new(KEY_WIDTH, KEY_HEIGHT),
            );
            return Some(KeyTouch::new(key, rect));
        }
        None
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        self.scroll(delta);
    }

    pub fn set_valid_keys(&mut self, valid_letters: ValidLetters) {
        let grey = Rgb565::new(10, 20, 10);

        for (idx, &key) in KEYBOARD_KEYS.iter().flatten().enumerate() {
            self.key_colors[idx] = if key == ' ' {
                KEYBOARD_COLOR
            } else if valid_letters.is_valid(key) {
                KEYBOARD_COLOR
            } else {
                grey
            };
        }

        self.needs_redraw = true;
    }
}
