use alloc::{boxed::Box, string::ToString};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU1},
        BinaryColor, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use u8g2_fonts::U8g2TextStyle;

use crate::graphics::palette::COLORS;

use super::{key_touch::KeyTouch, FONT_LARGE};

// Constants for the framebuffer and keyboard dimensions
const FRAMEBUFFER_WIDTH: u32 = 240;
const KEY_WIDTH: u32 = 60; // 240 / 4
const KEY_HEIGHT: u32 = 50;
const TOTAL_ROWS: usize = 7;
const BAR_HEIGHT: u32 = 2;
const FRAMEBUFFER_HEIGHT: u32 = (TOTAL_ROWS as u32 * KEY_HEIGHT) + 2 * BAR_HEIGHT;
const KEYBOARD_COLOR: Rgb565 = Rgb565::new(25, 52, 26);

const KEYBOARD_KEYS: [[char; 4]; 7] = [
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

#[derive(Debug)]
pub struct AlphabeticKeyboard {
    scroll_position: i32, // Current scroll offset
    framebuffer: Box<Fb>, // Boxed framebuffer
    needs_redraw: bool,   // Flag to trigger redraw
    keyspace: Rectangle,  // Area where keys are drawn
}

impl AlphabeticKeyboard {
    /// Create a new AlphabeticKeyboard instance.
    pub fn new() -> Self {
        let keyspace = Rectangle::new(
            Point::new(0, BAR_HEIGHT as i32),
            Size {
                width: FRAMEBUFFER_WIDTH,
                height: FRAMEBUFFER_HEIGHT - BAR_HEIGHT,
            },
        );

        let mut keyboard = Self {
            framebuffer: Box::new(Fb::new()),
            scroll_position: 0,
            needs_redraw: true,
            keyspace,
        };

        keyboard.render_full_keyboard();
        keyboard
    }

    /// Scroll the keyboard by the given amount, wrapping around infinitely.
    pub fn scroll(&mut self, amount: i32) {
        let height = FRAMEBUFFER_HEIGHT as i32;
        self.scroll_position = (self.scroll_position - amount).rem_euclid(height);
        self.needs_redraw = true;
    }

    /// Render the static full keyboard into the framebuffer.
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

        let bar_style = PrimitiveStyleBuilder::new()
            .fill_color(BinaryColor::Off)
            .build();
        let bar_size = Size::new(FRAMEBUFFER_WIDTH, BAR_HEIGHT);

        // Top bar
        let _ = Rectangle::new(Point::zero(), bar_size)
            .into_styled(bar_style)
            .draw(self.framebuffer.as_mut());
        // Bottom bar
        let _ = Rectangle::new(
            Point::new(0, FRAMEBUFFER_HEIGHT as i32 - BAR_HEIGHT as i32),
            bar_size,
        )
        .into_styled(bar_style)
        .draw(self.framebuffer.as_mut());
    }

    /// Draw the visible portion of the keyboard, handling wrapping.
    pub fn draw(&mut self, target: &mut impl DrawTarget<Color = Rgb565>) {
        if !self.needs_redraw {
            return;
        }

        let size = target.bounding_box().size;
        let visible_rows = size.height as usize;
        let width = FRAMEBUFFER_WIDTH as usize;
        let frame_height = FRAMEBUFFER_HEIGHT as usize;

        // Start index in the framebuffer data
        let start = (self.scroll_position as usize * width) % (frame_height * width);
        let pixels_visible = visible_rows * width;
        let until_end = frame_height * width - start;

        let map_color = |r: RawU1| match BinaryColor::from(r) {
            BinaryColor::On => KEYBOARD_COLOR,
            BinaryColor::Off => COLORS.background,
        };

        if pixels_visible <= until_end {
            // One contiguous block
            let _ = target.fill_contiguous(
                &Rectangle::new(Point::zero(), size),
                RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
                    .into_iter()
                    .skip(start)
                    .take(pixels_visible)
                    .map(map_color),
            );
        } else {
            // Two blocks: bottom of framebuffer then top
            let first_rows = until_end / width;
            let second_rows = visible_rows - first_rows;

            // First segment
            let _ = target.fill_contiguous(
                &Rectangle::new(
                    Point::zero(),
                    Size::new(FRAMEBUFFER_WIDTH, first_rows as u32),
                ),
                RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
                    .into_iter()
                    .skip(start)
                    .take(until_end)
                    .map(map_color),
            );

            // Second segment
            let _ = target.fill_contiguous(
                &Rectangle::new(
                    Point::new(0, first_rows as i32),
                    Size::new(FRAMEBUFFER_WIDTH, second_rows as u32),
                ),
                RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
                    .into_iter()
                    .take(second_rows * width)
                    .map(map_color),
            );
        }

        self.needs_redraw = false;
    }

    /// Handle a touch, mapping to a key and returning its rectangle in screen coordinates.
    pub fn handle_touch(&self, mut point: Point) -> Option<KeyTouch> {
        if !self.keyspace.contains(point) {
            return None;
        }

        // Convert to keyspace coordinates
        point -= self.keyspace.top_left;

        // y in framebuffer
        let fb_y = (point.y + self.scroll_position).rem_euclid(FRAMEBUFFER_HEIGHT as i32);
        let col = (point.x / KEY_WIDTH as i32) as usize;
        let row = (fb_y / KEY_HEIGHT as i32) as usize;

        if col < 4 && row < TOTAL_ROWS {
            let key = KEYBOARD_KEYS[row][col];

            // Compute on-screen y
            let pos_in_fb = (row as i32) * KEY_HEIGHT as i32;
            let screen_y = if pos_in_fb >= self.scroll_position {
                pos_in_fb - self.scroll_position
            } else {
                (FRAMEBUFFER_HEIGHT as i32 - self.scroll_position) + pos_in_fb
            };
            let x = (col as i32) * KEY_WIDTH as i32;
            let y = screen_y + self.keyspace.top_left.y;

            let rect = Rectangle::new(Point::new(x, y), Size::new(KEY_WIDTH, KEY_HEIGHT));
            return Some(KeyTouch::new(key, rect));
        }
        None
    }

    /// Handle vertical drag gestures to scroll.
    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        self.scroll(delta);
    }
}
