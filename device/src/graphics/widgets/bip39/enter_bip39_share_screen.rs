use super::{AlphabeticKeyboard, Bip39InputPreview};
use crate::bip39_words;
use crate::graphics::widgets::KeyTouch;
use alloc::{string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

#[derive(Debug)]
pub struct EnterBip39ShareScreen {
    alphabetic_keyboard: AlphabeticKeyboard,
    bip39_input: Bip39InputPreview,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    share_index: u16,
}

impl EnterBip39ShareScreen {
    pub fn new(area: Size, share_index: u16) -> Self {
        let preview_height = 60;
        let keyboard_rect = Rectangle::new(
            Point::new(0, preview_height),
            Size::new(area.width, area.height - preview_height as u32),
        );
        let input_display_rect =
            Rectangle::new(Point::zero(), Size::new(area.width, preview_height as u32));

        let alphabetic_keyboard = AlphabeticKeyboard::new();
        let bip39_input = Bip39InputPreview::new(input_display_rect);

        let mut screen = Self {
            alphabetic_keyboard,
            bip39_input,
            touches: vec![],
            keyboard_rect,
            share_index,
        };

        // Initialize valid keys for empty input
        screen.update_valid_keys();
        screen
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        // Draw keyboard
        self.alphabetic_keyboard
            .draw(&mut target.cropped(&self.keyboard_rect));

        // Draw BIP39 input preview
        let input_display_rect = Rectangle::new(
            Point::zero(),
            Size::new(target.bounding_box().size.width, 60),
        );
        self.bip39_input
            .draw(&mut target.cropped(&input_display_rect), current_time);

        // Draw touches
        self.touches.retain_mut(|touch| {
            touch.draw(target, current_time);
            !touch.is_finished()
        });
    }

    pub fn handle_touch(&mut self, point: Point, current_time: crate::Instant, lift_up: bool) {
        if lift_up {
            // First check if we're tapping the input area to accept autocomplete
            if self.bip39_input.contains(point) && self.bip39_input.has_current_word() {
                // Cancel any active touch before accepting
                if let Some(active_touch) = self.touches.last_mut() {
                    active_touch.cancel();
                }

                if self.bip39_input.try_accept_autocomplete() {
                    self.update_valid_keys();
                }
                return;
            }

            // Otherwise process normal key release
            if let Some(active_touch) = self.touches.last_mut() {
                if let Some(key) = active_touch.let_go(current_time) {
                    match key {
                        ' ' => {
                            // Space - accepts autocomplete if available
                            self.bip39_input.accept_word();
                            self.update_valid_keys();
                        }
                        '⌫' => {
                            // Backspace
                            self.bip39_input.backspace();
                            self.update_valid_keys();
                        }
                        c if c.is_alphabetic() => {
                            self.push_letter_and_autocomplete(c);
                        }
                        _ => {} // Ignore other characters
                    }
                }
            }
        } else {
            // Check backspace button in input preview
            let key_touch = if let Some(key_touch) = self.bip39_input.handle_touch(point) {
                Some(key_touch)
            } else if self.keyboard_rect.contains(point) {
                let translated_point = point - self.keyboard_rect.top_left;
                self.alphabetic_keyboard
                    .handle_touch(translated_point)
                    .map(|mut key_touch| {
                        key_touch.translate(self.keyboard_rect.top_left);
                        key_touch
                    })
            } else {
                None
            };

            if let Some(key_touch) = key_touch {
                if let Some(last) = self.touches.last_mut() {
                    if last.key == key_touch.key {
                        self.touches.pop();
                    } else {
                        last.cancel();
                    }
                }
                self.touches.push(key_touch);
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.bip39_input.is_finished()
    }

    pub fn get_mnemonic(&self) -> String {
        self.bip39_input.get_mnemonic()
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        // scrolling cancels the touch
        if let Some(active_touch) = self.touches.last_mut() {
            active_touch.cancel()
        }
        self.alphabetic_keyboard.handle_vertical_drag(prev_y, new_y);
    }

    pub fn needs_redraw(&self) -> bool {
        !self.touches.is_empty()
    }

    fn update_valid_keys(&mut self) {
        let current_word = self.bip39_input.current_word();
        let valid_letters = bip39_words::get_valid_next_letters(current_word);

        self.alphabetic_keyboard.set_valid_keys(valid_letters);
    }

    fn push_letter_and_autocomplete(&mut self, letter: char) {
        let word_completed = self.bip39_input.push_letter(letter);

        if !word_completed {
            let valid_letters =
                bip39_words::get_valid_next_letters(self.bip39_input.current_word());
            let valid_count = valid_letters.0.iter().filter(|&&v| v).count();
            if valid_count == 1 {
                self.bip39_input.try_accept_autocomplete();
            }
        }

        self.update_valid_keys();
    }
}
