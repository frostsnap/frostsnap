use crate::palette::PALETTE;
use crate::super_draw_target::SuperDrawTarget;
use crate::{Key, KeyTouch, Widget, FONT_LARGE, FONT_SMALL};
use alloc::string::String;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::ValidLetters;
use u8g2_fonts::U8g2TextStyle;

// T9 keyboard layout
const T9_KEYS: [[&str; 3]; 3] = [
    ["abc", "def", "ghi"],
    ["jkl", "mno", "pqr"],
    ["stu", "vwx", "yz"],
];

const KEY_MARGIN: u32 = 12;
const KEY_SPACING: u32 = 8;
const CORNER_RADIUS: u32 = 12;
const MULTI_TAP_TIMEOUT_MS: u64 = 800; // Time before committing a letter

#[derive(Debug, Clone, Copy, PartialEq)]
struct T9KeyPosition {
    row: usize,
    col: usize,
}

#[derive(Debug)]
struct MultiTapState {
    key_pos: T9KeyPosition,
    tap_count: usize,
    last_tap_time: crate::Instant,
}

#[derive(Debug)]
pub struct T9Keyboard {
    valid_keys: ValidLetters,
    key_rects: [[Rectangle; 3]; 3],
    bounds: Rectangle,
    multi_tap_state: Option<MultiTapState>,
    needs_redraw: bool,
}

impl T9Keyboard {
    pub fn new(keyboard_height: u32) -> Self {
        // Calculate key dimensions - 3x3 grid
        let available_width = 240 - (2 * KEY_MARGIN);
        let key_width = (available_width - (2 * KEY_SPACING)) / 3;

        // Use full height for the grid
        let grid_height = keyboard_height - (2 * KEY_MARGIN);
        let key_height = (grid_height - (2 * KEY_SPACING)) / 3;

        // Create key rectangles
        let mut key_rects = [[Rectangle::zero(); 3]; 3];
        for row in 0..3 {
            for col in 0..3 {
                let x = KEY_MARGIN + col * (key_width + KEY_SPACING);
                let y = KEY_MARGIN + row * (key_height + KEY_SPACING);
                key_rects[row as usize][col as usize] = Rectangle::new(
                    Point::new(x as i32, y as i32),
                    Size::new(key_width, key_height),
                );
            }
        }

        let bounds = Rectangle::new(Point::zero(), Size::new(240, keyboard_height));

        Self {
            valid_keys: ValidLetters::default(),
            key_rects,
            bounds,
            multi_tap_state: None,
            needs_redraw: true,
        }
    }

    pub fn set_valid_keys(&mut self, valid_keys: ValidLetters) {
        // ValidLetters doesn't implement PartialEq, so we'll just always mark as needing redraw
        // This is called infrequently enough that it shouldn't matter
        self.valid_keys = valid_keys;
        self.needs_redraw = true;
    }

    /// Get the current pending character based on multi-tap state
    pub fn get_pending_char(&self) -> Option<char> {
        if let Some(ref state) = self.multi_tap_state {
            let key_chars = T9_KEYS[state.key_pos.row][state.key_pos.col];
            let valid_letters = self.get_valid_letters_for_key(key_chars);
            if valid_letters.is_empty() {
                None
            } else {
                valid_letters
                    .chars()
                    .nth(state.tap_count % valid_letters.len())
            }
        } else {
            None
        }
    }

    /// Check if multi-tap timeout has expired
    pub fn check_timeout(&mut self, current_time: crate::Instant) -> Option<char> {
        if let Some(ref state) = self.multi_tap_state {
            let elapsed = current_time.saturating_duration_since(state.last_tap_time);

            if elapsed > MULTI_TAP_TIMEOUT_MS {
                // Timeout expired, commit the character
                let char_to_commit = self.get_pending_char();
                self.multi_tap_state = None;
                self.needs_redraw = true;
                return char_to_commit;
            }
        }
        None
    }

    fn get_valid_letters_for_key(&self, key_chars: &str) -> String {
        // Get only the valid letters from this T9 key
        key_chars
            .chars()
            .filter(|&c| self.valid_keys.is_valid(c.to_ascii_uppercase()))
            .collect()
    }

    fn get_key_position_at_point(&self, point: Point) -> Option<T9KeyPosition> {
        for (row_idx, row) in self.key_rects.iter().enumerate() {
            for (col_idx, rect) in row.iter().enumerate() {
                if rect.contains(point) {
                    return Some(T9KeyPosition {
                        row: row_idx,
                        col: col_idx,
                    });
                }
            }
        }
        None
    }

    /// Handle a key press and return the character to output (if any)
    pub fn handle_key_press(
        &mut self,
        point: Point,
        current_time: crate::Instant,
    ) -> Option<(Key, Rectangle)> {
        // Check if it's a T9 key
        if let Some(key_pos) = self.get_key_position_at_point(point) {
            let rect = self.key_rects[key_pos.row][key_pos.col];

            // Check if this key has any valid letters
            let key_chars = T9_KEYS[key_pos.row][key_pos.col];
            let valid_letters = self.get_valid_letters_for_key(key_chars);
            if valid_letters.is_empty() {
                return None;
            }

            // Handle multi-tap logic
            let mut char_to_output = None;

            if let Some(ref mut state) = self.multi_tap_state {
                if state.key_pos == key_pos {
                    // Same key pressed again - increment tap count
                    state.tap_count += 1;
                    state.last_tap_time = current_time;
                    self.needs_redraw = true;
                } else {
                    // Different key pressed - commit previous character and start new
                    char_to_output = self.get_pending_char();
                    self.multi_tap_state = Some(MultiTapState {
                        key_pos,
                        tap_count: 0,
                        last_tap_time: current_time,
                    });
                    self.needs_redraw = true;
                }
            } else {
                // No active multi-tap - start new
                self.multi_tap_state = Some(MultiTapState {
                    key_pos,
                    tap_count: 0,
                    last_tap_time: current_time,
                });
                self.needs_redraw = true;
            }

            // Return the committed character if any
            if let Some(ch) = char_to_output {
                return Some((Key::Keyboard(ch), rect));
            }
        }

        None
    }

    fn draw_key<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        rect: Rectangle,
        text: &str,
        is_valid: bool,
    ) -> Result<(), D::Error> {
        let key_color = if is_valid {
            PALETTE.surface_variant
        } else {
            PALETTE.surface
        };

        let text_color = if is_valid {
            PALETTE.on_surface
        } else {
            PALETTE.outline
        };

        // Draw key background
        RoundedRectangle::with_equal_corners(rect, Size::new(CORNER_RADIUS, CORNER_RADIUS))
            .into_styled(PrimitiveStyle::with_fill(key_color))
            .draw(target)?;

        // Draw key text - larger font for T9 keys
        if text.len() <= 3 {
            Text::with_text_style(
                text,
                rect.center(),
                U8g2TextStyle::new(FONT_LARGE, text_color),
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(Baseline::Middle)
                    .build(),
            )
            .draw(target)?;
        } else {
            Text::with_text_style(
                text,
                rect.center(),
                U8g2TextStyle::new(FONT_SMALL, text_color),
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(Baseline::Middle)
                    .build(),
            )
            .draw(target)?;
        }

        Ok(())
    }
}

impl crate::DynWidget for T9Keyboard {
    fn set_constraints(&mut self, _max_size: Size) {
        // T9Keyboard has fixed size based on its bounds
    }

    fn sizing(&self) -> crate::Sizing {
        self.bounds.size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<KeyTouch> {
        if !lift_up {
            // Only handle key press on touch down
            if let Some((key, rect)) = self.handle_key_press(point, current_time) {
                return Some(KeyTouch::new(key, rect));
            }
        }
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag behavior for keyboard
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl Widget for T9Keyboard {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Check for timeout before drawing
        let timed_out = self.check_timeout(current_time).is_some();

        // Only redraw if needed
        if !self.needs_redraw && !timed_out {
            return Ok(());
        }

        // Clear background
        self.bounds
            .into_styled(PrimitiveStyle::with_fill(PALETTE.background))
            .draw(target)?;

        // Draw T9 keys
        for (row_idx, row) in self.key_rects.iter().enumerate() {
            for (col_idx, rect) in row.iter().enumerate() {
                let key_chars = T9_KEYS[row_idx][col_idx];
                let valid_letters = self.get_valid_letters_for_key(key_chars);

                // Skip drawing if no valid letters
                if valid_letters.is_empty() {
                    continue;
                }

                // Check if this is the active multi-tap key
                let is_active = self
                    .multi_tap_state
                    .as_ref()
                    .map(|state| state.key_pos.row == row_idx && state.key_pos.col == col_idx)
                    .unwrap_or(false);

                if is_active {
                    // Draw with highlight to show it's active
                    RoundedRectangle::with_equal_corners(
                        *rect,
                        Size::new(CORNER_RADIUS, CORNER_RADIUS),
                    )
                    .into_styled(PrimitiveStyle::with_fill(PALETTE.primary_container))
                    .draw(target)?;

                    // Show the current character being selected
                    if let Some(pending_char) = self.get_pending_char() {
                        // Create a buffer for the single character
                        let mut char_buf = [0u8; 4];
                        let char_str = pending_char.encode_utf8(&mut char_buf);

                        Text::with_text_style(
                            char_str,
                            rect.center(),
                            U8g2TextStyle::new(FONT_LARGE, PALETTE.on_primary_container),
                            TextStyleBuilder::new()
                                .alignment(Alignment::Center)
                                .baseline(Baseline::Middle)
                                .build(),
                        )
                        .draw(target)?;
                    }
                } else {
                    // Draw key with only valid letters
                    self.draw_key(target, *rect, &valid_letters, true)?;
                }
            }
        }

        self.needs_redraw = false;
        Ok(())
    }
}
