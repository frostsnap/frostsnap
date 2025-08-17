use super::{AlphabeticKeyboard, Bip39InputPreview, EnteredWords, WordSelector};
use crate::super_draw_target::SuperDrawTarget;
use crate::{DynWidget, Key, KeyTouch, Widget};
use alloc::{string::String, vec, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use frostsnap_backup::bip39_words::{self, FROSTSNAP_BACKUP_WORDS};

pub const MAX_WORD_SELECTOR_WORDS: usize = 6;

#[derive(Debug)]
pub struct EnterBip39ShareScreen {
    alphabetic_keyboard: AlphabeticKeyboard,
    word_selector: Option<WordSelector>,
    entered_words: Option<EnteredWords>,
    bip39_input: Bip39InputPreview,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    mnemonic_complete: bool,
    needs_redraw: bool,
    size: Size,
}

impl EnterBip39ShareScreen {
    pub fn new(area: Size) -> Self {
        let preview_height = 60;
        let keyboard_rect = Rectangle::new(
            Point::new(0, preview_height),
            Size::new(area.width, area.height - preview_height as u32),
        );
        let input_display_rect =
            Rectangle::new(Point::zero(), Size::new(area.width, preview_height as u32));

        let alphabetic_keyboard = AlphabeticKeyboard::new(keyboard_rect.size.height);
        let bip39_input = Bip39InputPreview::new(input_display_rect);

        let mut screen = Self {
            alphabetic_keyboard,
            word_selector: None,
            entered_words: None,
            bip39_input,
            touches: vec![],
            keyboard_rect,
            mnemonic_complete: false,
            needs_redraw: true,
            size: area,
        };

        // Initialize valid keys for empty input
        screen.update_valid_keys();
        screen
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) {
        if self.mnemonic_complete {
            // Only draw if we just transitioned to complete state
            if self.needs_redraw {
                // Draw green checkmark in the center
                use crate::icons::Icon;
                use crate::palette::PALETTE;
                use embedded_graphics::primitives::PrimitiveStyleBuilder;
                use embedded_iconoir::size48px::actions::Check;

                // Clear background
                let bounds = target.bounding_box();
                let _ = bounds
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.background)
                            .build(),
                    )
                    .draw(target);

                // Draw large green checkmark in center
                Icon::<Check>::default()
                    .with_color(PALETTE.tertiary)
                    .with_center(bounds.center())
                    .draw(target);

                self.needs_redraw = false;
            }
        } else if let Some(ref mut entered_words) = self.entered_words {
            // Full-screen entered words view
            // Update button state before drawing
            entered_words.update_button_state();
            entered_words.draw(target);
        } else if let Some(ref mut word_selector) = self.word_selector {
            // Full-screen word selector
            word_selector.draw(target);
        } else {
            // Normal keyboard and input preview
            let _ = self
                .alphabetic_keyboard
                .draw(&mut target.clone().crop(self.keyboard_rect), current_time);

            // Draw BIP39 input preview
            let input_display_rect = Rectangle::new(
                Point::zero(),
                Size::new(target.bounding_box().size.width, 60),
            );
            let _ = self
                .bip39_input
                .draw(&mut target.clone().crop(input_display_rect), current_time);
        }

        // Draw touches and clean up
        // First draw all touches
        for touch in &mut self.touches {
            touch.draw(target, current_time);
        }

        // Then remove finished ones
        self.touches.retain(|touch| !touch.is_finished());
    }

    pub fn handle_touch(&mut self, point: Point, current_time: crate::Instant, lift_up: bool) {
        if lift_up {
            // Otherwise process normal key release
            // Find the last non-cancelled touch
            if let Some(active_touch) = self.touches.iter_mut().rev().find(|t| !t.has_been_let_go())
            {
                if let Some(key) = active_touch.let_go(current_time) {
                    match key {
                        Key::Keyboard('⌫') => {
                            // Backspace
                            self.bip39_input.backspace();
                            self.update_valid_keys();
                        }
                        Key::Keyboard(c) if c.is_alphabetic() => {
                            self.push_letter_and_autocomplete(c);
                        }
                        Key::WordSelector(index) => {
                            // Handle word selector index
                            if let Some(ref word_selector) = self.word_selector {
                                if let Some(word) = word_selector.get_word_by_index(index) {
                                    // Use unified autocomplete method
                                    self.bip39_input.autocomplete_word(word);
                                    self.update_valid_keys();

                                    // If we now have all 25 words entered, show EnteredWords view
                                    if self.bip39_input.word_count() == FROSTSNAP_BACKUP_WORDS {
                                        self.clear_touches();
                                        let framebuffer = self.bip39_input.get_framebuffer();
                                        let words_ref = self.bip39_input.get_words_ref();
                                        let mut entered_words =
                                            EnteredWords::new(framebuffer, self.size, words_ref);
                                        entered_words
                                            .scroll_to_word_at_top(FROSTSNAP_BACKUP_WORDS - 1);
                                        self.entered_words = Some(entered_words);
                                    }
                                }
                            }
                        }
                        Key::EditWord(word_index) => {
                            // If EditWord(0) is from input preview tap, show entered words view
                            // Otherwise, it's from EnteredWords view, so edit specific word
                            if word_index == 0 && self.entered_words.is_none() {
                                self.clear_touches();

                                // Show EnteredWords view, scrolled to show current word at bottom
                                let framebuffer = self.bip39_input.get_framebuffer();
                                let current_word_index = self.bip39_input.get_current_word_index();
                                let words_ref = self.bip39_input.get_words_ref();
                                let mut entered_words =
                                    EnteredWords::new(framebuffer, self.size, words_ref);
                                entered_words.scroll_to_word_at_top(current_word_index);
                                self.entered_words = Some(entered_words);
                            } else if self.entered_words.is_some() {
                                // Exit EnteredWords view and start editing the selected word
                                // Only allow if the word can be edited
                                if self.bip39_input.can_edit_word_at_index(word_index) {
                                    self.entered_words = None;
                                    self.clear_touches();
                                    self.bip39_input.set_editing_word(word_index);
                                    self.update_valid_keys();
                                    self.bip39_input.force_redraw();
                                }
                            }
                        }
                        Key::NavBack => {
                            // Go back to previous word
                            let current_index = self.bip39_input.get_current_word_index();
                            if current_index > 0 {
                                self.bip39_input.set_editing_word(current_index - 1);
                                self.update_valid_keys();
                                self.bip39_input.force_redraw();
                            }
                        }
                        Key::NavForward => {
                            // Go forward to next word
                            let current_index = self.bip39_input.get_current_word_index();
                            let next_index = current_index + 1;
                            if next_index < FROSTSNAP_BACKUP_WORDS
                                && self.bip39_input.can_edit_word_at_index(next_index)
                            {
                                self.bip39_input.set_editing_word(next_index);
                                self.update_valid_keys();
                                self.bip39_input.force_redraw();
                            }
                        }
                        Key::Submit => {
                            // User pressed submit button - close entered words view and mark as complete
                            self.entered_words = None;
                            self.clear_touches();
                            self.mnemonic_complete = true;
                            self.needs_redraw = true;
                        }
                        _ => {} // Ignore other keyboard characters
                    }
                }
            }
        } else {
            // Handle touch for different modes
            let key_touch = if let Some(ref entered_words) = self.entered_words {
                // EnteredWords is full-screen, handle its touches directly
                entered_words.handle_touch(point)
            } else if let Some(ref word_selector) = self.word_selector {
                // Word selector is full-screen, handle its touches directly
                word_selector.handle_touch(point)
            } else {
                // Normal mode: check input preview first, then keyboard
                if let Some(key_touch) = self.bip39_input.handle_touch(point, current_time, lift_up)
                {
                    Some(key_touch)
                } else if self.keyboard_rect.contains(point) {
                    let translated_point = point - self.keyboard_rect.top_left;
                    self.alphabetic_keyboard
                        .handle_touch(translated_point, current_time, lift_up)
                        .map(|mut key_touch| {
                            key_touch.translate(self.keyboard_rect.top_left);
                            key_touch
                        })
                } else {
                    None
                }
            };

            if let Some(key_touch) = key_touch {
                // Fast forward any ongoing scrolling animation immediately
                // This ensures the UI is responsive to new input
                if matches!(key_touch.key, Key::Keyboard(_)) {
                    self.bip39_input.fast_forward_scrolling();
                }

                if let Some(last) = self.touches.last_mut() {
                    if last.key == key_touch.key {
                        self.touches.pop();
                    } else {
                        last.cancel();
                    }
                }
                self.touches.push(key_touch);
            } else {
                // No valid key was touched - cancel all active touches
                // This can happen when keyboard state changes and a key becomes invalid
                for touch in &mut self.touches {
                    if !touch.is_finished() {
                        touch.cancel();
                    }
                }
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.mnemonic_complete
    }

    pub fn get_mnemonic(&self) -> String {
        self.bip39_input.get_mnemonic()
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        // scrolling cancels the touch
        if let Some(active_touch) = self.touches.last_mut() {
            active_touch.cancel()
        }

        if let Some(ref mut entered_words) = self.entered_words {
            // Handle drag for entered words view
            entered_words.handle_vertical_drag(prev_y, new_y, _is_release);
        } else if self.word_selector.is_none() {
            // Only handle drag for keyboard, not word selector
            self.alphabetic_keyboard
                .handle_vertical_drag(prev_y, new_y, _is_release);
        }
    }

    fn clear_touches(&mut self) {
        self.touches.clear();
    }

    fn update_valid_keys(&mut self) {
        let current_word = self.bip39_input.current_word();

        // Check if we should show the word selector when we have a partial word
        if !current_word.is_empty() {
            let matching_words = bip39_words::words_with_prefix(&current_word);

            // Only show word selector if there are 2-6 matching words (not 1)
            if matching_words.len() > 1 && matching_words.len() <= MAX_WORD_SELECTOR_WORDS {
                // Create word selector with the matching words
                let full_screen_size = Size::new(
                    self.keyboard_rect.size.width,
                    self.keyboard_rect.size.height + self.bip39_input.area.size.height, // Add input preview height
                );

                self.word_selector = Some(WordSelector::new(
                    full_screen_size,
                    matching_words,
                    current_word.clone(),
                ));
                // Cancel all touches before switching to word selector
                self.clear_touches();
            } else {
                // Clear word selector if we shouldn't show it
                if self.word_selector.is_some() {
                    // Cancel all touches before switching back to keyboard
                    self.clear_touches();
                    // Force redraw of input preview (including progress bar)
                    self.bip39_input.force_redraw();
                }
                self.word_selector = None;
            }
        } else {
            // Clear word selector
            if self.word_selector.is_some() {
                // Cancel all touches before switching back to keyboard
                for touch in &mut self.touches {
                    touch.cancel();
                }
                self.touches.clear();
                // Force redraw of input preview (including progress bar)
                self.bip39_input.force_redraw();
            }
            self.word_selector = None;
        }

        // Always update keyboard valid keys (even if word selector is showing)
        let current = self.bip39_input.current_word();
        let valid_letters = bip39_words::get_valid_next_letters(&current);
        self.alphabetic_keyboard.set_valid_keys(valid_letters);

        // Update the current word index on the keyboard
        let current_index = self.bip39_input.get_current_word_index();
        self.alphabetic_keyboard
            .set_current_word_index(current_index);
    }

    fn push_letter_and_autocomplete(&mut self, letter: char) {
        self.bip39_input.push_letter(letter);

        // Special case: if we just typed Q, automatically add U
        if letter.to_uppercase().next().unwrap_or(letter) == 'Q' {
            self.bip39_input.push_letter('U');
        }

        let current = self.bip39_input.current_word();
        let words_with_prefix = bip39_words::words_with_prefix(&current);

        if words_with_prefix.len() == 1 {
            self.bip39_input.autocomplete_word(words_with_prefix[0]);

            // If we now have all 25 words entered, show EnteredWords view
            if self.bip39_input.word_count() == FROSTSNAP_BACKUP_WORDS {
                self.clear_touches();
                // Calculate full screen size
                let screen_size = Size::new(
                    self.keyboard_rect.size.width,
                    self.keyboard_rect.size.height + self.bip39_input.area.size.height,
                );
                let framebuffer = self.bip39_input.get_framebuffer();
                let words_ref = self.bip39_input.get_words_ref();
                let mut entered_words = EnteredWords::new(framebuffer, screen_size, words_ref);
                entered_words.scroll_to_word_at_top(FROSTSNAP_BACKUP_WORDS - 1);
                self.entered_words = Some(entered_words);
            }
        }

        self.update_valid_keys();
    }
}

impl Widget for EnterBip39ShareScreen {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if self.mnemonic_complete {
            // Only draw if we just transitioned to complete state
            if self.needs_redraw {
                // Draw green checkmark in the center
                use crate::icons::Icon;
                use crate::palette::PALETTE;
                use embedded_graphics::primitives::PrimitiveStyleBuilder;
                use embedded_iconoir::size48px::actions::Check;

                // Clear background
                let bounds = target.bounding_box();
                bounds
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.background)
                            .build(),
                    )
                    .draw(target)?;

                // Draw large green checkmark in center
                Icon::<Check>::default()
                    .with_color(PALETTE.tertiary)
                    .with_center(bounds.center())
                    .draw(target);

                self.needs_redraw = false;
            }
        } else if let Some(ref mut entered_words) = self.entered_words {
            // Full-screen entered words view
            // Update button state before drawing
            entered_words.update_button_state();
            entered_words.draw(target);
        } else if let Some(ref mut word_selector) = self.word_selector {
            // Full-screen word selector
            word_selector.draw(target);
        } else {
            // Normal keyboard and input preview
            self.alphabetic_keyboard
                .draw(&mut target.clone().crop(self.keyboard_rect), current_time)?;

            // Draw BIP39 input preview
            let input_display_rect = Rectangle::new(
                Point::zero(),
                Size::new(target.bounding_box().size.width, 60),
            );
            self.bip39_input
                .draw(&mut target.clone().crop(input_display_rect), current_time)?;
        }

        // Draw touches and clean up
        // First draw all touches
        for touch in &mut self.touches {
            touch.draw(target, current_time);
        }

        // Then remove finished ones
        self.touches.retain(|touch| !touch.is_finished());

        Ok(())
    }
}

impl crate::DynWidget for EnterBip39ShareScreen {
    fn set_constraints(&mut self, _max_size: Size) {
        // EnterBip39ShareScreen has fixed size based on its area
    }

    fn sizing(&self) -> crate::Sizing {
        self.size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<KeyTouch> {
        if lift_up {
            // Otherwise process normal key release
            // Find the last non-cancelled touch
            if let Some(active_touch) = self.touches.iter_mut().rev().find(|t| !t.has_been_let_go())
            {
                if let Some(key) = active_touch.let_go(current_time) {
                    match key {
                        Key::Keyboard('⌫') => {
                            // Backspace
                            self.bip39_input.backspace();
                            self.update_valid_keys();
                        }
                        Key::Keyboard(c) if c.is_alphabetic() => {
                            self.push_letter_and_autocomplete(c);
                        }
                        Key::WordSelector(index) => {
                            // Handle word selector index
                            if let Some(ref word_selector) = self.word_selector {
                                if let Some(word) = word_selector.get_word_by_index(index) {
                                    // Use unified autocomplete method
                                    self.bip39_input.autocomplete_word(word);
                                    self.update_valid_keys();

                                    // If we now have all 25 words entered, show EnteredWords view
                                    if self.bip39_input.word_count() == FROSTSNAP_BACKUP_WORDS {
                                        self.clear_touches();
                                        let framebuffer = self.bip39_input.get_framebuffer();
                                        let words_ref = self.bip39_input.get_words_ref();
                                        let mut entered_words =
                                            EnteredWords::new(framebuffer, self.size, words_ref);
                                        entered_words
                                            .scroll_to_word_at_top(FROSTSNAP_BACKUP_WORDS - 1);
                                        self.entered_words = Some(entered_words);
                                    }
                                }
                            }
                        }
                        Key::EditWord(word_index) => {
                            // If EditWord(0) is from input preview tap, show entered words view
                            // Otherwise, it's from EnteredWords view, so edit specific word
                            if word_index == 0 && self.entered_words.is_none() {
                                self.clear_touches();

                                // Show EnteredWords view, scrolled to show current word at bottom
                                let framebuffer = self.bip39_input.get_framebuffer();
                                let current_word_index = self.bip39_input.get_current_word_index();
                                let words_ref = self.bip39_input.get_words_ref();
                                let mut entered_words =
                                    EnteredWords::new(framebuffer, self.size, words_ref);
                                entered_words.scroll_to_word_at_top(current_word_index);
                                self.entered_words = Some(entered_words);
                            } else if self.entered_words.is_some() {
                                // Exit EnteredWords view and start editing the selected word
                                // Only allow if the word can be edited
                                if self.bip39_input.can_edit_word_at_index(word_index) {
                                    self.entered_words = None;
                                    self.clear_touches();
                                    self.bip39_input.set_editing_word(word_index);
                                    self.update_valid_keys();
                                    self.bip39_input.force_redraw();
                                }
                            }
                        }
                        Key::NavBack => {
                            // Go back to previous word
                            let current_index = self.bip39_input.get_current_word_index();
                            if current_index > 0 {
                                self.bip39_input.set_editing_word(current_index - 1);
                                self.update_valid_keys();
                                self.bip39_input.force_redraw();
                            }
                        }
                        Key::NavForward => {
                            // Go forward to next word
                            let current_index = self.bip39_input.get_current_word_index();
                            let next_index = current_index + 1;
                            if next_index < FROSTSNAP_BACKUP_WORDS
                                && self.bip39_input.can_edit_word_at_index(next_index)
                            {
                                self.bip39_input.set_editing_word(next_index);
                                self.update_valid_keys();
                                self.bip39_input.force_redraw();
                            }
                        }
                        Key::Submit => {
                            // User pressed submit button - close entered words view and mark as complete
                            self.entered_words = None;
                            self.clear_touches();
                            self.mnemonic_complete = true;
                            self.needs_redraw = true;
                        }
                        _ => {} // Ignore other keyboard characters
                    }
                }
            }
            None
        } else {
            // Handle touch for different modes
            let key_touch = if let Some(ref entered_words) = self.entered_words {
                // EnteredWords is full-screen, handle its touches directly
                entered_words.handle_touch(point)
            } else if let Some(ref word_selector) = self.word_selector {
                // Word selector is full-screen, handle its touches directly
                word_selector.handle_touch(point)
            } else {
                // Normal mode: check input preview first, then keyboard
                if let Some(key_touch) = self.bip39_input.handle_touch(point, current_time, lift_up)
                {
                    Some(key_touch)
                } else if self.keyboard_rect.contains(point) {
                    let translated_point = point - self.keyboard_rect.top_left;
                    self.alphabetic_keyboard
                        .handle_touch(translated_point, current_time, lift_up)
                        .map(|mut key_touch| {
                            key_touch.translate(self.keyboard_rect.top_left);
                            key_touch
                        })
                } else {
                    None
                }
            };

            if let Some(key_touch) = key_touch {
                // Fast forward any ongoing scrolling animation immediately
                // This ensures the UI is responsive to new input
                if matches!(key_touch.key, Key::Keyboard(_)) {
                    self.bip39_input.fast_forward_scrolling();
                }

                if let Some(last) = self.touches.last_mut() {
                    if last.key == key_touch.key {
                        self.touches.pop();
                    } else {
                        last.cancel();
                    }
                }
                self.touches.push(key_touch);
                None
            } else {
                // No valid key was touched - cancel all active touches
                // This can happen when keyboard state changes and a key becomes invalid
                for touch in &mut self.touches {
                    if !touch.is_finished() {
                        touch.cancel();
                    }
                }
                None
            }
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        // scrolling cancels the touch
        if let Some(active_touch) = self.touches.last_mut() {
            active_touch.cancel()
        }

        if let Some(ref mut entered_words) = self.entered_words {
            // Handle drag for entered words view
            entered_words.handle_vertical_drag(prev_y, new_y, _is_release);
        } else if self.word_selector.is_none() {
            // Only handle drag for keyboard, not word selector
            self.alphabetic_keyboard
                .handle_vertical_drag(prev_y, new_y, _is_release);
        }
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
        self.bip39_input.force_redraw();
        self.alphabetic_keyboard.force_full_redraw();
        // TODO: Implement DynWidget for WordSelector and EnteredWords
        // if let Some(ref mut word_selector) = self.word_selector {
        //     word_selector.force_full_redraw();
        // }
        // if let Some(ref mut entered_words) = self.entered_words {
        //     entered_words.force_full_redraw();
        // }
    }
}
