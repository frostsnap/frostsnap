use crate::graphics::palette::COLORS;

use super::icons;
use super::key_touch::KeyTouch;
use alloc::string::ToString;
use embedded_graphics::mono_font::{ascii::*, MonoTextStyle};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

// Constants for the keyboard layout
const KEYBOARD_KEYS_NUMBERS: [[char; 3]; 4] = [
    ['1', '2', '3'],
    ['4', '5', '6'],
    ['7', '8', '9'],
    ['⌫', '0', '✓'],
];

#[derive(Debug)]
pub struct NumericKeyboard {
    disable_empty_input_keys: bool,
    redraw: bool,
    redraw_disabled_keys: bool,
    key_size: Size,
}

impl NumericKeyboard {
    // Create a new NumericKeyboard instance
    pub fn new(area: Size) -> Self {
        let key_width = area.width / 3;
        let key_height = area.height / 4;
        Self {
            disable_empty_input_keys: true,
            redraw: true,
            redraw_disabled_keys: false,
            key_size: Size {
                width: key_width,
                height: key_height,
            },
        }
    }

    pub fn size(&self) -> Size {
        Size {
            height: self.key_size.height * 4,
            width: self.key_size.width * 3,
        }
    }

    // Handle a touch event and return an Option<KeyTouch>
    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        // Determine the starting position (bottom-center alignment)
        for (row_index, row) in KEYBOARD_KEYS_NUMBERS.iter().enumerate() {
            for (col_index, &key) in row.iter().enumerate() {
                let x = col_index as i32 * self.key_size.width as i32;
                let y = row_index as i32 * self.key_size.height as i32;

                let rect = Rectangle::new(
                    Point::new(x, y),
                    Size::new(self.key_size.width, self.key_size.height),
                );

                // Check if the touch is within this key
                if point.x >= x
                    && point.x < x + self.key_size.width as i32
                    && point.y >= y
                    && point.y < y + self.key_size.width as i32
                {
                    let is_disabled = match key {
                        '0' | '✓' | '⌫' => self.disable_empty_input_keys,
                        _ => false,
                    };

                    if !is_disabled {
                        return Some(KeyTouch::new(key, rect));
                    } else {
                        return None;
                    }
                }
            }
        }

        None
    }

    pub fn disable_empty_input_keys(&mut self, is_input_empty: bool) {
        let changed = is_input_empty != self.disable_empty_input_keys;
        self.disable_empty_input_keys = is_input_empty;
        self.redraw_disabled_keys = changed;
    }

    // Draw the static keyboard (only called once)
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if !self.redraw && !self.redraw_disabled_keys {
            return;
        }

        for (row_index, row) in KEYBOARD_KEYS_NUMBERS.iter().enumerate() {
            for (col_index, &key) in row.iter().enumerate() {
                let x = col_index as i32 * self.key_size.width as i32;
                let y = row_index as i32 * self.key_size.height as i32;
                let rect = Rectangle::new(
                    Point::new(x, y),
                    Size::new(self.key_size.width, self.key_size.height),
                );

                if self.redraw {
                    // Clear the key
                    let _ = rect
                        .into_styled(
                            PrimitiveStyleBuilder::new()
                                .fill_color(COLORS.background)
                                .build(),
                        )
                        .draw(target);
                }

                let position = Point::new(
                    x + (self.key_size.width as i32) / 2,
                    y + (self.key_size.height as i32) / 2,
                );
                let color = match self.disable_empty_input_keys {
                    true => match key {
                        '1'..='9' => COLORS.primary,
                        '0' | '⌫' | '✓' => COLORS.disabled,
                        _ => unreachable!(),
                    },
                    false => match key {
                        '0'..='9' => COLORS.primary,
                        '⌫' => Rgb565::RED,
                        '✓' => Rgb565::GREEN,
                        _ => unreachable!(),
                    },
                };
                match key {
                    '0'..='9' => {
                        if self.redraw || (key == '0' && self.redraw_disabled_keys) {
                            let _ = Text::with_text_style(
                                &ToString::to_string(&key),
                                position,
                                MonoTextStyle::new(&FONT_10X20, color),
                                TextStyleBuilder::new()
                                    .alignment(Alignment::Center)
                                    .baseline(Baseline::Middle)
                                    .build(),
                            )
                            .draw(target);
                        }
                    }
                    '⌫' => {
                        if self.redraw || self.redraw_disabled_keys {
                            icons::backspace()
                                .with_color(color)
                                .with_center(position)
                                .draw(target);
                        }
                    }
                    '✓' => {
                        if self.redraw || self.redraw_disabled_keys {
                            icons::confirm()
                                .with_color(color)
                                .with_center(position)
                                .draw(target);
                        }
                    }
                    _ => unimplemented!(),
                };
            }
        }

        self.redraw = false;
        self.redraw_disabled_keys = false;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumericKey {
    Digit(u8),
    Backspace,
    Confirm,
}

impl NumericKey {
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            '0'..='9' => {
                // Convert character to its corresponding digit
                NumericKey::Digit(c as u8 - b'0')
            }
            '⌫' => NumericKey::Backspace,
            '✓' => NumericKey::Confirm,
            _ => return None, // Handle unexpected characters
        })
    }
}
