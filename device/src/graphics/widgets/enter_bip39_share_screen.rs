use crate::graphics::palette::COLORS;

use super::{AlphabeticKeyboard, KeyTouch};
use alloc::{string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

#[derive(Debug)]
pub struct EnterBip39ShareScreen {
    alphabetic_keyboard: AlphabeticKeyboard,
    words: Vec<String>,
    current_word: String,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    input_display_rect: Rectangle,
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

        Self {
            alphabetic_keyboard,
            words: Vec::new(),
            current_word: String::new(),
            touches: vec![],
            keyboard_rect,
            input_display_rect,
            share_index,
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        self.alphabetic_keyboard
            .draw(&mut target.cropped(&self.keyboard_rect));
        
        // TODO: Draw word input preview
        // TODO: Draw current word and suggestions
        
        self.touches.retain_mut(|touch| {
            touch.draw(target, current_time);
            !touch.is_finished()
        });
    }

    pub fn handle_touch(&mut self, point: Point, current_time: crate::Instant, lift_up: bool) {
        if lift_up {
            if let Some(active_touch) = self.touches.last_mut() {
                if let Some(key) = active_touch.let_go(current_time) {
                    match key {
                        ' ' => {
                            // Space - finish current word
                            if !self.current_word.is_empty() {
                                self.words.push(self.current_word.clone());
                                self.current_word.clear();
                            }
                        }
                        '<' => {
                            // Backspace
                            self.current_word.pop();
                        }
                        c if c.is_alphabetic() => {
                            // Add letter to current word
                            self.current_word.push(c.to_lowercase().next().unwrap_or(c));
                        }
                        _ => {} // Ignore other characters
                    }
                }
            }
        } else {
            let key_touch = if self.keyboard_rect.contains(point) {
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
        // BIP39 mnemonics can be 12, 15, 18, 21, or 24 words
        [12, 15, 18, 21, 24].contains(&self.words.len())
    }

    pub fn get_mnemonic(&self) -> String {
        self.words.join(" ")
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        // scrolling cancels the touch
        if let Some(active_touch) = self.touches.last_mut() {
            active_touch.cancel()
        }
        self.alphabetic_keyboard.handle_vertical_drag(prev_y, new_y);
    }
}