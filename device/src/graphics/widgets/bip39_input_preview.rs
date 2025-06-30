use super::{icons, KeyTouch, FONT_LARGE};
use crate::bip39_words;
use crate::graphics::palette::COLORS;

use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    geometry::AnchorX,
    image::GetPixel,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use micromath::F32Ext;
use u8g2_fonts::U8g2TextStyle;

#[derive(Debug)]
struct Cursor {
    visible: bool,
    last_toggle: Option<crate::Instant>,
    pub position: Point,
}

impl Cursor {
    fn new(position: Point) -> Self {
        Self {
            visible: true,
            last_toggle: None,
            position,
        }
    }

    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        // Update visibility based on time
        let cursor_rect = Rectangle::new(
            Point::new(
                self.position.x,
                self.position.y + FONT_SIZE.height as i32 - 4,
            ),
            Size::new(FONT_SIZE.width - 4, 2),
        );

        if let Some(last_toggle) = self.last_toggle {
            // Check if 600ms has passed since last toggle
            if current_time
                .checked_duration_since(last_toggle)
                .map(|d| d.to_millis() >= 600)
                .unwrap_or(false)
            {
                self.visible = !self.visible;
                self.last_toggle = Some(current_time);

                // Draw or clear based on new visibility state
                if self.visible {
                    let _ = cursor_rect
                        .into_styled(PrimitiveStyle::with_fill(COLORS.primary))
                        .draw(target);
                } else {
                    let _ = cursor_rect
                        .into_styled(PrimitiveStyle::with_fill(COLORS.background))
                        .draw(target);
                }
            }
        } else {
            // First time - draw cursor
            self.last_toggle = Some(current_time);
            let _ = cursor_rect
                .into_styled(PrimitiveStyle::with_fill(COLORS.primary))
                .draw(target);
        }
    }
}

// Constants for BIP39 word display
const MAX_CHARS: usize = 25 * 8;
const FONT_SIZE: Size = Size::new(16, 24);
const FRAMEBUFFER_WIDTH: u32 = MAX_CHARS as u32 * FONT_SIZE.width as u32;

#[derive(Debug, Default)]
struct AutocompleteDisplay {
    suggestion: Option<(&'static str, u8)>,
    redraw: bool,
}

impl AutocompleteDisplay {
    fn new() -> Self {
        Self {
            suggestion: None,
            redraw: true,
        }
    }

    fn update(&mut self, current_word: &str) {
        self.redraw = true;

        if current_word.is_empty() {
            self.suggestion = None;
        } else {
            // Word list is uppercase, current_word is already uppercase
            if let Some(suggestion) = bip39_words::first_word_with_prefix(current_word) {
                self.suggestion = Some((suggestion, (current_word.len() as u8)));
            } else {
                self.suggestion = None;
            }
        }
    }

    fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) -> Result<(), D::Error> {
        if !self.redraw {
            return Ok(());
        }
        if let &Some((suggestion, pos)) = &self.suggestion {
            Text::with_text_style(
                &suggestion[pos as usize..],
                Point::new(pos as i32 * FONT_SIZE.width as i32, 0),
                U8g2TextStyle::new(FONT_LARGE, Rgb565::new(10, 20, 10)),
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Top)
                    .build(),
            )
            .draw(target)?;
        } else {
            target.clear(COLORS.background);
        }
        self.redraw = false;
        Ok(())
    }

    fn get_suggestion(&self) -> Option<&'static str> {
        self.suggestion.map(|(suggestion, _)| suggestion)
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
pub struct Bip39InputPreview {
    area: Rectangle,
    preview_rect: Rectangle,
    backspace_rect: Rectangle,
    progress_rect: Rectangle,
    progress: Bip39ProgressBars,
    framebuf: Bip39Framebuf,
    init_draw: bool,
}

impl Bip39InputPreview {
    pub fn new(area: Rectangle) -> Self {
        let progress_height = 4;
        let backspace_width = area.size.width / 4;
        let backspace_rect = Rectangle::new(
            Point::new(area.size.width as i32 - backspace_width as i32, 0),
            Size {
                width: backspace_width,
                height: area.size.height - progress_height,
            },
        );

        let preview_rect = Rectangle::new(
            Point::new(
                0,
                ((area.size.height - progress_height) as i32 - FONT_SIZE.height as i32) / 2,
            ),
            Size {
                width: area.size.width - backspace_width,
                height: FONT_SIZE.height,
            },
        );

        let progress_rect = Rectangle::new(
            Point::new(0, area.size.height as i32 - progress_height as i32),
            Size::new(area.size.width, progress_height),
        );

        // 24 words maximum for BIP39
        let progress = Bip39ProgressBars::new(24);
        let framebuf = Bip39Framebuf::new();

        // Position cursor at fixed location relative to preview area
        let _cursor_position = Point::new(backspace_rect.top_left.x - WORD_START as i32, 0);

        Self {
            area,
            preview_rect,
            backspace_rect,
            progress_rect,
            progress,
            framebuf,
            init_draw: false,
        }
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        if self.backspace_rect.contains(point) {
            Some(KeyTouch::new('⌫', self.backspace_rect))
        } else {
            None
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        // Draw backspace icon on first draw
        if !self.init_draw {
            // Clear the entire area first
            let clear_rect = Rectangle::new(Point::zero(), self.area.size);
            let _ = clear_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.background)
                        .build(),
                )
                .draw(target);

            icons::backspace()
                .with_color(Rgb565::new(31, 20, 12))
                .with_center(
                    self.backspace_rect
                        .resized_width(self.backspace_rect.size.width / 2, AnchorX::Left)
                        .center(),
                )
                .draw(target);
            self.init_draw = true;
        }

        // Always draw the framebuffer (it has its own redraw logic)
        self.framebuf
            .draw(&mut target.cropped(&self.preview_rect), current_time);

        // // Draw cursor at fixed position only if we haven't entered all 24 words
        // if self.words.len() < MAX_WORDS {
        //     self.cursor
        //         .draw(&mut target.cropped(&self.preview_rect), current_time);
        // }

        // Always draw progress bars (they have their own redraw logic)
        let _ = self.progress.draw(&mut target.cropped(&self.progress_rect));
    }

    pub fn push_letter(&mut self, letter: char) {
        // Add uppercase letter to framebuffer
        let upper_letter = letter.to_uppercase().next().unwrap_or(letter);
        self.framebuf.add_character(upper_letter);

        // Get current word from characters string
        let current_word = self.framebuf.current_word();

        // Check if the current word is now a complete valid BIP39 word
        if bip39_words::is_valid_bip39_word(current_word) {
            self.framebuf.mark_word_boundary();
            self.update_progress();
        }
    }

    pub fn backspace(&mut self) {
        let current_word = self.framebuf.current_word();
        if current_word.is_empty() && self.framebuf.word_count() > 0 {
            // If no current word, we're at a word boundary
            // Remove the space first
            self.framebuf.backspace();
            self.update_progress();
        } else if !current_word.is_empty() {
            self.framebuf.backspace();
        }
    }

    pub fn accept_word(&mut self) {
        let current_word = self.framebuf.current_word();
        if !current_word.is_empty() {
            self.framebuf.mark_word_boundary();
            self.update_progress();
        }
    }

    fn update_progress(&mut self) {
        // Update progress based on number of words entered (1 bar per word)
        self.progress.progress(self.framebuf.word_count());
    }

    pub fn try_accept_autocomplete(&mut self) -> bool {
        let current_word = self.framebuf.current_word();
        if !current_word.is_empty() {
            if let Some(suggestion) = self.framebuf.autocomplete.get_suggestion() {
                // Complete the word with the suggestion
                let remaining = &suggestion[current_word.len()..];
                for c in remaining.chars() {
                    let upper_c = c.to_uppercase().next().unwrap_or(c);
                    self.framebuf.add_character(upper_c);
                }

                // Accept the completed word
                self.framebuf.mark_word_boundary();
                self.update_progress();
                return true;
            }
        }
        false
    }

    pub fn contains(&self, point: Point) -> bool {
        self.preview_rect.contains(point)
    }

    pub fn has_current_word(&self) -> bool {
        !self.framebuf.current_word().is_empty()
    }

    pub fn can_accept_letter(&self, letter: char) -> bool {
        let current_word = self.framebuf.current_word();
        // Convert to uppercase since our word list is uppercase
        let upper_letter = letter.to_uppercase().next().unwrap_or(letter);
        let potential_word = format!("{}{}", current_word, upper_letter);
        
        // Check if any BIP39 word starts with this prefix
        bip39_words::first_word_with_prefix(&potential_word).is_some()
    }

    pub fn is_finished(&self) -> bool {
        self.framebuf.word_count() == 24
    }

    pub fn get_mnemonic(&self) -> String {
        self.framebuf.characters.clone()
    }
}

#[derive(Debug)]
pub struct Bip39ProgressBars {
    total_bar_number: usize,
    progress: usize,
    redraw: bool,
}

impl Bip39ProgressBars {
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

impl Bip39ProgressBars {
    fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, display: &mut D) -> Result<(), D::Error> {
        if !self.redraw {
            return Ok(());
        }

        const GAP_WIDTH: u32 = 2; // Smaller gap for 24 bars
        let size = display.bounding_box().size;

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

        self.redraw = false;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Bip39Framebuf {
    framebuffer: Box<Fb>,
    characters: String,
    current_position: u32,
    current_time: Option<crate::Instant>,
    target_position: u32,
    color: Rgb565,
    redraw: bool,
    autocomplete: AutocompleteDisplay,
}

const WORD_START: u32 = 120;

impl Bip39Framebuf {
    pub fn new() -> Self {
        let mut framebuffer = Box::new(Fb::new());
        // Clear the framebuffer
        let _ = framebuffer.clear(Gray2::BLACK);

        let self_ = Self {
            framebuffer,
            characters: Default::default(),
            current_position: WORD_START, // Start at WORD_START to avoid initial animation
            current_time: None,
            target_position: WORD_START,
            redraw: true,
            color: COLORS.primary,
            autocomplete: AutocompleteDisplay::new(),
        };

        self_
    }

    pub fn add_character(&mut self, c: char) {
        self.characters.push(c);
        // Update autocomplete with just the current word
        // Get current word as String to avoid borrow issues
        self.autocomplete
            .update(self.characters.split_whitespace().last().unwrap_or(""));
        let char_index = self.characters.len() - 1;
        let char_pos = Self::position_for_character_in_framebuf(char_index);

        // Draw the character in the framebuffer
        let mut char_frame = self.framebuffer.cropped(&Rectangle::new(
            Point::new(char_pos as i32, 0),
            Size::new(FONT_SIZE.width, FONT_SIZE.height),
        ));

        let _ = char_frame.clear(Gray2::BLACK);
        let _ = Text::with_text_style(
            &c.to_string(),
            Point::zero(),
            U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
            TextStyleBuilder::new()
                .alignment(Alignment::Left)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(&mut char_frame);

        // Update target position so next character appears at fixed position
        // let next_char_pos = Self::position_for_character(self.characters.len());
        // self.target_position = 0;
        self.redraw = true;
    }

    pub fn mark_word_boundary(&mut self) {
        // Add a space after the word
        if !self.characters.is_empty() {
            self.characters.push(' ');

            self.move_to_current_word();
            self.redraw = true;
        }
    }

    pub fn backspace(&mut self) {
        if self.characters.is_empty() {
            return;
        }

        // Remove the last character
        while let Some(removed) = self.characters.pop() {
            let pos = Self::position_for_character_in_framebuf(self.characters.len());
            if removed == ' ' {
                self.move_to_current_word();
            } else {
                // Clear the character from framebuffer
                let mut char_frame = self.framebuffer.cropped(&Rectangle::new(
                    Point::new(pos as i32, 0),
                    Size::new(FONT_SIZE.width, FONT_SIZE.height),
                ));
                let _ = char_frame.clear(Gray2::BLACK);
                break;
            }
        }

        self.autocomplete
            .update(self.characters.split_whitespace().last().unwrap_or(""));
        self.redraw = true;
    }

    pub fn draw(
        &mut self,
        target: &mut impl DrawTarget<Color = Rgb565>,
        current_time: crate::Instant,
    ) {
        let bb = target.bounding_box();
        let last_draw_time = self.current_time.get_or_insert(current_time);

        if self.current_position == self.target_position && !self.redraw {
            // only draw autocomplete when the text is stationairy
            let autocomplete_space = bb.resized_width(WORD_START, AnchorX::Right);
            let _ = self.autocomplete.draw(
                &mut target
                    .clipped(&autocomplete_space)
                    .cropped(&autocomplete_space),
            );

            *last_draw_time = current_time;
        } else {
            let duration_millis = current_time
                .checked_duration_since(*last_draw_time)
                .unwrap()
                .to_millis();
            const VELOCITY: f32 = 0.08; // pixels per ms

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

            // Draw the framebuffer window
            let width = bb.size.width;
            let window_start = self.current_position.saturating_sub(width) as usize;
            let window_width = width.min(self.current_position);
            let left_padding = core::iter::repeat_n(
                COLORS.background,
                width.saturating_sub(self.current_position) as usize,
            );

            // Draw framebuffer content
            let fb = &self.framebuffer;
            let color = self.color;
            let iterator = (0..bb.size.height).flat_map(|y| {
                let start = window_start;
                let end = window_start + window_width as usize;

                left_padding.clone().chain((start..end).map(move |x| {
                    // Check bounds before accessing pixel
                    if let Some(pixel) = fb.pixel(Point::new(x as i32, y as i32)) {
                        match pixel.luma() {
                            0x00 => COLORS.background,
                            0x01 => Rgb565::new(20, 41, 22),
                            0x02 => color,
                            0x03 => color,
                            _ => COLORS.background,
                        }
                    } else {
                        COLORS.background
                    }
                }))
            });

            target
                .fill_contiguous(&bb, iterator)
                .map_err(|_| ())
                .unwrap();

            self.redraw = false;
        }
    }

    fn position_for_character_in_framebuf(index: usize) -> u32 {
        index as u32 * FONT_SIZE.width
    }

    pub fn current_word(&self) -> &str {
        self.characters.split_whitespace().last().unwrap_or("")
    }

    pub fn move_to_current_word(&mut self) {
        let char_index = self.characters.rfind(' ').map(|x| x + 1).unwrap_or(0);
        self.target_position =
            Self::position_for_character_in_framebuf(char_index).saturating_add(WORD_START);
    }

    pub fn word_count(&self) -> usize {
        if self.characters.is_empty() {
            0
        } else {
            self.characters.split_whitespace().count()
        }
    }
}
