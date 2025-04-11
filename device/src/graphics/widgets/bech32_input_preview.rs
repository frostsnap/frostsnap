use super::key_touch::KeyTouch;
use super::{icons, FONT_LARGE};
use crate::graphics::palette::COLORS;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use embedded_graphics::framebuffer::{buffer_size, Framebuffer};
use embedded_graphics::geometry::AnchorX;
use embedded_graphics::pixelcolor::raw::{LittleEndian, RawU2};
use embedded_graphics::pixelcolor::Gray2;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};
use embedded_graphics::{image::GetPixel, pixelcolor::Rgb565, primitives::Rectangle};
use micromath::F32Ext;
use u8g2_fonts::U8g2TextStyle;

const N_CHARACTERS: usize = 15 * 4 - 2;
const GAP_WIDTH: u32 = 10;
const FRAMEBUFFER_WIDTH: u32 = Bech32Framebuf::position_for_character(N_CHARACTERS);
const FONT_SIZE: Size = Size::new(16, 24);
const N_CHUNKS: usize = N_CHARACTERS.div_ceil(4);

#[derive(Debug)]
pub struct Bech32InputPreview {
    init_draw: bool,
    backspace_rect: Rectangle,
    preview_rect: Rectangle,
    n_characters: usize,
    framebuf: Bech32Framebuf,
    progress: ProgressBars,
    progress_rect: Rectangle,
}

impl Bech32InputPreview {
    // Create a new FrostShareInput instance
    pub fn new(visible_area: Size, n_characters: usize) -> Self {
        let usable_width = visible_area.width;
        let backspace_width = usable_width / 4;
        let progress_height = 4;
        let backspace_rect = Rectangle::new(
            Point::new(usable_width as i32 - backspace_width as i32, 0),
            Size {
                width: backspace_width,
                height: visible_area.height - progress_height,
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

        let progress_rect = Rectangle::new(
            Point::new(0, visible_area.height as i32 - progress_height as i32),
            Size::new(visible_area.width, progress_height),
        );
        let progress = ProgressBars::new(N_CHUNKS);

        Bech32InputPreview {
            init_draw: false,
            backspace_rect,
            preview_rect,
            n_characters,
            progress_rect,
            progress,
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

    fn update_progress(&mut self) {
        let progress = if self.framebuf.characters.len() == N_CHARACTERS {
            N_CHUNKS
        } else {
            self.framebuf.characters.len() / 4
        };
        self.progress.progress(progress);
    }

    // Draw the input area with the current characters
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        if !self.init_draw {
            let _ = target.clear(COLORS.background);
            icons::backspace()
                .with_color(Rgb565::new(31, 20, 12))
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

        let _ = self.progress.draw(&mut target.cropped(&self.progress_rect));
    }

    // Method to add a character and start animation if needed
    pub fn add_character(&mut self, c: char) {
        if c == '⌫' {
            self.framebuf.backspace();
        } else {
            self.framebuf.add_character(c);
        }
        self.update_progress();
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
    Gray2,
    RawU2,
    LittleEndian,
    { FRAMEBUFFER_WIDTH as usize },
    { FONT_SIZE.height as usize },
    { buffer_size::<Gray2>(FRAMEBUFFER_WIDTH as usize, FONT_SIZE.height as usize) },
>;

#[derive(Debug)]
pub struct Bech32Framebuf {
    framebuffer: Box<Fb>,
    characters: String,
    current_position: u32,
    current_time: Option<crate::Instant>,
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
        let mut self_ = Self {
            framebuffer: Box::new(Fb::new()),
            characters: Default::default(),
            current_position: Self::chunk_end_for_character(0),
            current_time: None,
            target_position: Self::chunk_end_for_character(0),
            redraw: true,
            color: COLORS.primary,
        };

        for i in 0..N_CHARACTERS {
            self_.clear_character(i, i == 0);
        }
        self_
    }

    pub fn change_color(&mut self, color: Rgb565) {
        let changed = color != self.color;
        self.color = color;
        self.redraw = self.redraw || changed;
    }

    pub fn draw(
        &mut self,
        target: &mut impl DrawTarget<Color = Rgb565>,
        current_time: crate::Instant,
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
        let left_padding = core::iter::repeat_n(
            COLORS.background,
            width.saturating_sub(self.current_position) as usize,
        );
        let fb = &self.framebuffer;
        let color = self.color;
        let iterator = (0..target.bounding_box().size.height).flat_map(|y| {
            let start = window_start;
            let end = window_start + window_width as usize;

            left_padding.clone().chain((start..end).map(move |x| {
                match fb.pixel(Point::new(x as i32, y as i32)).unwrap().luma() {
                    0x00 => COLORS.background,
                    0x01 => Rgb565::new(20, 41, 22),
                    0x02 => color,
                    0x03 => color,
                    _ => unreachable!(),
                }
            }))
        });

        target
            .fill_contiguous(&target.bounding_box(), iterator)
            .map_err(|_| ())
            .unwrap();
        self.redraw = false;
    }

    fn clear_character(&mut self, index: usize, is_it_next: bool) {
        if index >= N_CHARACTERS {
            return;
        }
        let mut character_frame = self.character_frame(index);
        let _ = character_frame.clear(Gray2::BLACK);
        let _ = Text::with_text_style(
            if is_it_next { "_" } else { "-" },
            Point::zero(),
            U8g2TextStyle::new(
                FONT_LARGE,
                if is_it_next {
                    Gray2::WHITE
                } else {
                    Gray2::new(0x01)
                },
            ),
            TextStyleBuilder::new()
                .alignment(Alignment::Left)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(&mut character_frame);
    }

    fn character_frame(&mut self, index: usize) -> impl DrawTarget<Color = Gray2> + '_ {
        let character_pos = Self::position_for_character(index);
        self.framebuffer.cropped(&Rectangle::new(
            Point::new(character_pos as i32, 0),
            FONT_SIZE,
        ))
    }

    pub fn add_character(&mut self, c: char) {
        if self.characters.len() >= N_CHARACTERS {
            return;
        }
        self.characters.push(c);
        let mut character_frame = self.character_frame(self.characters.len() - 1);
        let _ = character_frame.clear(Gray2::BLACK);

        let _ = Text::with_text_style(
            &c.to_string(),
            Point::zero(),
            U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
            TextStyleBuilder::new()
                .alignment(Alignment::Left)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(&mut character_frame);
        drop(character_frame);

        self.clear_character(self.characters.len(), true);
        self.target_position = Self::chunk_end_for_character(self.characters.len());
        self.redraw = true;
    }

    pub fn backspace(&mut self) {
        if self.characters.is_empty() {
            return;
        }
        self.characters.pop();
        self.clear_character(self.characters.len(), true);
        self.clear_character(self.characters.len() + 1, false);

        self.target_position = Self::chunk_end_for_character(self.characters.len());
        self.redraw = true;
    }

    const fn position_for_character(index: usize) -> u32 {
        index as u32 * FONT_SIZE.width + (index as u32 / 4) * GAP_WIDTH
    }

    const fn chunk_end_for_character(index: usize) -> u32 {
        let chunk_index = index as u32 / 4;
        let chunk_width = 4 * FONT_SIZE.width;
        let on_last_chunk = index / 4 == N_CHUNKS - 1;
        let current_chunk_width = if on_last_chunk {
            (N_CHARACTERS as u32 % 4) * FONT_SIZE.width
        } else {
            chunk_width
        };

        chunk_index * (chunk_width + GAP_WIDTH) + current_chunk_width
    }
}

#[derive(Debug)]
pub struct ProgressBars {
    total_bar_number: usize,
    progress: usize,
    redraw: bool,
}

impl ProgressBars {
    pub fn new(total_bar_number: usize) -> Self {
        Self {
            total_bar_number,
            progress: 0,
            redraw: true,
        }
    }

    pub fn progress(&mut self, progress: usize) {
        self.redraw = self.redraw || progress != self.progress;
        self.progress = progress;
    }
}

impl Drawable for ProgressBars {
    type Color = Rgb565;
    type Output = ();

    fn draw<D: DrawTarget<Color = Self::Color>>(&self, display: &mut D) -> Result<(), D::Error> {
        const GAP_WIDTH: u32 = 4; // Gap between bars
        let size = display.bounding_box().size;

        if self.redraw {
            let bar_width = (size.width - (self.total_bar_number as u32 - 1) * GAP_WIDTH)
                / self.total_bar_number as u32;
            let bar_height = size.height;

            for i in 0..self.total_bar_number {
                let x_offset = i as u32 * (bar_width + GAP_WIDTH);

                let color = if i < self.progress {
                    Rgb565::new(8, 49, 16) // Draw green for progress
                } else {
                    Rgb565::new(16, 32, 16) // Draw grey for remaining bars
                };

                // Define the rectangle for the bar
                let bar = Rectangle::new(
                    Point::new(x_offset as i32, 0),
                    Size::new(bar_width, bar_height),
                );

                // Draw the bar
                bar.into_styled(PrimitiveStyle::with_fill(color))
                    .draw(display)?;
            }
        }

        Ok(())
    }
}
