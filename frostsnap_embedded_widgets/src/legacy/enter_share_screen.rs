use crate::palette::PALETTE;
use crate::{Key, KeyTouch, Widget};
use crate::prelude::FreeCrop;
use super::{Bech32InputPreview, Bech32Keyboard};
use alloc::{vec::Vec, vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

#[derive(Debug)]
pub struct EnterShareScreen {
    bech32_keyboard: Bech32Keyboard,
    backup_input_preview: Bech32InputPreview,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    input_display_rect: Rectangle,
    _share_index: u16,
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
            _share_index: share_index,
        }
    }

    fn is_share_valid(&self) -> bool {
        // TODO: Implement actual share validation
        true
    }
}

impl crate::DynWidget for EnterShareScreen {
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        if is_release {
            if let Some(active_touch) = self.touches.last_mut() {
                if let Some(key) = active_touch.let_go(current_time) {
                    if let Key::Keyboard(c) = key {
                        self.backup_input_preview.add_character(c);
                        if self.backup_input_preview.is_finished() && !self.is_share_valid() {
                            self.backup_input_preview.set_input_color(PALETTE.error);
                        } else {
                            self.backup_input_preview.set_input_color(PALETTE.primary);
                        }

                        for (magic_string, backup) in [
                            ("00000", TEST_BACKUP_1),
                            ("00002", TEST_BACKUP_1A),
                            ("22220", TEST_BACKUP_2),
                            ("33330", TEST_BACKUP_3),
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
            }
        } else {
            let key_touch = if self.keyboard_rect.contains(point) {
                let translated_point = point - self.keyboard_rect.top_left;
                self.bech32_keyboard
                    .handle_touch(translated_point, current_time, is_release)
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
        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // scrolling cancels the touch
        if let Some(active_touch) = self.touches.last_mut() {
            active_touch.cancel()
        }
        self.bech32_keyboard
            .handle_vertical_drag(prev_y, new_y, is_release);
    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(Size::new(240, 320)) // Standard screen size
    }
    
    fn force_full_redraw(&mut self) {
        self.backup_input_preview.force_full_redraw();
        self.bech32_keyboard.force_full_redraw();
    }
}

impl Widget for EnterShareScreen {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        let mut keyboard_target = target.free_cropped(&self.keyboard_rect);
        self.bech32_keyboard.draw(&mut keyboard_target, current_time)?;

        let mut input_display_target = target.free_cropped(&self.input_display_rect);
        <Bech32InputPreview as Widget>::draw(&mut self.backup_input_preview, &mut input_display_target, current_time)?;

        self.touches.retain_mut(|touch| {
            touch.draw(target, current_time);
            !touch.is_finished()
        });

        Ok(())
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

// incompatible with the other shares
const TEST_BACKUP_1A: [char; 58] = [
    'R', 'R', '0', 'E', 'V', 'E', '0', 'L', 'E', 'K', '6', 'F', 'Z', 'Z', 'M', '5', '9', 'L', 'J',
    'W', 'Y', 'D', '5', 'W', 'J', 'W', '5', 'S', 'X', '2', '0', 'A', 'V', 'A', 'C', '9', '9', 'Q',
    '2', '6', 'S', 'J', '7', 'W', 'C', '2', '9', 'V', 'N', 'L', 'D', 'S', 'F', 'W', 'S', 'W', 'G',
    'Y',
];
