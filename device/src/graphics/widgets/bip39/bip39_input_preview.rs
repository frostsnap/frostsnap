use super::progress_bars::ProgressBars;
use crate::graphics::palette::COLORS;
use crate::graphics::widgets::{icons, KeyTouch, FONT_LARGE};
use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    geometry::AnchorX,
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words;
use micromath::F32Ext;
use u8g2_fonts::U8g2TextStyle;

// Constants for vertical BIP39 word display
const TOTAL_WORDS: usize = 25;
const FONT_SIZE: Size = Size::new(16, 24);
// 180 pixels width / 16 pixels per char = 11.25 chars total
// So we can fit 11 chars total
const INDEX_CHARS: usize = 3; // "25."
const SPACE_BETWEEN: usize = 1;
const FB_WIDTH: u32 = 180; // Target width is 180
const FB_HEIGHT: u32 = (TOTAL_WORDS * FONT_SIZE.height as usize) as u32;

type Fb = Framebuffer<
    Gray2,
    RawU2,
    LittleEndian,
    { FB_WIDTH as usize },
    { FB_HEIGHT as usize },
    { buffer_size::<Gray2>(FB_WIDTH as usize, FB_HEIGHT as usize) },
>;

#[derive(Debug)]
pub struct Bip39InputPreview {
    pub(super) area: Rectangle,
    preview_rect: Rectangle,
    backspace_rect: Rectangle,
    progress_rect: Rectangle,
    progress: ProgressBars,
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
        let progress = ProgressBars::new(24);
        let framebuf = Bip39Framebuf::new();

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

        // Always draw progress bars (they have their own redraw logic)
        let _ = self.progress.draw(&mut target.cropped(&self.progress_rect));
    }

    pub fn push_letter(&mut self, letter: char) {
        // Add uppercase letter to framebuffer
        let upper_letter = letter.to_uppercase().next().unwrap_or(letter);
        self.framebuf.add_character(upper_letter);
    }

    pub fn backspace(&mut self) {
        // Delete characters until we reach a state with multiple possibilities
        loop {
            self.framebuf.backspace();
            let current_prefix = self.framebuf.current_input();
            if current_prefix.is_empty() {
                break;
            }

            // Stop when we have multiple possibilities (more than 1 word)
            if bip39_words::words_with_prefix(current_prefix).len() > 1 {
                break;
            }
        }

        self.update_progress();
    }

    pub fn accept_word(&mut self) {
        let current_word = self.framebuf.current_input();
        if !current_word.is_empty() {
            self.framebuf.mark_word_boundary();
            self.update_progress();
        }
    }

    /// Unified autocomplete method
    pub fn autocomplete_word(&mut self, target_word: &str) -> bool {
        let current_prefix = self.framebuf.current_input();

        // Validate that target_word starts with current_prefix
        if !target_word.starts_with(current_prefix) {
            return false;
        }

        // Add the remaining characters
        let remaining = &target_word[current_prefix.len()..];
        for c in remaining.chars() {
            let upper_c = c.to_uppercase().next().unwrap_or(c);
            self.framebuf.add_character(upper_c);
        }

        // Accept the completed word
        self.framebuf.mark_word_boundary();
        self.update_progress();

        true
    }

    fn update_progress(&mut self) {
        // Update progress based on number of words entered (1 bar per word)
        self.progress.progress(self.framebuf.word_count());
    }

    pub fn contains(&self, point: Point) -> bool {
        self.preview_rect.contains(point)
    }

    pub fn has_current_word(&self) -> bool {
        !self.framebuf.current_input().is_empty()
    }

    pub fn current_word(&self) -> &str {
        self.framebuf.current_input()
    }

    pub fn is_finished(&self) -> bool {
        self.framebuf.word_count() == 24
    }

    pub fn get_mnemonic(&self) -> String {
        self.framebuf.words.join(" ")
    }

    /// Get the number of BIP39 words that match the current input prefix
    pub fn get_matching_word_count(&self) -> usize {
        let current_word = self.current_word();
        if current_word.is_empty() {
            0
        } else {
            bip39_words::words_with_prefix(current_word).len()
        }
    }

    /// Force redraw of the input preview (including progress bar)
    pub fn force_redraw(&mut self) {
        self.init_draw = false;
        self.framebuf.redraw = true;
        self.progress.redraw = true;
    }
}

#[derive(Debug)]
pub struct Bip39Framebuf {
    framebuffer: Box<Fb>,
    words: Vec<String>,
    current_input: String,
    current_position: u32, // Current vertical scroll position
    current_time: Option<crate::Instant>,
    target_position: u32, // Target vertical scroll position
    color: Rgb565,
    pub(super) redraw: bool,
}

impl Bip39Framebuf {
    pub fn new() -> Self {
        let mut fb = Box::new(Fb::new());
        // Clear the framebuffer
        let _ = fb.clear(Gray2::BLACK);

        // Pre-render word indices
        for i in 0..TOTAL_WORDS {
            let y = (i * FONT_SIZE.height as usize) as i32;
            let index = format!("{}.", i + 1);
            // Move the number to the right by starting at x=8 (half a character width)
            let _ = Text::with_text_style(
                &index,
                Point::new(8, y),
                U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Top)
                    .build(),
            )
            .draw(&mut *fb);
        }

        Self {
            framebuffer: fb,
            words: Vec::new(),
            current_input: String::new(),
            current_position: 0,
            current_time: None,
            target_position: 0,
            color: COLORS.primary,
            redraw: true,
        }
    }

    pub fn add_character(&mut self, c: char) {
        let upper = c.to_uppercase().next().unwrap_or(c);
        self.current_input.push(upper);

        // Draw the character directly to the framebuffer
        let word_idx = self.words.len();
        let char_idx = self.current_input.len() - 1;
        let x = ((INDEX_CHARS + SPACE_BETWEEN) as usize + char_idx) * FONT_SIZE.width as usize;
        let y = word_idx * FONT_SIZE.height as usize;

        let mut char_frame = self.framebuffer.cropped(&Rectangle::new(
            Point::new(x as i32, y as i32),
            Size::new(FONT_SIZE.width, FONT_SIZE.height),
        ));

        let _ = char_frame.clear(Gray2::BLACK);
        let _ = Text::with_text_style(
            &upper.to_string(),
            Point::zero(),
            U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
            TextStyleBuilder::new()
                .alignment(Alignment::Left)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(&mut char_frame);

        self.redraw = true;
    }

    pub fn mark_word_boundary(&mut self) {
        if !self.current_input.is_empty() && self.words.len() < TOTAL_WORDS {
            self.words.push(self.current_input.clone());
            self.current_input.clear();

            // Set target position to scroll up one line after entering a word
            // But only if we need to (when we have more words than visible lines)
            // We'll calculate this in draw() based on visible_height
            self.redraw = true;
        }
    }

    pub fn backspace(&mut self) {
        if let Some(_) = self.current_input.pop() {
            // Clear the character from framebuffer
            let word_idx = self.words.len();
            let char_idx = self.current_input.len();
            let x = ((INDEX_CHARS + SPACE_BETWEEN) as usize + char_idx) * FONT_SIZE.width as usize;
            let y = word_idx * FONT_SIZE.height as usize;

            let mut char_frame = self.framebuffer.cropped(&Rectangle::new(
                Point::new(x as i32, y as i32),
                Size::new(FONT_SIZE.width, FONT_SIZE.height),
            ));
            let _ = char_frame.clear(Gray2::BLACK);

            self.redraw = true;
        } else if let Some(prev_word) = self.words.pop() {
            // Current input is empty, go back to previous word
            self.current_input = prev_word;

            // Clear the entire previous word line from framebuffer
            let word_idx = self.words.len();
            let start_x = (INDEX_CHARS + SPACE_BETWEEN) as usize * FONT_SIZE.width as usize;
            let y = word_idx * FONT_SIZE.height as usize;
            let width = self.current_input.len() * FONT_SIZE.width as usize;

            let mut word_frame = self.framebuffer.cropped(&Rectangle::new(
                Point::new(start_x as i32, y as i32),
                Size::new(width as u32, FONT_SIZE.height),
            ));
            let _ = word_frame.clear(Gray2::BLACK);

            // Now remove the last character
            if let Some(_) = self.current_input.pop() {
                // Re-draw the word without the last character
                for (char_idx, ch) in self.current_input.chars().enumerate() {
                    let x = ((INDEX_CHARS + SPACE_BETWEEN) as usize + char_idx)
                        * FONT_SIZE.width as usize;

                    let mut char_frame = self.framebuffer.cropped(&Rectangle::new(
                        Point::new(x as i32, y as i32),
                        Size::new(FONT_SIZE.width, FONT_SIZE.height),
                    ));

                    let _ = Text::with_text_style(
                        &ch.to_string(),
                        Point::zero(),
                        U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
                        TextStyleBuilder::new()
                            .alignment(Alignment::Left)
                            .baseline(Baseline::Top)
                            .build(),
                    )
                    .draw(&mut char_frame);
                }
            }

            // Force recalculation of scroll position
            self.target_position = 0; // This will be recalculated in draw()
            self.redraw = true;
        }
    }

    pub fn draw(
        &mut self,
        target: &mut impl DrawTarget<Color = Rgb565>,
        current_time: crate::Instant,
    ) {
        let bb = target.bounding_box();

        // Assert that framebuffer width matches target width
        assert_eq!(
            FB_WIDTH, bb.size.width,
            "Framebuffer width ({}) must match target width ({})",
            FB_WIDTH, bb.size.width
        );

        // Calculate where we should be scrolled to based on current word
        let current_word_line = self.words.len();
        let visible_lines = bb.size.height as usize / FONT_SIZE.height as usize;

        let new_target = if current_word_line >= visible_lines {
            ((current_word_line - visible_lines + 1) * FONT_SIZE.height as usize) as u32
        } else {
            0
        };

        // Update target if it changed
        if new_target != self.target_position {
            self.target_position = new_target;
            self.redraw = true;
        }

        // Animate scrolling using time-based velocity
        let last_draw_time = self.current_time.get_or_insert(current_time);

        if self.current_position != self.target_position {
            let duration_millis = current_time
                .checked_duration_since(*last_draw_time)
                .unwrap()
                .to_millis();

            const VELOCITY: f32 = 0.01; // pixels per millisecond for vertical scrolling

            let distance = (duration_millis as f32 * VELOCITY).round() as i32;
            if distance > 0 {
                *last_draw_time = current_time;

                let direction = self.target_position as i32 - self.current_position as i32;
                let traveled = direction.clamp(-distance, distance);
                self.current_position = ((self.current_position as i32) + traveled) as u32;
                self.redraw = true; // Keep redrawing until animation completes
            }
        } else {
            *last_draw_time = current_time;
        }

        // Only redraw if needed
        if !self.redraw {
            return;
        }

        // Skip to the correct starting position in the framebuffer
        let skip_pixels = self.current_position as usize * FB_WIDTH as usize;
        let take_pixels = bb.size.height as usize * bb.size.width as usize;

        let framebuffer_pixels = RawDataSlice::<RawU2, LittleEndian>::new(self.framebuffer.data())
            .into_iter()
            .skip(skip_pixels)
            .take(take_pixels)
            .map(|pixel| match Gray2::from(pixel).luma() {
                0x00 => COLORS.background,
                0x01 => Rgb565::new(20, 41, 22),
                0x02 => self.color,
                0x03 => self.color,
                _ => COLORS.background,
            });

        let _ = target.fill_contiguous(&bb, framebuffer_pixels);

        // Only clear redraw flag if animation is complete
        if self.current_position == self.target_position {
            self.redraw = false;
        }
    }

    pub fn current_input(&self) -> &str {
        &self.current_input
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }
}
