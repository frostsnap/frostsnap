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
        let backup_input_preview = Bech32InputPreview::new(input_display_rect.size, 15 * 4 - 2);

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

                    for (magic_string, backup) in [
                        ("0000", TEST_BACKUP_1),
                        ("2222", TEST_BACKUP_2),
                        ("3333", TEST_BACKUP_3),
                    ] {
                        if self.backup_input_preview.get_input() == magic_string {
                            self.backup_input_preview.clear();
                            for c in backup {
                                self.backup_input_preview.add_character(c);
                            }
                        }
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

// 2-of-3 test backups that can easily be entered by pressing magic keys
const TEST_BACKUP_1: [char; 58] = [
    '6', 'L', '6', '8', 'R', '6', 'E', '7', 'Q', '3', 'H', '8', 'H', '2', 'D', 'F', 'J', 'C', 'X',
    'Z', 'Q', 'D', 'K', 'Q', '2', 'F', 'Y', 'A', '2', 'J', 'K', 'Y', '2', 'D', 'T', 'T', '7', 'G',
    'Z', 'G', 'Y', 'A', 'R', 'T', '8', 'Q', '8', 'X', '7', 'S', 'Q', 'Q', '6', '0', 'C', 'Z', 'D',
    'S',
];

const TEST_BACKUP_2: [char; 58] = [
    'Y', 'J', 'X', 'P', 'Z', '3', 'N', 'S', 'V', '8', 'W', 'E', 'A', 'E', 'V', 'S', 'R', '0', 'V',
    'N', 'C', 'X', '2', 'S', '5', '8', 'K', '3', '8', 'U', '5', 'Q', 'T', '3', 'W', '6', '7', 'S',
    'S', '3', '3', 'S', 'X', 'R', '9', 'Q', 'H', 'M', 'L', '5', '9', 'S', 'U', 'Y', 'T', 'W', 'C',
    'Z',
];

const TEST_BACKUP_3: [char; 58] = [
    'W', 'Y', '3', 'M', 'P', 'G', 'D', 'Z', 'H', 'A', 'X', 'V', 'Y', 'G', 'T', 'K', '5', 'X', 'N',
    '9', '0', '7', 'L', 'Q', '7', 'P', '9', 'S', 'Z', 'A', 'E', 'R', 'Z', 'J', 'K', 'N', 'L', '8',
    'U', '6', 'C', 'V', 'C', 'R', 'U', '4', '2', '8', 'G', 'A', 'T', 'S', '9', 'Q', 'K', 'L', 'N',
    'E',
];
