use crate::graphics::palette::COLORS;

use super::{Bech32InputPreview, Bech32Keyboard, KeyTouch};
use alloc::{string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use frostsnap_core::schnorr_fun::frost::SecretShare;

#[derive(Debug)]
pub struct EnterShareScreen {
    bech32_keyboard: Bech32Keyboard,
    backup_input_preview: Bech32InputPreview,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    input_display_rect: Rectangle,
    share_index: u16,
}

impl EnterShareScreen {
    pub fn new(area: Size, share_index: u16) -> Self {
        let preview_height = 60;
        let keyboard_rect = Rectangle::new(
            Point::new(0, preview_height),
            Size::new(area.width, area.height - preview_height as u32),
        );
        let input_display_rect =
            Rectangle::new(Point::zero(), Size::new(area.width, preview_height as u32));
        let mut backup_input_preview = Bech32InputPreview::new(input_display_rect.size, 15 * 4 - 2);

        // grey
        let _chars_grey = [
            '8', 'J', 'N', 'Z', '7', 'Q', '7', 'J', 'E', 'C', '7', '2', 'E', '5', 'V', '8', 'Q',
            'A', 'Q', 'T', 'W', 'W', '4', 'J', 'W', 'K', 'T', 'G', 'P', 'U', '4', 'K', '5', 'X',
            '2', 'N', 'J', 'S', '5', 'J', 'M', 'Y', '7', 'C', 'H', 'Q', 'H', '5', 'C', 'V', 'U',
            'Q', '7', '7', 'U', '8', 'Z', 'H',
        ];

        let _chars_black = [
            '4', 'T', 'P', 'W', '9', 'D', '8', 'F', 'F', 'H', 'U', 'F', 'A', 'M', 'N', 'A', 'N',
            'W', 'F', 'V', 'E', 'W', '9', 'Q', 'S', '0', '7', '3', 'J', 'L', 'X', 'D', 'T', 'S',
            'N', '3', 'T', 'L', '7', 'K', 'N', 'Q', 'K', 'G', 'Y', '0', '0', 'T', '6', 'G', '4',
            'Q', '2', '5', 'K', 'E', 'N', 'H',
        ];

        let _chars_blue = [
            'R', 'R', '0', 'E', 'V', 'E', '0', 'L', 'E', 'K', '6', 'F', 'Z', 'Z', 'M', '5', '9',
            'L', 'J', 'W', 'Y', 'D', '5', 'W', 'J', 'W', '5', 'S', 'X', '2', '0', 'A', 'V', 'A',
            'C', '9', '9', 'Q', '2', '6', 'S', 'J', '7', 'W', 'C', '2', '9', 'V', 'N', 'L', 'D',
            'S', 'F', 'W', 'S', 'W', 'G', 'Y',
        ];

        for v in _chars_blue {
            backup_input_preview.add_character(v);
        }

        let bech32_keyboard = Bech32Keyboard::new(keyboard_rect.size.height);

        Self {
            bech32_keyboard,
            backup_input_preview,
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
        self.bech32_keyboard
            .draw(&mut target.cropped(&self.keyboard_rect));
        self.backup_input_preview
            .draw(&mut target.cropped(&self.input_display_rect), current_time);

        self.touches.retain_mut(|touch| {
            touch.draw(target, current_time);
            !touch.is_finished()
        });
    }

    pub fn handle_touch(&mut self, point: Point, current_time: crate::Instant, lift_up: bool) {
        if lift_up {
            if let Some(active_touch) = self.touches.last_mut() {
                if let Some(key) = active_touch.let_go(current_time) {
                    self.backup_input_preview.add_character(key);
                    if self.backup_input_preview.is_finished() && !self.is_share_valid() {
                        self.backup_input_preview.set_input_color(COLORS.error);
                    } else {
                        self.backup_input_preview.set_input_color(COLORS.primary);
                    }
                }
            }
        } else {
            let key_touch = if self.keyboard_rect.contains(point) {
                let translated_point = point - self.keyboard_rect.top_left;
                self.bech32_keyboard
                    .handle_touch(translated_point)
                    .map(|mut key_touch| {
                        key_touch.translate(self.keyboard_rect.top_left);
                        key_touch
                    })
            } else {
                self.backup_input_preview.handle_touch(point)
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
        self.backup_input_preview.is_finished()
    }

    pub fn try_create_share(&self) -> Result<SecretShare, String> {
        assert!(self.is_finished(), "must be finished to take share");
        let characters = self.backup_input_preview.get_input();
        let backup_string = format!("frost[{}]1{}", self.share_index, characters.to_lowercase());

        SecretShare::from_bech32_backup(&backup_string).map_err(|_e| backup_string)
    }

    pub fn is_share_valid(&self) -> bool {
        if !self.is_finished() {
            return false;
        }

        self.try_create_share().is_ok()
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        // scrolling cancels the touch
        if let Some(active_touch) = self.touches.last_mut() {
            active_touch.cancel()
        }
        self.bech32_keyboard.handle_vertical_drag(prev_y, new_y);
    }
}
