use super::submit_backup_button::SubmitBackupState;
use crate::cursor::Cursor;
use crate::palette::PALETTE;
use crate::progress_bars::ProgressBars;
use crate::super_draw_target::SuperDrawTarget;
use crate::{icons, DynWidget, Key, KeyTouch, Widget, FONT_LARGE};
use alloc::{
    borrow::Cow,
    boxed::Box,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;
use core::slice::Iter;
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
use frostsnap_backup::bip39_words::{self, FROSTSNAP_BACKUP_WORDS};
use u8g2_fonts::U8g2TextStyle;

// Constants for vertical BIP39 word display
pub(super) const TOTAL_WORDS: usize = FROSTSNAP_BACKUP_WORDS;
pub(super) const FONT_SIZE: Size = Size::new(16, 24);
pub(super) const VERTICAL_PAD: u32 = 12; // 6px top + 6px bottom padding per word
                                         // 180 pixels width / 16 pixels per char = 11.25 chars total
                                         // So we can fit 11 chars total
const INDEX_CHARS: usize = 2; // "25" (no dot)
const SPACE_BETWEEN: usize = 1;
const PREVIEW_LEFT_PAD: i32 = 4; // Left padding for preview rect
pub(super) const TOP_PADDING: u32 = 10; // Top padding before first word
pub(super) const FB_WIDTH: u32 = 176; // Divisible by 4 for Gray2 alignment
pub(super) const FB_HEIGHT: u32 =
    TOP_PADDING + (TOTAL_WORDS as u32 * (FONT_SIZE.height + VERTICAL_PAD));

pub(super) type Fb = Framebuffer<
    Gray2,
    RawU2,
    LittleEndian,
    { FB_WIDTH as usize },
    { FB_HEIGHT as usize },
    { buffer_size::<Gray2>(FB_WIDTH as usize, FB_HEIGHT as usize) },
>;

#[derive(Debug)]
pub struct Bip39Words {
    words: [Cow<'static, str>; TOTAL_WORDS],
    /// Number of completed words (Borrowed) from the beginning
    /// Words at index > n_completed cannot be edited
    n_completed: usize,
}

impl Default for Bip39Words {
    fn default() -> Self {
        Self::new()
    }
}

impl Bip39Words {
    pub fn new() -> Self {
        Self {
            words: [const { Cow::Borrowed("") }; TOTAL_WORDS],
            n_completed: 0,
        }
    }

    pub fn completed_words(&self) -> impl Iterator<Item = (usize, &'static str)> + '_ {
        self.words
            .iter()
            .enumerate()
            .filter_map(|(idx, cow)| match cow {
                Cow::Borrowed(s) if !s.is_empty() => Some((idx, *s)),
                _ => None,
            })
    }

    pub fn get(&self, index: usize) -> &Cow<'static, str> {
        &self.words[index]
    }

    pub fn get_mut(&mut self, index: usize) -> &mut Cow<'static, str> {
        &mut self.words[index]
    }

    /// Check if a word at the given index can be edited
    pub fn can_edit_at(&self, index: usize) -> bool {
        // Can always edit at or before n_completed
        index <= self.n_completed
    }

    /// Get the number of completed words
    pub fn n_completed(&self) -> usize {
        self.n_completed
    }

    pub fn iter(&self) -> Iter<'_, Cow<'static, str>> {
        self.words.iter()
    }

    pub fn get_submit_button_state(&self) -> SubmitBackupState {
        // Count completed words
        let completed_count = self.completed_words().count();

        if completed_count < TOTAL_WORDS {
            SubmitBackupState::Incomplete {
                words_entered: completed_count,
            }
        } else {
            // All words entered, collect them into array
            let mut words_array: [&'static str; FROSTSNAP_BACKUP_WORDS] =
                [""; FROSTSNAP_BACKUP_WORDS];
            for (idx, word) in self.completed_words() {
                words_array[idx] = word;
            }

            // TODO: Implement proper BIP39 checksum validation
            // For now, just return Complete if all words are filled
            SubmitBackupState::Complete {
                words: Box::new(words_array),
            }
        }
    }
}

#[derive(Debug)]
pub struct Bip39InputPreview {
    pub(super) area: Rectangle,
    preview_rect: Rectangle,
    backspace_rect: Rectangle,
    progress_rect: Rectangle,
    progress: ProgressBars,
    framebuf: Bip39Framebuf,
    init_draw: bool,
    cursor: Cursor,
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

        // Preview rect should use full available height
        let preview_rect = Rectangle::new(
            Point::new(PREVIEW_LEFT_PAD, 0),
            Size {
                width: FB_WIDTH, // Must match framebuffer width exactly
                height: area.size.height - progress_height,
            },
        );

        let progress_rect = Rectangle::new(
            Point::new(0, area.size.height as i32 - progress_height as i32),
            Size::new(area.size.width, progress_height),
        );

        // 25 words for Frostsnap backup
        let mut progress = ProgressBars::new(FROSTSNAP_BACKUP_WORDS);
        progress.set_constraints(progress_rect.size);
        let framebuf = Bip39Framebuf::new();

        Self {
            area,
            preview_rect,
            backspace_rect,
            progress_rect,
            progress,
            framebuf,
            init_draw: false,
            cursor: Cursor::new(Point::zero()), // Will update position in draw
        }
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
            if bip39_words::words_with_prefix(&current_prefix).len() > 1 || went_back_to_prev_word {
                break;
            }
        }

        self.update_progress();
    }

    pub fn accept_word(&mut self) {
        if self.framebuf.current_input().is_empty() {
            return;
        }
        self.framebuf.mark_word_boundary();
        self.update_progress();
    }

    /// Unified autocomplete method
    pub fn autocomplete_word(&mut self, target_word: &str) -> bool {
        // Find the matching BIP39 word and store as static reference
        if let Ok(idx) = bip39_words::BIP39_WORDS.binary_search(&target_word) {
            let mut words = self.framebuf.words.borrow_mut();
            *words.get_mut(self.framebuf.current_input) =
                Cow::Borrowed(bip39_words::BIP39_WORDS[idx]);

            // Update n_completed to be at least current_input + 1
            words.n_completed = words.n_completed.max(self.framebuf.current_input + 1);
            drop(words);

            // Redraw the word in the framebuffer
            self.framebuf.redraw_current_word();

            // Accept the completed word
            self.framebuf.mark_word_boundary();
            self.update_progress();

            true
        } else {
            false
        }
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

    pub fn current_word(&self) -> String {
        self.framebuf.current_input()
    }

    pub fn is_finished(&self) -> bool {
        self.framebuf.word_count() == FROSTSNAP_BACKUP_WORDS - 1
    }

    pub fn get_mnemonic(&self) -> String {
        self.framebuf
            .words
            .borrow()
            .iter()
            .take(FROSTSNAP_BACKUP_WORDS - 1)
            .map(|w| w.as_ref())
            .collect::<Vec<_>>()
            .join(" ")
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
            bip39_words::words_with_prefix(&current_word).len()
        }
    }

    /// Force redraw of the input preview (including progress bar)
    pub fn force_redraw(&mut self) {
        self.init_draw = false;
        self.framebuf.redraw = true;
        self.progress.force_full_redraw();
    }

    /// Set the current word being edited
    pub fn set_editing_word(&mut self, word_index: usize) {
        self.framebuf.set_current_input(word_index);
    }

    /// Get the current word index being edited
    pub fn get_current_word_index(&self) -> usize {
        self.framebuf.current_input
    }

    /// Get all the words entered so far
    pub fn get_words(&self) -> Vec<String> {
        self.framebuf.get_words()
    }

    /// Get a shared reference to the words array
    pub fn get_words_ref(&self) -> Rc<RefCell<Bip39Words>> {
        self.framebuf.words.clone()
    }

    /// Get the number of words that have been entered
    pub fn word_count(&self) -> usize {
        self.framebuf.word_count()
    }

    /// Fast forward any ongoing scrolling animation
    pub fn fast_forward_scrolling(&mut self) {
        self.framebuf.fast_forward_scrolling();
    }

    /// Check if a word at the given index can be edited
    pub fn can_edit_word_at_index(&self, word_index: usize) -> bool {
        let words = self.framebuf.words.borrow();
        words.can_edit_at(word_index)
    }

    fn draw_cursor<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Calculate cursor position based on current word and character count
        let current_word = self.framebuf.current_input();
        let char_count = current_word.len();

        // Calculate x position for cursor
        let x = ((INDEX_CHARS + SPACE_BETWEEN) + char_count) * FONT_SIZE.width as usize;

        // Fixed Y position - cursor always appears at the same vertical position
        // This should be in the middle of the preview area
        let y = target.bounding_box().size.height as i32 / 2 - FONT_SIZE.height as i32 / 2;

        // Update cursor position
        self.cursor.set_position(Point::new(x as i32, y));

        // Let the cursor handle its own drawing and blinking
        self.cursor.draw(target, current_time)?;
        Ok(())
    }
}

impl crate::DynWidget for Bip39InputPreview {
    fn set_constraints(&mut self, _max_size: Size) {
        // Bip39InputPreview has fixed size based on its area
    }

    fn sizing(&self) -> crate::Sizing {
        self.area.size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<KeyTouch> {
        if self.backspace_rect.contains(point) {
            Some(KeyTouch::new(Key::Keyboard('⌫'), self.backspace_rect))
        } else if self.area.contains(point) {
            // Tap on the input preview area triggers the entered words view
            Some(KeyTouch::new(Key::EditWord(0), self.area))
        } else {
            None
        }
    }
}

impl Widget for Bip39InputPreview {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Draw backspace icon on first draw
        if !self.init_draw {
            // Clear the entire area first
            let clear_rect = Rectangle::new(Point::zero(), self.area.size);
            let _ = clear_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.background)
                        .build(),
                )
                .draw(target);

            icons::backspace()
                .with_color(PALETTE.error)
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
            .draw(&mut target.clone().crop(self.preview_rect), current_time)?;

        // Draw cursor if on current word
        if self.framebuf.current_input < FROSTSNAP_BACKUP_WORDS {
            let _ = self.draw_cursor(&mut target.clone().crop(self.preview_rect), current_time);
        }

        // Always draw progress bars (they have their own redraw logic)
        self.progress
            .draw(&mut target.clone().crop(self.progress_rect), current_time)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct Bip39Framebuf {
    framebuffer: Rc<RefCell<Fb>>,
    words: Rc<RefCell<Bip39Words>>,
    current_input: usize,  // Index of current word being edited
    current_position: u32, // Current vertical scroll position
    current_time: Option<crate::Instant>,
    target_position: u32, // Target vertical scroll position
    animation_start_time: Option<crate::Instant>, // When current animation started
    viewport_height: u32, // Height of the visible area
    pub(super) redraw: bool,
}

impl Bip39Framebuf {
    pub fn new() -> Self {
        let mut fb = Box::new(Fb::new());
        // Clear the framebuffer
        let _ = fb.clear(Gray2::BLACK);

        // Pre-render word indices with aligned dots
        for i in 0..TOTAL_WORDS {
            let y = TOP_PADDING as i32
                + (i as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as i32
                + (VERTICAL_PAD / 2) as i32;
            let number = (i + 1).to_string();

            // Right-align numbers at 2 characters from left (no dots)
            let number_right_edge = 32; // 2 * 16 pixels

            // Calculate number position to right-align
            let number_x = if i < 9 {
                // Single digit: right-aligned at position
                number_right_edge - FONT_SIZE.width as i32
            } else {
                // Double digit: starts at position 0
                0
            };

            // Draw the number with a different gray level
            let _ = Text::with_text_style(
                &number,
                Point::new(number_x, y),
                U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x01)), // Use Gray level 1 for numbers
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Top)
                    .build(),
            )
            .draw(&mut *fb);
        }

        Self {
            framebuffer: Rc::new(RefCell::new(*fb)),
            words: Rc::new(RefCell::new(Bip39Words::new())),
            current_input: 0,
            current_position: 0,
            current_time: None,
            target_position: 0,
            animation_start_time: None,
            viewport_height: 34, // Default viewport height
            redraw: true,
        }
    }

    pub fn add_character(&mut self, c: char) {
        let upper = c.to_uppercase().next().unwrap_or(c);

        // Get mutable access to the current word
        let mut words = self.words.borrow_mut();
        let word = words.get_mut(self.current_input);
        word.to_mut().push(upper);

        // Draw the character directly to the framebuffer
        let word_idx = self.current_input;
        let char_idx = word.len() - 1;
        let x = ((INDEX_CHARS + SPACE_BETWEEN) + char_idx) * FONT_SIZE.width as usize;
        let y = TOP_PADDING as usize
            + (word_idx as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
            + (VERTICAL_PAD / 2) as usize;

        drop(words); // Release the borrow before borrowing framebuffer

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
        let should_advance = {
            let words = self.words.borrow();
            let current_word = words.get(self.current_input);
            !current_word.is_empty() && self.current_input < TOTAL_WORDS - 1
        };

        if should_advance {
            self.current_input += 1;
            // Update scroll position with animation
            self.update_scroll_position(false);
            self.redraw = true;
        }
        // Note: The word is already validated and stored as a static BIP39 word
        // via the autocomplete process, so we don't need to do it again here
    }

    pub fn backspace(&mut self) -> bool {
        let (is_empty, char_to_clear) = {
            let mut words = self.words.borrow_mut();
            let word = words.get_mut(self.current_input);

            if !word.is_empty() {
                // Remove last character
                word.to_mut().pop();
                let char_idx = word.len();
                (false, Some(char_idx))
            } else {
                (true, None)
            }
        };

        if let Some(char_idx) = char_to_clear {
            // Clear the character from framebuffer
            let word_idx = self.current_input;
            let x = ((INDEX_CHARS + SPACE_BETWEEN) + char_idx) * FONT_SIZE.width as usize;
            let y = TOP_PADDING as usize
                + (word_idx as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
                + (VERTICAL_PAD / 2) as usize;

            let mut fb = self.framebuffer.borrow_mut();
            let mut char_frame = fb.cropped(&Rectangle::new(
                Point::new(x as i32, y as i32),
                Size::new(FONT_SIZE.width, FONT_SIZE.height),
            ));
            let _ = char_frame.clear(Gray2::BLACK);

            self.redraw = true;
            false
        } else if is_empty && self.current_input > 0 {
            // Current word is empty, go back to previous word
            self.current_input -= 1;

            // Update scroll position without animation
            self.update_scroll_position(true);
            self.redraw = true;
            true
        } else {
            false
        }
    }

    pub fn current_input(&self) -> String {
        self.words.borrow().get(self.current_input).to_string()
    }

    pub fn word_count(&self) -> usize {
        // Count non-empty words - a word is complete if it's not an empty borrow
        self.words.borrow().iter().filter(|w| !w.is_empty()).count()
    }

    fn redraw_current_word(&mut self) {
        // Clear and redraw the current word in the framebuffer
        let word_idx = self.current_input;
        let y = TOP_PADDING as usize
            + (word_idx as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
            + (VERTICAL_PAD / 2) as usize;

        let words = self.words.borrow();
        let current_word = words.get(word_idx);

        let mut fb = self.framebuffer.borrow_mut();

        // Clear the entire word area
        let word_rect = Rectangle::new(
            Point::new(
                ((INDEX_CHARS + SPACE_BETWEEN) * FONT_SIZE.width as usize) as i32,
                y as i32,
            ),
            Size::new(FONT_SIZE.width * 8, FONT_SIZE.height), // Max 8 chars
        );
        let mut word_frame = fb.cropped(&word_rect);
        let _ = word_frame.clear(Gray2::BLACK);

        // Redraw each character
        for (i, ch) in current_word.chars().enumerate() {
            let x = ((INDEX_CHARS + SPACE_BETWEEN) + i) * FONT_SIZE.width as usize;
            let mut char_frame = fb.cropped(&Rectangle::new(
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

    pub fn set_current_input(&mut self, word_index: usize) {
        if word_index < TOTAL_WORDS {
            self.current_input = word_index;
            self.update_scroll_position(true); // true = skip animation
            self.redraw_current_word();
            self.redraw = true;
        }
    }

    // Calculate the target scroll position based on current word and viewport
    fn calculate_target_position(&self, viewport_height: u32) -> u32 {
        let row_height = FONT_SIZE.height + VERTICAL_PAD;
        let visible_lines = viewport_height as usize / row_height as usize;

        // Keep the current word visible within the viewport
        if self.current_input >= visible_lines {
            // Scroll so that the current word is at the bottom of the visible area
            ((self.current_input + 1).saturating_sub(visible_lines) * row_height as usize) as u32
        } else {
            0
        }
    }

    // Update scroll position (called when word changes)
    pub fn update_scroll_position(&mut self, skip_animation: bool) {
        let new_target = self.calculate_target_position(self.viewport_height);

        if new_target != self.target_position {
            self.target_position = new_target;
            if skip_animation {
                self.current_position = new_target;
                self.animation_start_time = None;
            } else {
                self.animation_start_time = self.current_time;
            }
            self.redraw = true;
        }
    }

    pub fn get_words(&self) -> Vec<String> {
        // Convert Cow array to Vec<String> for compatibility
        self.words
            .borrow()
            .iter()
            .map(|word| word.to_string())
            .collect()
    }

    /// Fast forward scrolling by jumping to target position
    pub fn fast_forward_scrolling(&mut self) {
        self.redraw = self.current_position != self.target_position;
        self.current_position = self.target_position;
        self.animation_start_time = None;
    }
}

impl crate::DynWidget for Bip39Framebuf {
    fn set_constraints(&mut self, max_size: Size) {
        // Update viewport height based on constraints
        self.viewport_height = max_size.height;
    }

    fn sizing(&self) -> crate::Sizing {
        // Return the actual framebuffer dimensions
        crate::Sizing {
            width: FB_WIDTH,
            height: self.viewport_height,
        }
    }
}

impl Widget for Bip39Framebuf {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let bb = target.bounding_box();

        // Assert that framebuffer width matches target width
        assert_eq!(
            FB_WIDTH, bb.size.width,
            "Framebuffer width ({}) must match target width ({})",
            FB_WIDTH, bb.size.width
        );

        // Check if this is the first draw
        let is_first_draw = self.current_time.is_none();

        // Update viewport height if it changed
        if self.viewport_height != bb.size.height {
            self.viewport_height = bb.size.height;
            // Recalculate target position with new viewport
            self.update_scroll_position(is_first_draw);
        }

        // On first draw, jump to target position
        if is_first_draw {
            self.current_position = self.target_position;
        }

        // Animate scrolling using acceleration
        let last_draw_time = self.current_time.get_or_insert(current_time);

        if self.current_position != self.target_position {
            // Calculate time since animation started
            let animation_elapsed = if let Some(start_time) = self.animation_start_time {
                current_time.duration_since(start_time).unwrap_or(0) as f32
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
            let frame_duration = current_time.duration_since(*last_draw_time).unwrap_or(0) as f32;

            // For upward scrolling, we want positive distance to move up (decrease position)
            // When velocity is negative, we actually want to move down briefly
            // Manual rounding: add 0.5 and truncate for positive values
            let raw_distance = frame_duration * velocity;
            let distance = if raw_distance >= 0.0 {
                (raw_distance + 0.5) as i32
            } else {
                (raw_distance - 0.5) as i32
            };

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
            return Ok(());
        }

        // Skip to the correct starting position in the framebuffer
        // current_position is already in pixels (Y coordinate), so we need to skip
        // that many rows worth of pixels in the framebuffer
        let skip_rows = self.current_position as usize;
        let skip_pixels = skip_rows * FB_WIDTH as usize;
        let take_pixels = bb.size.height as usize * bb.size.width as usize;

        {
            let fb = self.framebuffer.try_borrow().unwrap();
            let framebuffer_pixels = RawDataSlice::<RawU2, LittleEndian>::new(fb.data())
                .into_iter()
                .skip(skip_pixels)
                .take(take_pixels)
                .map(|pixel| match Gray2::from(pixel).luma() {
                    0x00 => PALETTE.background,
                    0x01 => PALETTE.outline, // Numbers in subtle outline color
                    0x02 => PALETTE.on_background, // Words in normal text color
                    0x03 => PALETTE.on_background, // Also words
                    _ => PALETTE.background,
                });

            target.fill_contiguous(&bb, framebuffer_pixels)?;
        }

        // Only clear redraw flag if animation is complete
        if self.current_position == self.target_position {
            self.redraw = false;
        }

        Ok(())
    }
}
