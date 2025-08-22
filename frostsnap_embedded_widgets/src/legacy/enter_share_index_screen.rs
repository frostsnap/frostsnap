use super::{NumericKey, NumericKeyboard, ShareIndexInputDisplay};
use crate::super_draw_target::SuperDrawTarget;
use crate::{Key, KeyTouch, Widget};
use alloc::{vec, vec::Vec};
use embedded_graphics::{
    geometry::AnchorPoint, pixelcolor::Rgb565, prelude::*, primitives::Rectangle,
};

#[derive(Debug)]
pub struct EnterShareIndexScreen {
    numeric_keyboard: NumericKeyboard,
    share_index_input_display: ShareIndexInputDisplay,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    input_display_rect: Rectangle,
}

impl EnterShareIndexScreen {
    pub fn new(area: Size) -> Self {
        let share_index_input_display = ShareIndexInputDisplay::new();
        let keyboard_height = area.height - share_index_input_display.min_height();

        let numeric_keyboard = NumericKeyboard::new(Size {
            height: keyboard_height,
            width: area.width,
        });
        let keyboard_size = numeric_keyboard.size();

        let input_display_height = area.height - keyboard_size.height;

        let input_display_rect =
            Rectangle::new(Point::zero(), Size::new(area.width, input_display_height));
        let keyboard_rect = Rectangle::new(
            input_display_rect.anchor_point(AnchorPoint::BottomLeft),
            keyboard_size,
        );

        Self {
            numeric_keyboard,
            share_index_input_display,
            touches: vec![],
            keyboard_rect,
            input_display_rect,
        }
    }
}

impl crate::DynWidget for EnterShareIndexScreen {
    fn set_constraints(&mut self, _max_size: Size) {
        // EnterShareIndexScreen has fixed size based on keyboard and input display
    }

    fn sizing(&self) -> crate::Sizing {
        // Total size is the keyboard rect plus input display rect
        crate::Sizing {
            width: self.keyboard_rect.size.width,
            height: self.keyboard_rect.size.height + self.input_display_rect.size.height,
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        if is_release {
            if let Some(active_touch) = self.touches.last_mut() {
                if let Some(Key::Keyboard(c)) = active_touch.let_go(current_time) {
                    let numeric_key = NumericKey::from_char(c).expect("from numeric keyboard");
                    match numeric_key {
                        NumericKey::Digit(digit) => self.share_index_input_display.add_digit(digit),
                        NumericKey::Backspace => self.share_index_input_display.backspace(),
                        NumericKey::Confirm => {
                            let _index = self
                                .share_index_input_display
                                .index
                                .expect("confirm can't be pressed if there's nothing");
                            // TODO: How to send this up?
                        }
                    }

                    // TODO: Update numeric keyboard state based on input
                    // self.numeric_keyboard.disable_empty_input_keys(self.share_index_input_display.is_empty());
                }
            }
        } else if self.keyboard_rect.contains(point) {
            let translated_point = point - self.keyboard_rect.top_left;
            if let Some(mut key_touch) =
                self.numeric_keyboard
                    .handle_touch(translated_point, current_time, is_release)
            {
                key_touch.translate(self.keyboard_rect.top_left);
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

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag behavior for this screen
    }

    fn force_full_redraw(&mut self) {
        self.numeric_keyboard.force_full_redraw();
        self.share_index_input_display.force_full_redraw();
    }
}

impl Widget for EnterShareIndexScreen {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut keyboard_target = target.clone().crop(self.keyboard_rect);
        self.numeric_keyboard
            .draw(&mut keyboard_target, current_time)?;

        let mut input_display_target = target.clone().crop(self.input_display_rect);
        self.share_index_input_display
            .draw(&mut input_display_target, current_time)?;

        self.touches.retain_mut(|touch| {
            touch.draw(target, current_time);
            !touch.is_finished()
        });

        Ok(())
    }
}
