use super::progress_bars::ProgressBars;
use crate::graphics::palette::COLORS;
use crate::graphics::widgets::{icons, Key, KeyTouch, FONT_LARGE};
use alloc::{
    boxed::Box,
    rc::Rc,
    string::{String, ToString},
    vec,
};
use core::cell::RefCell;
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
pub(super) const TOTAL_WORDS: usize = 25;
pub(super) const FONT_SIZE: Size = Size::new(16, 24);
pub(super) const VERTICAL_PAD: u32 = 10; // 5px top + 5px bottom padding per word
                                         // 180 pixels width / 16 pixels per char = 11.25 chars total
                                         // So we can fit 11 chars total
const INDEX_CHARS: usize = 3; // "25."
const SPACE_BETWEEN: usize = 1;
pub(super) const FB_WIDTH: u32 = 180; // Target width is 180
pub(super) const FB_HEIGHT: u32 = TOTAL_WORDS as u32 * (FONT_SIZE.height + VERTICAL_PAD);

pub(super) type Fb = Framebuffer<
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

        // Preview rect should show at least one full row with padding
        let row_height = FONT_SIZE.height + VERTICAL_PAD;
        let preview_rect = Rectangle::new(
            Point::new(
                0,
                ((area.size.height - progress_height) as i32 - row_height as i32) / 2,
            ),
            Size {
                width: area.size.width - backspace_width,
                height: row_height,
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
            Some(KeyTouch::new(Key::Keyboard('⌫'), self.backspace_rect))
        } else if self.area.contains(point) {
            // Tap on the input preview area triggers the entered words view
            Some(KeyTouch::new(Key::EditWord(0), self.area))
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
            let went_back_to_prev_word = self.framebuf.backspace();
            let current_prefix = self.framebuf.current_input();
            // Stop when we have multiple possibilities (more than 1 word)
            if bip39_words::words_with_prefix(current_prefix).len() > 1 || went_back_to_prev_word {
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

    pub fn get_framebuffer(&self) -> Rc<RefCell<Fb>> {
        self.framebuf.framebuffer.clone()
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

    /// Set the current word being edited
    pub fn set_editing_word(&mut self, word_index: usize) {
        self.framebuf.set_current_input(word_index);
    }

    /// Get the current word index being edited
    pub fn get_current_word_index(&self) -> usize {
        self.framebuf.current_input
    }
}

#[derive(Debug)]
pub struct Bip39Framebuf {
    framebuffer: Rc<RefCell<Fb>>,
    words: [String; TOTAL_WORDS],
    current_input: usize,  // Index of current word being edited
    current_position: u32, // Current vertical scroll position
    current_time: Option<crate::Instant>,
    target_position: u32, // Target vertical scroll position
    animation_start_time: Option<crate::Instant>, // When current animation started
    color: Rgb565,
    pub(super) redraw: bool,
}

impl Bip39Framebuf {
    pub fn new() -> Self {
        let mut fb = Box::new(Fb::new());
        // Clear the framebuffer
        let _ = fb.clear(Gray2::BLACK);

        // Pre-render word indices with aligned dots
        for i in 0..TOTAL_WORDS {
            let y =
                (i as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as i32 + (VERTICAL_PAD / 2) as i32;
            let number = (i + 1).to_string();

            // Position dot at a fixed location (2.5 chars from left)
            let dot_x = 40; // 2.5 * 16 pixels

            // Calculate number position to right-align before the dot
            let number_x = if i < 9 {
                // Single digit: one char before dot
                dot_x - FONT_SIZE.width as i32 // 40 - 16 = 24
            } else {
                // Double digit: two chars before dot
                dot_x - (2 * FONT_SIZE.width as i32) // 40 - 32 = 8
            };

            // Draw the number
            let _ = Text::with_text_style(
                &number,
                Point::new(number_x, y),
                U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Top)
                    .build(),
            )
            .draw(&mut *fb);

            // Draw the dot at the fixed position
            let _ = Text::with_text_style(
                ".",
                Point::new(dot_x, y),
                U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Top)
                    .build(),
            )
            .draw(&mut *fb);
        }

        // Create empty strings array
        let words: [String; TOTAL_WORDS] = vec![String::new(); TOTAL_WORDS]
            .try_into()
            .expect("vec of correct size");

        Self {
            framebuffer: Rc::new(RefCell::new(*fb)),
            words,
            current_input: 0,
            current_position: 0,
            current_time: None,
            target_position: 0,
            animation_start_time: None,
            color: COLORS.primary,
            redraw: true,
        }
    }

    pub fn add_character(&mut self, c: char) {
        let upper = c.to_uppercase().next().unwrap_or(c);
        self.words[self.current_input].push(upper);

        // Draw the character directly to the framebuffer
        let word_idx = self.current_input;
        let char_idx = self.words[word_idx].len() - 1;
        let x = ((INDEX_CHARS + SPACE_BETWEEN) as usize + char_idx) * FONT_SIZE.width as usize;
        let y = (word_idx as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
            + (VERTICAL_PAD / 2) as usize;

        let mut fb = self.framebuffer.borrow_mut();
        let mut char_frame = fb.cropped(&Rectangle::new(
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
        if !self.words[self.current_input].is_empty() && self.current_input < TOTAL_WORDS - 1 {
            // Move to next word
            self.current_input += 1;

            // Set target position to scroll up one line after entering a word
            // But only if we need to (when we have more words than visible lines)
            // We'll calculate this in draw() based on visible_height
            self.redraw = true;
        }
    }

    pub fn backspace(&mut self) -> bool {
        if let Some(_) = self.words[self.current_input].pop() {
            // Clear the character from framebuffer
            let word_idx = self.current_input;
            let char_idx = self.words[word_idx].len();
            let x = ((INDEX_CHARS + SPACE_BETWEEN) as usize + char_idx) * FONT_SIZE.width as usize;
            let y = (word_idx as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
                + (VERTICAL_PAD / 2) as usize;

            let mut fb = self.framebuffer.borrow_mut();
            let mut char_frame = fb.cropped(&Rectangle::new(
                Point::new(x as i32, y as i32),
                Size::new(FONT_SIZE.width, FONT_SIZE.height),
            ));
            let _ = char_frame.clear(Gray2::BLACK);

            self.redraw = true;
            false
        } else if self.current_input > 0 {
            // Current word is empty, go back to previous word
            self.current_input -= 1;
            // Force recalculation of scroll position
            self.target_position = 0; // This will be recalculated in draw()
            self.redraw = true;
            true
        } else {
            false
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

        // Check if this is the first draw
        let is_first_draw = self.current_time.is_none();

        // Calculate where we should be scrolled to based on current word
        let current_word_line = self.current_input;
        let row_height = FONT_SIZE.height + VERTICAL_PAD;
        let visible_lines = bb.size.height as usize / row_height as usize;

        // Keep the current word visible within the viewport
        let new_target = if current_word_line >= visible_lines {
            // Scroll so that the current word is at the bottom of the visible area
            ((current_word_line + 1).saturating_sub(visible_lines) * row_height as usize) as u32
        } else {
            0
        };

        // Update target if it changed
        if new_target != self.target_position {
            self.target_position = new_target;
            self.animation_start_time = Some(current_time); // Reset animation start
            self.redraw = true;
        }

        // On first draw, jump directly to target position without animation
        if is_first_draw {
            self.current_position = self.target_position;
        }

        // Animate scrolling using acceleration
        let last_draw_time = self.current_time.get_or_insert(current_time);

        if self.current_position != self.target_position {
            // Calculate time since animation started
            let animation_elapsed = if let Some(start_time) = self.animation_start_time {
                current_time
                    .checked_duration_since(start_time)
                    .unwrap()
                    .to_millis() as f32
            } else {
                self.animation_start_time = Some(current_time);
                0.0
            };

            // Accelerating curve: starts slow, speeds up
            // Using a quadratic function for smooth acceleration
            const ACCELERATION: f32 = 0.00000005; // Acceleration factor (5x faster)
            const MIN_VELOCITY: f32 = 0.0005; // Minimum velocity to ensure it starts moving

            // Calculate current velocity based on time elapsed
            let velocity = MIN_VELOCITY + (ACCELERATION * animation_elapsed * animation_elapsed);

            // Calculate distance to move this frame
            let frame_duration = current_time
                .checked_duration_since(*last_draw_time)
                .unwrap()
                .to_millis() as f32;

            // For upward scrolling, we want positive distance to move up (decrease position)
            // When velocity is negative, we actually want to move down briefly
            let distance = (frame_duration * velocity).round() as i32;

            // Only proceed if we're actually going to move
            if distance != 0 {
                *last_draw_time = current_time;

                // Direction: negative means scrolling up (decreasing position)
                let direction =
                    (self.target_position as i32 - self.current_position as i32).signum();

                // Apply the velocity in the correct direction
                // For upward scroll (direction < 0), positive velocity should decrease position
                let position_change = if direction < 0 {
                    -distance // Upward scroll
                } else {
                    distance // Downward scroll
                };

                let new_position = (self.current_position as i32 + position_change).max(0);

                // Check if we've reached or passed the target
                if (direction < 0 && new_position <= self.target_position as i32)
                    || (direction > 0 && new_position >= self.target_position as i32)
                    || direction == 0
                {
                    self.current_position = self.target_position;
                    self.animation_start_time = None; // Animation complete
                } else {
                    self.current_position = new_position as u32;
                }

                self.redraw = true; // Keep redrawing until animation completes
            }
            // If distance is 0, we don't update last_draw_time, allowing frame_duration to accumulate
        } else {
            *last_draw_time = current_time;
            self.animation_start_time = None;
        }

        // Only redraw if needed
        if !self.redraw {
            return;
        }

        // Skip to the correct starting position in the framebuffer
        // current_position is already in pixels (Y coordinate), so we need to skip
        // that many rows worth of pixels in the framebuffer
        let skip_rows = self.current_position as usize;
        let skip_pixels = skip_rows * FB_WIDTH as usize;
        let take_pixels = bb.size.height as usize * bb.size.width as usize;

        {
            let fb = self.framebuffer.borrow();
            let framebuffer_pixels = RawDataSlice::<RawU2, LittleEndian>::new(fb.data())
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
        }

        // Only clear redraw flag if animation is complete
        if self.current_position == self.target_position {
            self.redraw = false;
        }
    }

    pub fn current_input(&self) -> &str {
        &self.words[self.current_input]
    }

    pub fn word_count(&self) -> usize {
        // Count non-empty words
        self.words.iter().filter(|w| !w.is_empty()).count()
    }

    pub fn set_current_input(&mut self, word_index: usize) {
        if word_index < TOTAL_WORDS {
            self.current_input = word_index;

            // Calculate the correct scroll position for this word
            // We want the word to be visible, so we scroll to show it
            let row_height = FONT_SIZE.height + VERTICAL_PAD;
            let word_position = word_index as u32 * row_height;

            // Just set both positions to the word's position
            // The draw method will adjust if needed based on visible height
            self.current_position = word_position;
            self.target_position = word_position;
            self.animation_start_time = None; // Clear any ongoing animation
            self.redraw = true;
        }
    }
}
