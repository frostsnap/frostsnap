use super::{
    AlphabeticKeyboard, Bip39InputPreview, Bip39Model, EnteredWords, MainViewState,
    NumericKeyboard, WordSelector,
};
use crate::super_draw_target::SuperDrawTarget;
use crate::OneTimeClearHack;
use crate::{DynWidget, Key, KeyTouch, Widget};
use alloc::{vec, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

pub struct EnterBip39ShareScreen {
    model: Bip39Model,
    numeric_keyboard: Option<OneTimeClearHack<NumericKeyboard>>,
    alphabetic_keyboard: AlphabeticKeyboard,
    word_selector: Option<OneTimeClearHack<WordSelector>>,
    entered_words: Option<EnteredWords>,
    bip39_input: Bip39InputPreview,
    touches: Vec<KeyTouch>,
    keyboard_rect: Rectangle,
    needs_redraw: bool,
    size: Size,
}

impl EnterBip39ShareScreen {
    pub fn new() -> Self {
        let model = Bip39Model::new();
        let alphabetic_keyboard = AlphabeticKeyboard::new();
        let bip39_input = Bip39InputPreview::new();

        Self {
            model,
            numeric_keyboard: None,
            alphabetic_keyboard,
            word_selector: None,
            entered_words: None,
            bip39_input,
            touches: vec![],
            keyboard_rect: Rectangle::zero(),
            needs_redraw: true,
            size: Size::zero(),
        }
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self.model.view_state().main_view,
            MainViewState::AllWordsEntered { .. }
        )
    }

    /// Testing method to pre-fill with 24 "EGG" words for quick testing
    pub fn prefill_test_words(&mut self) {
        // First complete the share index
        let mutations = self.model.complete_row("1");
        self.bip39_input.apply_mutations(&mutations);

        // Then complete 24 "EGG" words
        for _ in 0..24 {
            let mutations = self.model.complete_row("EGG");
            self.bip39_input.apply_mutations(&mutations);
        }

        self.update_from_model();
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

    fn cancel_all_touches(&mut self) {
        for touch in &mut self.touches {
            touch.cancel();
        }
    }

    fn update_from_model(&mut self) {
        let view_state = self.model.view_state();

        // Update the input preview based on view state
        self.bip39_input.update_from_view_state(&view_state);

        // Update progress - use total completed rows (share index + words)
        let completed_rows = self.model.num_completed_rows();
        self.bip39_input.update_progress(completed_rows);
        // Update keyboard/UI based on main view state
        match view_state.main_view {
            MainViewState::AllWordsEntered { .. } => {
                // Show the EnteredWords view - same as when user taps ShowEnteredWords
                if self.entered_words.is_none() {
                    let framebuffer = self.bip39_input.get_framebuffer();
                    let entered_words =
                        EnteredWords::new(framebuffer, self.size, view_state.clone());
                    self.entered_words = Some(entered_words);
                }

                // Hide keyboards and word selector
                self.numeric_keyboard = None;
                self.word_selector = None;
                self.cancel_all_touches();
            }
            MainViewState::EnterShareIndex { ref current } => {
                // Show numeric keyboard
                if self.numeric_keyboard.is_none() {
                    let numeric_keyboard = NumericKeyboard::new();
                    let mut numeric_keyboard_with_clear = OneTimeClearHack::new(numeric_keyboard);
                    numeric_keyboard_with_clear.set_constraints(self.keyboard_rect.size);
                    self.numeric_keyboard = Some(numeric_keyboard_with_clear);
                }

                if let Some(numeric_keyboard) = &mut self.numeric_keyboard {
                    numeric_keyboard.set_bottom_buttons_enabled(!current.is_empty());
                }
            }
            MainViewState::EnterWord { valid_letters } => {
                if self.numeric_keyboard.is_some() || self.word_selector.is_some() {
                    self.numeric_keyboard = None;
                    self.word_selector = None;
                    self.cancel_all_touches();
                }

                // Update alphabetic keyboard
                self.alphabetic_keyboard.set_valid_keys(valid_letters);
                let word_index = view_state.row - 1; // -1 because row 0 is share index
                self.alphabetic_keyboard.set_current_word_index(word_index);
            }
            MainViewState::WordSelect {
                ref current,
                possible_words,
            } => {
                // Show word selector if not already showing
                if self.word_selector.is_none() {
                    let word_selector = WordSelector::new(possible_words, current);
                    let mut word_selector_with_clear = OneTimeClearHack::new(word_selector);
                    word_selector_with_clear.set_constraints(self.keyboard_rect.size);
                    self.word_selector = Some(word_selector_with_clear);
                    self.cancel_all_touches();
                }

                // Still update alphabetic keyboard for consistency
                let word_index = view_state.row - 1; // -1 because row 0 is share index
                self.alphabetic_keyboard.set_current_word_index(word_index);
            }
        }
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
        // Draw touches and clean up
        // First draw all touches
        for touch in &mut self.touches {
            touch.draw(target, current_time);
        }

        // Then remove finished ones
        self.touches.retain(|touch| !touch.is_finished());

        let input_display_rect = Rectangle::new(
            Point::zero(),
            Size::new(target.bounding_box().size.width, 60),
        );

        if let Some(ref mut entered_words) = self.entered_words {
            // Full-screen entered words view
            entered_words.draw(target, current_time);
        } else if let Some(ref mut numeric_keyboard) = self.numeric_keyboard {
            self.bip39_input
                .draw(&mut target.clone().crop(input_display_rect), current_time)?;
            // Draw BIP39 input preview
            numeric_keyboard.draw(&mut target.clone().crop(self.keyboard_rect), current_time)?;
        } else if let Some(ref mut word_selector) = self.word_selector {
            // Draw input preview at top
            let _ = self
                .bip39_input
                .draw(&mut target.clone().crop(input_display_rect), current_time);

            // Draw word selector in keyboard area
            word_selector.draw(&mut target.clone().crop(self.keyboard_rect), current_time)?;
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

        Ok(())
    }
}

impl crate::DynWidget for EnterBip39ShareScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.size = max_size;
        
        // Calculate keyboard rect
        let preview_height = 60;
        self.keyboard_rect = Rectangle::new(
            Point::new(0, preview_height),
            Size::new(max_size.width, max_size.height - preview_height as u32),
        );
        
        // Update children constraints
        self.bip39_input.set_constraints(Size::new(max_size.width, preview_height as u32));
        self.alphabetic_keyboard.set_constraints(self.keyboard_rect.size);
        
        // Update numeric keyboard and word selector if they exist
        if let Some(ref mut numeric_keyboard) = self.numeric_keyboard {
            numeric_keyboard.set_constraints(self.keyboard_rect.size);
        }
        if let Some(ref mut word_selector) = self.word_selector {
            word_selector.set_constraints(self.keyboard_rect.size);
        }
        
        // Update from model to ensure proper initial scroll position
        self.update_from_model();
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
                            // Just pass to model, let it handle
                            let mutations = self.model.backspace();
                            self.bip39_input.apply_mutations(&mutations);
                            self.update_from_model();
                        }
                        Key::Keyboard('✓') => {
                            // Get current state to know what to complete
                            let view_state = self.model.view_state();
                            if let MainViewState::EnterShareIndex { current } = view_state.main_view
                            {
                                let mutations = self.model.complete_row(&current);
                                self.bip39_input.apply_mutations(&mutations);
                                self.update_from_model();
                            }
                        }
                        Key::Keyboard(c) if c.is_alphabetic() || c.is_numeric() => {
                            // Just pass character to model
                            let mutations = self.model.add_character(c);
                            self.bip39_input.apply_mutations(&mutations);
                            self.update_from_model();

                            // Check if we're complete
                            if self.model.is_complete() {
                                // TODO: Create EnteredWords view when needed
                                // For now just mark as complete
                                self.needs_redraw = true;
                            }
                        }
                        Key::WordSelector(word) => {
                            // Complete the current row with selected word
                            let mutations = self.model.complete_row(word);
                            self.bip39_input.apply_mutations(&mutations);
                            self.update_from_model();

                            // Check if we're complete
                            if self.model.is_complete() {
                                // TODO: Show EnteredWords view
                                self.needs_redraw = true;
                            }
                        }
                        Key::ShowEnteredWords => {
                            // Only show EnteredWords if we're at the start of a new word
                            let view_state = self.model.view_state();
                            if view_state.can_show_entered_words() {
                                let framebuffer = self.bip39_input.get_framebuffer();
                                let current_row = view_state.row;
                                let mut entered_words =
                                    EnteredWords::new(framebuffer, self.size, view_state);
                                // Scroll to show current word
                                if current_row > 0 {
                                    entered_words.scroll_to_word_at_top(current_row - 1);
                                }
                                self.entered_words = Some(entered_words);
                                // Cancel all touches when switching views
                                self.cancel_all_touches();
                            }
                            // Otherwise ignore the touch
                        }
                        Key::EditWord(word_index) => {
                            // word_index from EnteredWords is actually the row index (0 = share index, 1+ = words)
                            let mutations = self.model.edit_row(word_index);
                            self.bip39_input.apply_mutations(&mutations);
                            self.bip39_input.force_redraw();
                            self.update_from_model();

                            // Exit EnteredWords view if we're in it
                            if self.entered_words.is_some() {
                                self.entered_words = None;
                                // Cancel all touches when switching views
                                self.cancel_all_touches();
                            }
                        }
                        _ => {} // Ignore other keys
                    }
                }
            }
        } else {
            // Handle touch for different modes
            let key_touch = if let Some(ref entered_words) = self.entered_words {
                // EnteredWords is full-screen, handle its touches directly
                entered_words.handle_touch(point)
            } else if let Some(ref mut numeric_keyboard) = self.numeric_keyboard {
                // Numeric keyboard is in keyboard area for share index entry
                if self.keyboard_rect.contains(point) {
                    let translated_point = point - self.keyboard_rect.top_left;
                    numeric_keyboard
                        .handle_touch(translated_point, current_time, lift_up)
                        .map(|mut key_touch| {
                            key_touch.translate(self.keyboard_rect.top_left);
                            key_touch
                        })
                } else {
                    // Check input preview area
                    self.bip39_input.handle_touch(point, current_time, lift_up)
                }
            } else if let Some(ref mut word_selector) = self.word_selector {
                // Word selector is in keyboard area, input preview is visible
                if self.keyboard_rect.contains(point) {
                    let translated_point = point - self.keyboard_rect.top_left;
                    word_selector
                        .handle_touch(translated_point, current_time, lift_up)
                        .map(|mut key_touch| {
                            key_touch.translate(self.keyboard_rect.top_left);
                            key_touch
                        })
                } else {
                    // Check input preview area
                    self.bip39_input.handle_touch(point, current_time, lift_up)
                }
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
            }
        }
        None
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
