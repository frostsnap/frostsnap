extern crate alloc;
use core::fmt::Display;

use crate::st7789::Graphics;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use cst816s::TouchGesture;
use cst816s::CST816S;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
    text::{Alignment, Text, TextStyleBuilder},
};
use embedded_graphics_framebuf::FrameBuf;
use embedded_hal as hal;
use embedded_text::{style::TextBoxStyleBuilder, TextBox};
use esp_hal::timer::{self, timg::Timer};
use esp_hal::Blocking;
use frostsnap_core::schnorr_fun::share_backup::{self, ShareBackup};
use fugit::Instant;
use mipidsi::error::Error;
use u8g2_fonts::{fonts, U8g2TextStyle};

pub struct KeyboardKey {
    label: KeyboardKeyType,
    rectangle: Rectangle,
}

pub enum KeyboardKeyType {
    Character(char),
    String(String),
}

impl KeyboardKey {
    pub fn new(point: Point, size: Size, label: KeyboardKeyType) -> Self {
        let rectangle = Rectangle::new(point, size);
        Self { label, rectangle }
    }

    pub fn rectangle(&self) -> Rectangle {
        self.rectangle
    }

    pub fn label(&self) -> &KeyboardKeyType {
        &self.label
    }
}

impl Display for KeyboardKeyType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            KeyboardKeyType::Character(c) => write!(f, "{}", c),
            KeyboardKeyType::String(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Default)]
pub struct Keyboard {
    buffer: Vec<char>,
}

const SCREEN_HEIGHT: u32 = 280;
const HEADER_BUFFER: u32 = 40;
const KEY_HEIGHT: u32 = 50;

impl Keyboard {
    pub fn new() -> Self {
        Self { buffer: vec![] }
    }

    pub fn clear_keyboard(&mut self, framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>) {
        Rectangle::new(
            Point::new(0, (SCREEN_HEIGHT - 2 * KEY_HEIGHT) as i32),
            Size::new(240, 2 * KEY_HEIGHT),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(Rgb565::BLACK)
                .build(),
        )
        .draw(framebuf)
        .unwrap();
    }

    pub fn render_character_key(
        &mut self,
        framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>,
        key: &KeyboardKey,
        is_active: bool,
    ) {
        let rect = key.rectangle;

        if is_active {
            rect.into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(Rgb565::WHITE)
                    .stroke_width(1)
                    .build(),
            )
            .draw(framebuf)
            .unwrap();
        } else {
            rect.into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(Rgb565::new(5, 5, 5))
                    .stroke_width(1)
                    .build(),
            )
            .draw(framebuf)
            .unwrap();

            let font = U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, Rgb565::WHITE);
            Text::with_text_style(
                key.label().to_string().as_str(),
                rect.center(),
                font,
                TextStyleBuilder::new().alignment(Alignment::Center).build(),
            )
            .draw(framebuf)
            .unwrap();
        }
    }

    pub fn print_text_input(&mut self, framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>) {
        let mut x_offset = 0;
        let mut y_offset = 0;
        let spacing_size = 20;

        let rect = Rectangle::new(
            Point::new(x_offset, HEADER_BUFFER as i32),
            Size::new(240, SCREEN_HEIGHT - HEADER_BUFFER - 2 * KEY_HEIGHT),
        );

        rect.into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(Rgb565::BLACK)
                .build(),
        )
        .draw(framebuf)
        .unwrap();

        let chunked_backup =
            self.buffer
                .clone()
                .into_iter()
                .fold(vec![String::new()], |mut chunk_vec, char| {
                    if chunk_vec.last().unwrap().len() < 4 {
                        let last = chunk_vec.last_mut().unwrap();
                        last.push(char);
                    } else {
                        chunk_vec.push(char.to_string());
                    }
                    chunk_vec
                });

        let textbox_style = TextBoxStyleBuilder::new().build();

        // Don't show the top line once the backup gets to a certain length, "pan" down
        if chunked_backup.len() <= 9 {
            let _overflow = TextBox::with_textbox_style(
                "frost1",
                rect,
                U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, Rgb565::WHITE),
                textbox_style,
            )
            .draw(framebuf)
            .unwrap();
            y_offset += spacing_size * 3 / 2;
        }

        // skip the first rows to only show the end 12 chunks
        let rows_to_skip = if chunked_backup.len() <= 12 {
            0
        } else {
            (chunked_backup.len() - 1) / 3 - 3
        };

        for (i, chunk) in chunked_backup[(rows_to_skip * 3)..].into_iter().enumerate() {
            let _overflow = TextBox::with_textbox_style(
                chunk.as_ref(),
                Rectangle::new(
                    Point::new(x_offset, (HEADER_BUFFER as i32) + y_offset),
                    Size::new(
                        280,
                        SCREEN_HEIGHT - HEADER_BUFFER - 2 * KEY_HEIGHT - (y_offset as u32),
                    ),
                ),
                U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, Rgb565::WHITE),
                textbox_style,
            )
            .draw(framebuf)
            .unwrap();
            x_offset += spacing_size * 4;
            // For rows of 3, we want a new line for the 4th, 7th, ... chunk
            if (i + 1) % 3 == 0 {
                y_offset += spacing_size * 3 / 2;
                x_offset = 0;
            }
        }
    }

    pub fn enter_backup<
        'd,
        T: timer::timg::Instance,
        DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
        CommE,
        PinE,
        I2C: hal::i2c::I2c<Error = CommE>,
        PINT: hal::digital::InputPin,
        RST: hal::digital::StatefulOutputPin<Error = PinE>,
    >(
        &mut self,
        display: &mut Graphics<'d, DT>,
        capsense: &mut CST816S<I2C, PINT, RST>,
        timer: &'d Timer<T, Blocking>,
    ) -> ShareBackup {
        display.clear(Rgb565::BLACK);
        display.flush().unwrap();
        self.print_text_input(&mut display.framebuf);

        // Keyboard setup
        let keyboard_keys = [
            ["acde", "fghj"],
            ["klmn", "pqrs"],
            ["tuvw", "xyz0"],
            ["2345", "6789"],
        ];
        let mut key_set_index = 0;
        let mut kbkeys = Vec::new();

        keyboard_keys.iter().for_each(|key_set| {
            let mut keysvec = Vec::new();
            key_set.iter().enumerate().for_each(|(i, row)| {
                row.chars().enumerate().for_each(|(j, c)| {
                    let key = KeyboardKey::new(
                        Point::new(j as i32 * 60, (130 + (i as u32 + 1) * KEY_HEIGHT) as i32),
                        Size::new(60, KEY_HEIGHT),
                        KeyboardKeyType::Character(c),
                    );
                    keysvec.push(key);
                })
            });
            kbkeys.push(keysvec);
        });

        kbkeys[key_set_index].iter().for_each(|k| {
            self.render_character_key(&mut display.framebuf, k, false);
        });
        display.flush().unwrap();

        let mut last_touch: Option<Instant<u64, 1, 1_000_000>> = None;
        let mut touched_key: Option<&KeyboardKey> = None;
        loop {
            let pending_share_backup = format!(
                "frost1{}",
                self.buffer.clone().into_iter().collect::<String>()
            );

            match share_backup::decode_backup(pending_share_backup) {
                Ok(share_backup) => return share_backup,
                Err(e) => {}
            }

            let now = timer::Timer::now(timer);
            let mut is_pressed = || {
                match capsense.read_one_touch_event(true) {
                    None => match last_touch {
                        None => false,
                        Some(last_touch) => {
                            now.checked_duration_since(last_touch).unwrap().to_millis() < 25
                        }
                    },
                    Some(touch) => {
                        // Gestures
                        match (&touch.gesture, touch.action) {
                            // Backspace
                            (TouchGesture::SlideLeft, 1) => {
                                self.buffer.pop();
                                self.print_text_input(&mut display.framebuf);
                                display.flush().unwrap();
                                return false;
                            }

                            // Slide up/down to jog through 8-key groups
                            (TouchGesture::SlideDown, 1) => {
                                key_set_index = key_set_index.wrapping_sub(1);
                                self.clear_keyboard(&mut display.framebuf);
                                kbkeys[key_set_index % 4].iter().for_each(|k| {
                                    self.render_character_key(&mut display.framebuf, k, false);
                                });
                                display.flush().unwrap();
                                return false;
                            }

                            (TouchGesture::SlideUp, 1) => {
                                key_set_index = key_set_index.wrapping_add(1);
                                self.clear_keyboard(&mut display.framebuf);
                                kbkeys[key_set_index % 4].iter().for_each(|k| {
                                    self.render_character_key(&mut display.framebuf, k, false);
                                });
                                display.flush().unwrap();
                                return false;
                            }
                            _ => {}
                        }

                        // Find the key being touched
                        let touch_point = Point::new(touch.x, touch.y);
                        if let Some(k) = kbkeys[key_set_index % 4]
                            .iter()
                            .find(|k| k.rectangle().contains(touch_point))
                        {
                            if touched_key.is_none() {
                                if let KeyboardKeyType::Character(c) = k.label() {
                                    self.buffer.push(*c);
                                }
                                // highlight touched key
                                self.render_character_key(&mut display.framebuf, k, true);
                                display.flush().unwrap();
                                touched_key = Some(k);
                            }
                            last_touch = Some(now);
                            true
                        } else {
                            false
                        }
                    }
                }
            };

            if !is_pressed() {
                if let Some(k) = touched_key {
                    // finger lifted, un-highlight touched key border
                    self.render_character_key(&mut display.framebuf, k, false);
                    self.print_text_input(&mut display.framebuf);
                    touched_key = None;
                    display.flush().unwrap();
                }
            }
        }
    }
}
