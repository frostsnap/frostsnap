use crate::graphics::widgets::{KeyTouch, FONT_LARGE};
use crate::{bip39_words::ValidLetters, graphics::palette::COLORS};
use alloc::{boxed::Box, string::ToString, vec::Vec};
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

#[derive(Debug)]
pub struct AlphabeticKeyboard {
    scroll_position: i32,      // Current scroll offset
    framebuffer: Box<Fb>,      // Boxed framebuffer
    needs_redraw: bool,        // Flag to trigger redraw
    enabled_keys: Vec<char>,   // List of enabled keys in compact layout order
}

impl AlphabeticKeyboard {
    pub fn new() -> Self {
        let mut keyboard = Self {
            framebuffer: Box::new(Fb::new()),
            scroll_position: 0,
            needs_redraw: true,
            enabled_keys: Vec::new(),
        };

        // Initialize with default valid letters
        keyboard.set_valid_keys(ValidLetters::default());
        keyboard
    }

    pub fn scroll(&mut self, amount: i32) {
        // Calculate the effective height based on enabled keys
        let num_enabled = self.enabled_keys.len();
        if num_enabled == 0 {
            return;
        }

        let rows_needed = (num_enabled + TOTAL_COLS - 1) / TOTAL_COLS;
        let effective_height = (rows_needed * KEY_HEIGHT as usize) as i32;

        if effective_height > KEY_HEIGHT as i32 {
            let new_scroll_position = (self.scroll_position - amount)
                .clamp(0, (effective_height - KEY_HEIGHT as i32).max(0));
            self.needs_redraw = new_scroll_position != self.scroll_position;
            self.scroll_position = new_scroll_position;
        }
    }
    
    pub fn reset_scroll(&mut self) {
        if self.scroll_position != 0 {
            self.scroll_position = 0;
            self.needs_redraw = true;
        }
    }

    fn render_compact_keyboard(&mut self) {
        // Clear the framebuffer
        let _ = self.framebuffer.clear(BinaryColor::Off);
        
        let character_style = U8g2TextStyle::new(FONT_LARGE, BinaryColor::On);

        // Draw only enabled keys in compact layout
        for (idx, &key) in self.enabled_keys.iter().enumerate() {
            let row = idx / TOTAL_COLS;
            let col = idx % TOTAL_COLS;
            
            let x = col as i32 * KEY_WIDTH as i32;
            let y = row as i32 * KEY_HEIGHT as i32;
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
            .draw(&mut *self.framebuffer);
        }
    }

    pub fn draw(&mut self, target: &mut impl DrawTarget<Color = Rgb565>) {
        if !self.needs_redraw {
            return;
        }

        let bounds = target.bounding_box();

        // If no enabled keys, clear and return
        if self.enabled_keys.is_empty() {
            let _ = bounds
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.background)
                        .build(),
                )
                .draw(target);
            self.needs_redraw = false;
            return;
        }

        // Calculate how many rows we need for compact layout
        let rows_needed = (self.enabled_keys.len() + TOTAL_COLS - 1) / TOTAL_COLS;
        let compact_height = (rows_needed * KEY_HEIGHT as usize) as u32;
        
        // Calculate the height of content we'll draw from the framebuffer
        let content_height = (compact_height.saturating_sub(self.scroll_position as u32))
            .min(bounds.size.height);
        
        // Calculate pixels to skip based on scroll position
        let skip_pixels = (self.scroll_position.max(0) as usize) * FRAMEBUFFER_WIDTH as usize;

        // Draw the framebuffer content followed by background padding
        let framebuffer_pixels = RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
            .into_iter()
            .skip(skip_pixels)
            .take(FRAMEBUFFER_WIDTH as usize * content_height as usize)
            .map(|r| match BinaryColor::from(r) {
                BinaryColor::Off => COLORS.background,
                BinaryColor::On => KEYBOARD_COLOR,
            });
        
        let padding_pixels = core::iter::repeat(COLORS.background)
            .take(FRAMEBUFFER_WIDTH as usize * (bounds.size.height - content_height) as usize);
        
        let _ = target.fill_contiguous(
            &Rectangle::new(Point::zero(), bounds.size),
            framebuffer_pixels.chain(padding_pixels),
        );

        self.needs_redraw = false;
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        if self.enabled_keys.is_empty() {
            return None;
        }

        // In compact layout, keys are positioned differently
        let col = (point.x / KEY_WIDTH as i32) as usize;
        let row = ((point.y + self.scroll_position) / KEY_HEIGHT as i32) as usize;

        if col < TOTAL_COLS {
            let idx = row * TOTAL_COLS + col;
            if idx < self.enabled_keys.len() {
                let key = self.enabled_keys[idx];

                // Calculate the screen position of the key in compact layout
                let x = col as i32 * KEY_WIDTH as i32;
                let y = row as i32 * KEY_HEIGHT as i32 - self.scroll_position;
                let rect = Rectangle::new(Point::new(x, y), Size::new(KEY_WIDTH, KEY_HEIGHT));

                return Some(KeyTouch::new(key, rect));
            }
        }
        None
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        self.scroll(delta);
    }

    pub fn set_valid_keys(&mut self, valid_letters: ValidLetters) {
        // Clear existing enabled keys
        self.enabled_keys.clear();

        // Collect all enabled keys from the keyboard layout
        for row in KEYBOARD_KEYS.iter() {
            for &key in row.iter() {
                if key != ' ' && valid_letters.is_valid(key) {
                    self.enabled_keys.push(key);
                }
            }
        }

        // Reset scroll position and redraw the framebuffer
        self.scroll_position = 0;
        self.render_compact_keyboard();
        self.needs_redraw = true;
    }
}
