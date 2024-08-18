extern crate alloc;
use core::fmt::Display;

use crate::graphics::Graphics;
use crate::graphics::{FONT_LARGE, FONT_MED};
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
use esp_hal::timer::{self, timg::Timer};
use esp_hal::Blocking;
use frostsnap_core::schnorr_fun::frost::SecretShare;
use fugit::Instant;
use mipidsi::error::Error;
use u8g2_fonts::U8g2TextStyle;

const SCREEN_HEIGHT: u32 = 280;
const SCREEN_WIDTH: u32 = 240;
const HEADER_BUFFER: u32 = 20; // small padding since we don't show the header on the keyboard screen
const KEY_HEIGHT: u32 = 50;
const BACKUP_LEFT_PADDING: u32 = 5;

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

            let font = U8g2TextStyle::new(FONT_MED, Rgb565::WHITE);
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

    pub fn render_backup_input(
        &mut self,
        framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>,
        hrp: &str,
    ) {
        let mut y_offset = 0;
        let spacing_size = 20;

        // clear area
        let rect = Rectangle::new(
            Point::new(0, HEADER_BUFFER as i32),
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
                .fold(vec!["".to_string()], |mut chunk_vec, char| {
                    if chunk_vec.last().unwrap().len() < 4 {
                        let last = chunk_vec.last_mut().unwrap();
                        last.push(char);
                    } else {
                        chunk_vec.push(char.to_string());
                    }
                    chunk_vec
                });

        // Don't show the top line once the backup gets to a certain length, "pan" down
        if chunked_backup.len() <= 4 * 3 {
            Text::with_text_style(
                hrp,
                Point::new((SCREEN_WIDTH / 2) as i32, HEADER_BUFFER as i32),
                U8g2TextStyle::new(FONT_LARGE, Rgb565::WHITE),
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(embedded_graphics::text::Baseline::Top)
                    .build(),
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

        for row_chunks in chunked_backup[(rows_to_skip * 3)..].chunks(3) {
            Text::with_baseline(
                row_chunks.join(" ").as_ref(),
                Point::new(
                    BACKUP_LEFT_PADDING as i32,
                    (HEADER_BUFFER as i32) + y_offset,
                ),
                U8g2TextStyle::new(FONT_LARGE, Rgb565::WHITE),
                embedded_graphics::text::Baseline::Top,
            )
            .draw(framebuf)
            .unwrap();

            y_offset += spacing_size * 3 / 2;
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
        proposed_share_index: Option<u32>,
    ) -> SecretShare {
        let hrp_display_string = format!(
            "frost[{}]",
            proposed_share_index
                .map(|index| index.to_string())
                .unwrap_or("_".to_string())
        );
        self.buffer.push('1');

        display.clear();
        display.flush().unwrap();
        self.render_backup_input(&mut display.framebuf, &hrp_display_string);

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
            let pending_share_backup = self.buffer.clone().into_iter().collect::<String>();
            if let Ok(share_backup) = SecretShare::from_bech32_backup(&pending_share_backup) {
                return share_backup;
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
                                if self.buffer.len() > 1 {
                                    self.buffer.pop();
                                    self.render_backup_input(
                                        &mut display.framebuf,
                                        &hrp_display_string,
                                    );
                                    display.flush().unwrap();
                                }
                                false
                            }

                            // Slide up/down to jog through 8-key groups
                            (TouchGesture::SlideDown, 1) => {
                                key_set_index = key_set_index.wrapping_sub(1);
                                self.clear_keyboard(&mut display.framebuf);
                                kbkeys[key_set_index % 4].iter().for_each(|k| {
                                    self.render_character_key(&mut display.framebuf, k, false);
                                });
                                display.flush().unwrap();
                                false
                            }

                            (TouchGesture::SlideUp, 1) => {
                                key_set_index = key_set_index.wrapping_add(1);
                                self.clear_keyboard(&mut display.framebuf);
                                kbkeys[key_set_index % 4].iter().for_each(|k| {
                                    self.render_character_key(&mut display.framebuf, k, false);
                                });
                                display.flush().unwrap();
                                false
                            }
                            (TouchGesture::SingleClick, _) => {
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
                            _ => false,
                        }
                    }
                }
            };

            if !is_pressed() {
                if let Some(k) = touched_key {
                    // finger lifted, un-highlight touched key border
                    self.render_character_key(&mut display.framebuf, k, false);
                    self.render_backup_input(&mut display.framebuf, &hrp_display_string);
                    touched_key = None;
                    display.flush().unwrap();
                }
            }
        }
    }
}
