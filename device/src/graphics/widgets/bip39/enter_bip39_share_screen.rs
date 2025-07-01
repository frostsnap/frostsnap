use super::{AlphabeticKeyboard, Bip39InputPreview, WordSelector};
use crate::bip39_words;
use crate::graphics::widgets::KeyTouch;
use alloc::{string::{String, ToString}, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

pub const MAX_WORD_SELECTOR_WORDS: usize = 6;

#[derive(Debug)]
pub struct EnterBip39ShareScreen {
    alphabetic_keyboard: AlphabeticKeyboard,
    word_selector: Option<WordSelector>,
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
            word_selector: None,
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
        // Draw either word selector full-screen or normal keyboard + input
        if let Some(ref mut word_selector) = self.word_selector {
            // Full-screen word selector
            word_selector.draw(target);
        } else {
            // Normal keyboard and input preview
            self.alphabetic_keyboard
                .draw(&mut target.cropped(&self.keyboard_rect));

            // Draw BIP39 input preview
            let input_display_rect = Rectangle::new(
                Point::zero(),
                Size::new(target.bounding_box().size.width, 60),
            );
            self.bip39_input
                .draw(&mut target.cropped(&input_display_rect), current_time);
        }

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
            // Find the last non-cancelled touch
            if let Some(active_touch) = self.touches.iter_mut().rev().find(|t| !t.has_been_let_go())
            {
                if let Some(key) = active_touch.let_go(current_time) {
                    match key {
                        '⌫' => {
                            // Backspace
                            self.bip39_input.backspace();
                            self.update_valid_keys();
                        }
                        c if c.is_alphabetic() => {
                            self.push_letter_and_autocomplete(c);
                        }
                        c if c.is_numeric() => {
                            // Handle word selector index
                            if let Some(ref word_selector) = self.word_selector {
                                if let Some(digit) = c.to_digit(10) {
                                    if let Some(word) =
                                        word_selector.get_word_by_index(digit as usize)
                                    {
                                        // Use unified autocomplete method
                                        self.bip39_input.autocomplete_word(word);
                                        self.update_valid_keys();
                                    }
                                }
                            }
                        }
                        _ => {} // Ignore other characters
                    }
                }
            }
        } else {
            // Handle touch for different modes
            let key_touch = if let Some(ref word_selector) = self.word_selector {
                // Word selector is full-screen, handle its touches directly
                word_selector.handle_touch(point)
            } else {
                // Normal mode: check input preview first, then keyboard
                if let Some(key_touch) = self.bip39_input.handle_touch(point) {
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
                }
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
            } else {
                // No valid key was touched - cancel any active touch
                if let Some(last) = self.touches.last_mut() {
                    if !last.is_finished() {
                        last.cancel();
                    }
                }
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
        // Only handle drag for keyboard, not word selector
        if self.word_selector.is_none() {
            self.alphabetic_keyboard.handle_vertical_drag(prev_y, new_y);
        }
    }

    fn update_valid_keys(&mut self) {
        let current_word = self.bip39_input.current_word();

        // Check if we should show the word selector when we have a partial word
        if !current_word.is_empty() {
            let word_count =
                bip39_words::count_words_with_prefix(current_word, MAX_WORD_SELECTOR_WORDS + 1);
            if word_count > 0 && word_count <= MAX_WORD_SELECTOR_WORDS {
                // Create word selector with the matching words
                let full_screen_size = Size::new(
                    self.keyboard_rect.size.width,
                    self.keyboard_rect.size.height + 60, // Add input preview height
                );
                let matching_words: Vec<_> = bip39_words::words_with_prefix(current_word)
                    .take(MAX_WORD_SELECTOR_WORDS)
                    .collect();
                self.word_selector = Some(WordSelector::new(
                    full_screen_size,
                    matching_words,
                    current_word.to_string(),
                ));
                // Clear all touches when switching to word selector
                self.touches.clear();
            } else {
                // Clear word selector if we shouldn't show it
                if self.word_selector.is_some() {
                    // Clear all touches when switching back to keyboard
                    self.touches.clear();
                    // Force redraw of input preview (including progress bar)
                    self.bip39_input.force_redraw();
                }
                self.word_selector = None;
            }
        } else {
            // Clear word selector
            if self.word_selector.is_some() {
                // Clear all touches when switching back to keyboard
                self.touches.clear();
                // Force redraw of input preview (including progress bar)
                self.bip39_input.force_redraw();
            }
            self.word_selector = None;
        }

        // Always update keyboard valid keys (even if word selector is showing)
        let valid_letters = bip39_words::get_valid_next_letters(self.bip39_input.current_word());
        self.alphabetic_keyboard.set_valid_keys(valid_letters);
    }

    fn push_letter_and_autocomplete(&mut self, letter: char) {
        let word_completed = self.bip39_input.push_letter(letter);

        if !word_completed {
            // Auto-complete if there's only one possible word
            if bip39_words::count_words_with_prefix(self.bip39_input.current_word(), 2) == 1 {
                self.bip39_input.try_accept_autocomplete();
            }
        }

        self.update_valid_keys();
    }
}
