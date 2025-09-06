use crate::{palette::PALETTE, super_draw_target::SuperDrawTarget, Widget, FONT_LARGE};
use alloc::{boxed::Box, string::ToString};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    image::Image,
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU1},
        BinaryColor, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_iconoir::{
    prelude::IconoirNewIcon,
    size32px::navigation::{NavArrowLeft, NavArrowRight},
};
use frost_backup::bip39_words::ValidLetters;
use u8g2_fonts::U8g2TextStyle;

// Constants for framebuffer and keyboard dimensions
const FRAMEBUFFER_WIDTH: u32 = 240;
const TOTAL_COLS: usize = 4;
const KEY_WIDTH: u32 = FRAMEBUFFER_WIDTH / TOTAL_COLS as u32;
const KEY_HEIGHT: u32 = 50;
const TOTAL_ROWS: usize = 7;
const FRAMEBUFFER_HEIGHT: u32 = TOTAL_ROWS as u32 * KEY_HEIGHT;
// Remove this inline color - we'll use PALETTE.primary_container for keys

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
    scroll_position: i32,       // Current scroll offset
    framebuffer: Box<Fb>,       // Boxed framebuffer
    needs_redraw: bool,         // Flag to trigger redraw
    enabled_keys: ValidLetters, // Which keys are enabled
    visible_height: u32,
    current_word_index: usize, // Current word being edited (0-24 for 25 words)
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
            visible_height: 0, // Will be set in set_constraints
            current_word_index: 0,
        };

        // Initialize by rendering the keyboard
        keyboard.render_compact_keyboard();
        keyboard
    }

    pub fn scroll(&mut self, amount: i32) {
        // Calculate the effective height based on what's rendered
        let num_rendered = if self.enabled_keys.count_enabled() == 0 {
            ValidLetters::all_valid().count_enabled()
        } else {
            self.enabled_keys.count_enabled()
        };
        let rows_needed = num_rendered.div_ceil(TOTAL_COLS);
        let keyboard_buffer_height = rows_needed * KEY_HEIGHT as usize;

        let max_scroll = keyboard_buffer_height.saturating_sub(self.visible_height as usize);
        let new_scroll_position = (self.scroll_position - amount).clamp(0, max_scroll as i32);
        self.needs_redraw = new_scroll_position != self.scroll_position;
        self.scroll_position = new_scroll_position;
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

        // Determine which keys to render
        let keys_to_render = if self.enabled_keys.count_enabled() == 0 {
            ValidLetters::all_valid()
        } else {
            self.enabled_keys
        };

        // Always render in compact layout
        for (idx, c) in keys_to_render.iter_valid().enumerate() {
            let row = idx / TOTAL_COLS;
            let col = idx % TOTAL_COLS;

            let x = col as i32 * KEY_WIDTH as i32;
            let y = row as i32 * KEY_HEIGHT as i32;
            let position = Point::new(x + (KEY_WIDTH as i32 / 2), y + (KEY_HEIGHT as i32 / 2));

            let _ = Text::with_text_style(
                &c.to_string(),
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

    pub fn set_valid_keys(&mut self, valid_letters: ValidLetters) {
        // Simply update the enabled keys
        self.enabled_keys = valid_letters;

        // Reset scroll position and redraw the framebuffer
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
            // Handle navigation button touches
            let screen_width = FRAMEBUFFER_WIDTH;
            let screen_height = self.visible_height;

            // Check back button area (left side) - only if we can go back
            if point.x < (screen_width / 2) as i32 && self.current_word_index > 0 {
                let rect =
                    Rectangle::new(Point::new(0, 0), Size::new(screen_width / 2, screen_height));
                return Some(KeyTouch::new(Key::NavBack, rect));
            }
            // Check forward button area (right side) - only if we can go forward
            else if point.x >= (screen_width / 2) as i32 && self.current_word_index < 24 {
                // 0-24 for 25 words
                let rect = Rectangle::new(
                    Point::new((screen_width / 2) as i32, 0),
                    Size::new(screen_width / 2, screen_height),
                );
                return Some(KeyTouch::new(Key::NavForward, rect));
            }
        }

        // In compact layout, keys are positioned differently
        let col = (point.x / KEY_WIDTH as i32) as usize;
        let row = ((point.y + self.scroll_position) / KEY_HEIGHT as i32) as usize;

        if col < TOTAL_COLS {
            let idx = row * TOTAL_COLS + col;
            // Use nth_enabled to get the key at this index
            if let Some(key) = self.enabled_keys.nth_enabled(idx) {
                // Calculate the screen position of the key in compact layout
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

        // Draw based on layout
        if self.enabled_keys.count_enabled() == 0 {
            // Draw navigation buttons when no keys are enabled
            let left_arrow = NavArrowLeft::new(PALETTE.on_background);
            let right_arrow = NavArrowRight::new(PALETTE.on_background);

            let screen_width = bounds.size.width;
            let screen_height = bounds.size.height;
            let icon_size = 32;
            let padding = 10;

            // Clear the area first
            Rectangle::new(Point::zero(), bounds.size)
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.background)
                        .build(),
                )
                .draw(target)?;

            // Draw left arrow if not at the first word
            if self.current_word_index > 0 {
                let left_point = Point::new(padding, (screen_height / 2 - icon_size / 2) as i32);
                Image::new(&left_arrow, left_point).draw(target)?;
            }

            // Draw right arrow if not at the last word
            if self.current_word_index < 24 {
                let right_point = Point::new(
                    (screen_width - icon_size - padding as u32) as i32,
                    (screen_height / 2 - icon_size / 2) as i32,
                );
                Image::new(&right_arrow, right_point).draw(target)?;
            }

            // Removed word number display - not needed with navigation buttons
        } else {
            // Draw the framebuffer for compact keyboard
            let content_height = ((self.framebuffer.size().height as i32 - self.scroll_position)
                .max(0) as u32)
                .min(bounds.size.height);

            if content_height > 0 {
                let skip_pixels =
                    (self.scroll_position.max(0) as usize) * FRAMEBUFFER_WIDTH as usize;

                // Draw the framebuffer content followed by background padding
                let framebuffer_pixels =
                    RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
                        .into_iter()
                        .skip(skip_pixels)
                        .take(FRAMEBUFFER_WIDTH as usize * content_height as usize)
                        .map(|r| match BinaryColor::from(r) {
                            BinaryColor::Off => PALETTE.background,
                            BinaryColor::On => PALETTE.primary_container,
                        });

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
