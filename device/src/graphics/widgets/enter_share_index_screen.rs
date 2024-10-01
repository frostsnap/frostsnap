use super::{KeyTouch, NumericKey, NumericKeyboard, ShareIndexInputDisplay};
use alloc::vec::Vec;
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

    pub fn draw(
        &mut self,
        target: &mut impl DrawTarget<Color = Rgb565>,
        current_time: crate::Instant,
    ) {
        let keyboard_size = self.numeric_keyboard.size();
        let mut input_size = target.bounding_box().size;
        input_size.height -= keyboard_size.height;
        self.numeric_keyboard
            .draw(&mut target.cropped(&self.keyboard_rect));
        self.share_index_input_display
            .draw(&mut target.clipped(&self.input_display_rect));

        // self.keyboard_rect
        //     .into_styled(PrimitiveStyle::with_stroke(Rgb565::BLUE, 1))
        //     .draw(target);
        self.touches.retain_mut(|touch| {
            touch.draw(target, current_time);
            !touch.is_finished()
        });
    }

    pub fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<u16> {
        if lift_up {
            if let Some(active_touch) = self.touches.last_mut() {
                if let Some(c) = active_touch.let_go(current_time) {
                    let numeric_key = NumericKey::from_char(c).expect("from numeric keyboard");
                    match numeric_key {
                        NumericKey::Digit(digit) => self.share_index_input_display.add_digit(digit),
                        NumericKey::Backspace => self.share_index_input_display.backspace(),
                        NumericKey::Confirm => {
                            let index = self
                                .share_index_input_display
                                .index
                                .expect("confirm can't be pressed if there's nothing");
                            return Some(index);
                        }
                    }

                    self.numeric_keyboard
                        .disable_empty_input_keys(self.share_index_input_display.is_empty());
                }
            }
        } else if self.keyboard_rect.contains(point) {
            let translated_point = point - self.keyboard_rect.top_left;
            if let Some(mut key_touch) = self.numeric_keyboard.handle_touch(translated_point) {
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
}
