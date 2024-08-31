extern crate alloc;

use crate::graphics::Graphics;
use crate::graphics::{FONT_LARGE, FONT_MED, PADDING_TOP};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use cst816s::TouchGesture;
use cst816s::CST816S;
use embedded_graphics::image::Image;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
    text::{Alignment, Text, TextStyleBuilder},
};
use embedded_iconoir::{icons::size24px::actions::Check, prelude::IconoirNewIcon};

use embedded_graphics_framebuf::FrameBuf;
use embedded_hal as hal;
use esp_hal::timer::{self, timg::Timer};
use esp_hal::Blocking;
use frostsnap_core::schnorr_fun::frost::SecretShare;
use fugit::Instant;
use mipidsi::error::Error;
use u8g2_fonts::U8g2TextStyle;

const KEY_WIDTH: u32 = 60;
const KEY_HEIGHT: u32 = 50;
const KEYROWS_SHOWN: usize = 4;
const BACKUP_LEFT_PADDING: u32 = 5;

// for convenience: framebuffer.height() - 2 * KEY_HEIGHT;
const KEYBOARD_START_HEIGHT: u32 = 280 - PADDING_TOP - (KEYROWS_SHOWN as u32) * (KEY_HEIGHT as u32);

const KEYBOARD_KEYS: [[char; 4]; 8] = [
    ['2', '3', '4', '5'],
    ['6', '7', '8', '9'],
    ['0', 'A', 'C', 'D'],
    ['E', 'F', 'G', 'H'],
    ['J', 'K', 'L', 'M'],
    ['N', 'P', 'Q', 'R'],
    ['S', 'T', 'U', 'V'],
    ['W', 'X', 'Y', 'Z'],
];

const KEYBOARD_KEYS_NUMBERS: [[char; 4]; 3] = [
    ['1', '2', '3', '4'],
    ['5', '6', '7', '8'],
    ['9', '0', '✓', '✓'],
];

#[derive(Default, Debug, Clone)]
pub struct Keyboard {
    buffer: Vec<char>,
    entered_hrp_index: Option<String>,
    last_touch: Option<Instant<u64, 1, 1_000_000>>,
    touched_key: Option<(usize, usize)>,
    top_row_index: usize,
    init_rendered: bool,
}

#[derive(Debug, Clone)]
pub enum EnteredBackupStatus {
    Valid(SecretShare),
    Invalid(String),
}

impl Keyboard {
    fn get_key_from_indicies(&self, (y, x): (usize, usize)) -> (Option<Rectangle>, char) {
        let wrapped_keyboard_y = if self.entered_hrp_index.is_some() {
            (y + KEYBOARD_KEYS.len() - self.top_row_index) % KEYBOARD_KEYS.len()
        } else {
            y
        };
        let rect = if wrapped_keyboard_y < KEYROWS_SHOWN {
            Some(Rectangle::new(
                Point::new(
                    x as i32 * KEY_WIDTH as i32,
                    (KEYBOARD_START_HEIGHT + (wrapped_keyboard_y as u32 + 1) * KEY_HEIGHT) as i32,
                ),
                Size::new(KEY_WIDTH, KEY_HEIGHT),
            ))
        } else {
            None
        };

        let char = if self.entered_hrp_index.is_some() {
            debug_assert!(y < KEYBOARD_KEYS.len());
            debug_assert!(x < KEYBOARD_KEYS[y].len());
            KEYBOARD_KEYS[y][x]
        } else {
            KEYBOARD_KEYS_NUMBERS[y][x]
        };

        (rect, char)
    }

    fn get_key_from_touch(&self, (y, x): (i32, i32)) -> Option<(usize, usize)> {
        if y < (KEYBOARD_START_HEIGHT + KEY_HEIGHT) as i32 {
            return None;
        }

        let mut row =
            (((y as u32).saturating_sub(KEYBOARD_START_HEIGHT + KEY_HEIGHT)) / KEY_HEIGHT) as usize;
        if self.entered_hrp_index.is_some() {
            row = row + self.top_row_index % KEYBOARD_KEYS.len();
        }
        let col = ((x as u32 - 0) / KEY_WIDTH) as usize;

        let keyboard_bounds = if self.entered_hrp_index.is_some() {
            (KEYBOARD_KEYS.len(), KEYBOARD_KEYS[0].len())
        } else {
            (KEYBOARD_KEYS_NUMBERS.len(), KEYBOARD_KEYS_NUMBERS[0].len())
        };

        if row < keyboard_bounds.0 && col < keyboard_bounds.1 {
            Some((row, col))
        } else {
            None
        }
    }

    pub fn new() -> Self {
        Self {
            buffer: vec![],
            entered_hrp_index: None,
            last_touch: None,
            touched_key: None,
            top_row_index: 0,
            init_rendered: false,
        }
    }

    pub fn reset_keyboard(&mut self) {
        *self = Self::new();
    }

    pub fn entered_backup_validity(&mut self) -> Option<EnteredBackupStatus> {
        let backup_input = self.buffer.clone().into_iter().collect::<String>();
        if backup_input.len() < 59 {
            return None;
        }

        match &self.entered_hrp_index {
            None => None,
            Some(hrp) => {
                let mut backup_string = hrp.clone();
                backup_string.push_str(&backup_input);
                match SecretShare::from_bech32_backup(&backup_string) {
                    Ok(share_backup) => {
                        self.reset_keyboard();
                        Some(EnteredBackupStatus::Valid(share_backup))
                    }
                    Err(_) => Some(EnteredBackupStatus::Invalid(backup_string)),
                }
            }
        }
    }

    fn clear_keyboard(&mut self, framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>) {
        Rectangle::new(
            Point::new(0, (framebuf.height() as u32 - 2 * KEY_HEIGHT) as i32),
            Size::new(framebuf.width() as u32, 2 * KEY_HEIGHT),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(Rgb565::BLACK)
                .build(),
        )
        .draw(framebuf)
        .unwrap();
    }

    fn render_character_key(
        &mut self,
        framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>,
        key_position: (usize, usize),
        is_active: bool,
    ) {
        let (rect, char) = self.get_key_from_indicies(key_position);
        let rect = rect.expect(&format!(
            "should be on screen if we are rendering it.. {}",
            char
        ));

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

            if char == '✓' {
                let icon = Check::new(Rgb565::GREEN);
                Image::with_center(&icon, rect.center())
                    .draw(framebuf)
                    .unwrap();
            } else {
                Text::with_text_style(
                    &char.to_string(),
                    rect.center(),
                    font,
                    TextStyleBuilder::new().alignment(Alignment::Center).build(),
                )
                .draw(framebuf)
                .unwrap();
            };
        }
    }

    fn render_backup_input(&mut self, framebuf: &mut FrameBuf<Rgb565, &mut [Rgb565; 67200]>) {
        let mut y_offset = 0;
        let spacing_size = 20;

        let text_color = match self.entered_backup_validity() {
            Some(validity) => match validity {
                EnteredBackupStatus::Valid(_) => Rgb565::WHITE,
                EnteredBackupStatus::Invalid(_) => Rgb565::RED,
            },
            None => Rgb565::WHITE,
        };

        // clear area
        let rect = Rectangle::new(
            Point::new(0, PADDING_TOP as i32),
            Size::new(
                framebuf.width() as u32,
                framebuf.height() as u32 - PADDING_TOP - 2 * KEY_HEIGHT,
            ),
        );
        rect.into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(Rgb565::BLACK)
                .build(),
        )
        .draw(framebuf)
        .unwrap();

        if self.entered_hrp_index.is_none() {
            let pending_hrp = format!(
                "frost[{}]",
                self.buffer.clone().into_iter().collect::<String>()
            );
            Text::with_text_style(
                &pending_hrp,
                Point::new((framebuf.width() / 2) as i32, PADDING_TOP as i32),
                U8g2TextStyle::new(FONT_LARGE, text_color),
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(embedded_graphics::text::Baseline::Top)
                    .build(),
            )
            .draw(framebuf)
            .unwrap();
        } else {
            let chunked_backup = self.buffer.clone().into_iter().fold(
                vec!["".to_string()],
                |mut chunk_vec, char| {
                    if chunk_vec.last().unwrap().len() < 4 {
                        let last = chunk_vec.last_mut().unwrap();
                        last.push(char);
                    } else {
                        chunk_vec.push(char.to_string());
                    }
                    chunk_vec
                },
            );

            Text::with_baseline(
                chunked_backup[chunked_backup.len().saturating_sub(3)..]
                    .join(" ")
                    .as_ref(),
                Point::new(BACKUP_LEFT_PADDING as i32, (PADDING_TOP as i32) + y_offset),
                U8g2TextStyle::new(FONT_LARGE, text_color),
                embedded_graphics::text::Baseline::Top,
            )
            .draw(framebuf)
            .unwrap();
        }
    }

    pub fn render_backup_keyboard<
        DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
    >(
        &mut self,
        display: &mut Graphics<'_, DT>,
        proposed_share_index: Option<u32>,
    ) {
        self.render_backup_input(&mut display.framebuf);
        self.clear_keyboard(&mut display.framebuf);

        if self.entered_hrp_index.is_none() {
            for y_row in 0..KEYBOARD_KEYS_NUMBERS.len() {
                for x_pos in 0..KEYBOARD_KEYS_NUMBERS[y_row].len() {
                    self.render_character_key(&mut display.framebuf, (y_row, x_pos), false);
                }
            }
        } else {
            let top_index = self.top_row_index;
            for y_row in (0..KEYROWS_SHOWN).map(|i| {
                let row = (top_index + i) % KEYBOARD_KEYS.len();
                row
            }) {
                for x_pos in 0..KEYBOARD_KEYS[y_row].len() {
                    self.render_character_key(&mut display.framebuf, (y_row, x_pos), false);
                }
            }
        }

        if let Some(key) = &self.touched_key {
            self.render_character_key(&mut display.framebuf, key.clone(), true);
        }
    }

    pub fn poll_input<
        T: timer::timg::Instance,
        CommE,
        PinE,
        I2C: hal::i2c::I2c<Error = CommE>,
        PINT: hal::digital::InputPin,
        RST: hal::digital::StatefulOutputPin<Error = PinE>,
    >(
        &mut self,
        capsense: &mut CST816S<I2C, PINT, RST>,
        timer: &'_ Timer<T, Blocking>,
    ) -> bool {
        let now = timer::Timer::now(timer);
        let mut is_changes = {
            match capsense.read_one_touch_event(true) {
                None => match self.last_touch {
                    None => false,
                    Some(last_touch) => {
                        if now.checked_duration_since(last_touch).unwrap().to_millis() < 25 {
                            true
                        } else {
                            self.touched_key = None;
                            self.last_touch = None;
                            true
                        }
                    }
                },
                Some(touch) => {
                    // Gestures
                    match (&touch.gesture, touch.action) {
                        // Backspace
                        (TouchGesture::SlideLeft, 1) => {
                            if self.buffer.len() > 0 {
                                self.buffer.pop();
                            } else {
                                if let Some(hrp) = &self.entered_hrp_index {
                                    self.buffer = hrp.chars().collect();
                                    self.entered_hrp_index = None;
                                }
                            }
                            true
                        }
                        // Slide up/down to jog through 8-key groups
                        (TouchGesture::SlideDown, 1) => {
                            self.top_row_index = (self.top_row_index
                                + (KEYBOARD_KEYS.len() - KEYROWS_SHOWN))
                                % KEYBOARD_KEYS.len();
                            true
                        }
                        // /* Useful for quick testing */
                        // (TouchGesture::SlideRight, 1) => {
                        //     self.buffer =
                        //         "162zh846g3zp67zh3mqvq7kfcahefpdpw2v09rjegtrakrw0hynyqfgwk2"
                        //             .chars()
                        //             .collect();
                        //     true
                        // }
                        (TouchGesture::SlideUp, 1) => {
                            self.top_row_index =
                                (self.top_row_index + KEYROWS_SHOWN) % KEYBOARD_KEYS.len();
                            true
                        }
                        (TouchGesture::SingleClick, _) => {
                            // Find the key being touched
                            if let Some(key_position) = self.get_key_from_touch((touch.y, touch.x))
                            {
                                let (_rect, c) = self.get_key_from_indicies(key_position);
                                _rect.expect("should be on screen if we touched it...");
                                if self.touched_key.is_none() {
                                    if c == '✓' {
                                        // special case where we finish entering hrp
                                        self.entered_hrp_index = Some(format!(
                                            "{}",
                                            self.buffer.clone().into_iter().collect::<String>()
                                        ));
                                        self.touched_key = None;
                                        self.buffer.clear();
                                    } else {
                                        self.touched_key = Some(key_position);
                                        self.last_touch = Some(now);
                                        self.buffer.push(c);
                                    }
                                } else {
                                    self.touched_key = Some(key_position);
                                    self.last_touch = Some(now);
                                }
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

        if !self.init_rendered {
            is_changes = true;
            self.init_rendered = true;
        }

        is_changes
    }
}
