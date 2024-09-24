use super::key_touch::KeyTouch;
use super::{icons, FONT_LARGE};
use crate::graphics::palette::COLORS;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use embedded_graphics::framebuffer::{buffer_size, Framebuffer};
use embedded_graphics::geometry::AnchorX;
use embedded_graphics::pixelcolor::raw::{LittleEndian, RawU1};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::{image::GetPixel, pixelcolor::Rgb565, primitives::Rectangle};
use fugit::Instant;
use micromath::F32Ext;
use u8g2_fonts::U8g2TextStyle;

const N_CHARACTERS: u32 = 15 * 4 - 2;
const GAP_WIDTH: u32 = 10;
const FRAMEBUFFER_WIDTH: u32 = Bech32Framebuf::position_for_character(N_CHARACTERS);
const FONT_SIZE: Size = Size::new(16, 24);

#[derive(Debug)]
pub struct Bech32InputPreview {
    init_draw: bool,
    backspace_rect: Rectangle,
    preview_rect: Rectangle,
    n_characters: usize,
    framebuf: Bech32Framebuf,
}

impl Bech32InputPreview {
    // Create a new FrostShareInput instance
    pub fn new(visible_area: Size, n_characters: usize) -> Self {
        let usable_width = visible_area.width;
        let backspace_width = usable_width / 4;
        let backspace_rect = Rectangle::new(
            Point::new(usable_width as i32 - backspace_width as i32, 0),
            Size {
                width: backspace_width,
                height: visible_area.height,
            },
        );

        let preview_width = usable_width - backspace_rect.size.width;

        let preview_rect = Rectangle::new(
            Point::new(
                0,
                (visible_area.height as i32 - FONT_SIZE.height as i32) / 2,
            ),
            Size {
                width: preview_width,
                height: FONT_SIZE.height,
            },
        );
        Bech32InputPreview {
            init_draw: false,
            backspace_rect,
            preview_rect,
            n_characters,
            framebuf: Bech32Framebuf::new(),
        }
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        if self.backspace_rect.contains(point) {
            Some(KeyTouch::new('⌫', self.backspace_rect))
        } else {
            None
        }
    }

    // Draw the input area with the current characters
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: Instant<u64, 1, 1_000_000>,
    ) {
        if !self.init_draw {
            let _ = target.clear(COLORS.background);
            icons::backspace()
                .with_color(Rgb565::RED)
                // shift the icon over to the left of the backspace rectangle
                .with_center(
                    self.backspace_rect
                        .resized_width(self.backspace_rect.size.width / 2, AnchorX::Left)
                        .center(),
                )
                .draw(target);

            self.init_draw = true;
        }

        self.framebuf
            .draw(&mut target.cropped(&self.preview_rect), current_time);
    }

    // Method to add a character and start animation if needed
    pub fn add_character(&mut self, c: char) {
        self.framebuf.add_character(c);
    }

    pub fn get_input(&self) -> &str {
        self.framebuf.characters.as_str()
    }

    pub fn set_input_color(&mut self, color: Rgb565) {
        self.framebuf.change_color(color);
    }

    pub fn is_finished(&self) -> bool {
        self.framebuf.characters.len() == self.n_characters
    }
}

type Fb = Framebuffer<
    BinaryColor,
    RawU1,
    LittleEndian,
    { FRAMEBUFFER_WIDTH as usize },
    { FONT_SIZE.height as usize },
    { buffer_size::<BinaryColor>(FRAMEBUFFER_WIDTH as usize, FONT_SIZE.height as usize) },
>;

#[derive(Debug)]
pub struct Bech32Framebuf {
    framebuffer: Box<Fb>,
    characters: String,
    current_position: u32,
    current_time: Option<Instant<u64, 1, 1_000_000>>,
    target_position: u32,
    color: Rgb565,
    redraw: bool,
}

impl Default for Bech32Framebuf {
    fn default() -> Self {
        Self::new()
    }
}

impl Bech32Framebuf {
    pub fn new() -> Self {
        Self {
            framebuffer: Box::new(Fb::new()),
            characters: Default::default(),
            current_position: 0,
            current_time: None,
            target_position: 0,
            redraw: true,
            color: COLORS.primary,
        }
    }

    pub fn change_color(&mut self, color: Rgb565) {
        let changed = color != self.color;
        self.color = color;
        self.redraw = changed;
    }

    pub fn draw(
        &mut self,
        target: &mut impl DrawTarget<Color = Rgb565>,
        current_time: Instant<u64, 1, 1_000_000>,
    ) {
        let last_draw_time = self.current_time.get_or_insert(current_time);
        if self.current_position == self.target_position && !self.redraw {
            *last_draw_time = current_time;
            return;
        }
        let duration_millis = current_time
            .checked_duration_since(*last_draw_time)
            .unwrap()
            .to_millis();
        const VELOCITY: f32 = 0.05; // pixels per ms

        let distance = (duration_millis as f32 * VELOCITY).round() as i32;
        if distance == 0 && !self.redraw {
            return;
        }
        *last_draw_time = current_time;

        let direction = self.target_position as i32 - self.current_position as i32;
        let traveled = direction.clamp(-distance, distance);
        self.current_position = ((self.current_position as i32) + traveled)
            .try_into()
            .expect("shouldn't be negative");
        let width = target.bounding_box().size.width;

        let window_start = self.current_position.saturating_sub(width) as usize;
        let window_width = width.min(self.current_position);
        let left_padding = core::iter::repeat(COLORS.background)
            .take(width.saturating_sub(self.current_position) as usize);
        let fb = &self.framebuffer;
        let color = self.color;
        let iterator = (0..target.bounding_box().size.height).flat_map(|y| {
            let start = window_start;
            let end = window_start + window_width as usize;

            left_padding.clone().chain((start..end).map(move |x| {
                match fb.pixel(Point::new(x as i32, y as i32)).unwrap() {
                    BinaryColor::Off => COLORS.background,
                    BinaryColor::On => color,
                }
            }))
        });

        let _ = target.fill_contiguous(&target.bounding_box(), iterator);
        self.redraw = false;
    }

    pub fn add_character(&mut self, c: char) {
        if c == '⌫' {
            self.backspace();
            return;
        }
        self.characters.push(c);

        let character_pos = Self::position_for_character(self.characters.len() as u32 - 1);
        let _ = Text::with_text_style(
            &c.to_string(),
            Point::new(character_pos as i32, 0),
            U8g2TextStyle::new(FONT_LARGE, BinaryColor::On),
            TextStyleBuilder::new()
                .alignment(Alignment::Left)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(self.framebuffer.as_mut());

        self.target_position = Self::position_for_character(self.characters.len() as u32);
    }

    pub fn backspace(&mut self) {
        if self.characters.is_empty() {
            return;
        }
        self.characters.pop();
        let deleted_character_pos = Self::position_for_character(self.characters.len() as u32);

        let _ = self
            .framebuffer
            .cropped(&Rectangle::new(
                Point::new(deleted_character_pos as i32, 0),
                FONT_SIZE,
            ))
            .clear(BinaryColor::Off);

        self.target_position = Self::position_for_character(self.characters.len() as u32);
    }

    const fn position_for_character(index: u32) -> u32 {
        index * FONT_SIZE.width + (index / 4) * GAP_WIDTH
    }
}
