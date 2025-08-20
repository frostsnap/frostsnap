use crate::palette::PALETTE;
use crate::{Key, KeyTouch};
use alloc::rc::Rc;
use core::cell::RefCell;
use embedded_graphics::{
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
};

use super::{
    bip39_input_preview::{
        Bip39Words, Fb, FB_WIDTH, FONT_SIZE, TOP_PADDING, TOTAL_WORDS, VERTICAL_PAD,
    },
    submit_backup_button::{SubmitBackupButton, SUBMIT_BUTTON_HEIGHT, SUBMIT_BUTTON_WIDTH},
};
use crate::scroll_bar::{ScrollBar, SCROLLBAR_WIDTH};

const WORD_LIST_LEFT_PAD: i32 = 4; // Left padding for word list

#[derive(Debug)]
pub struct EnteredWords {
    framebuffer: Rc<RefCell<Fb>>,
    words: Rc<RefCell<Bip39Words>>,
    scroll_position: i32,
    visible_size: Size,
    needs_redraw: bool,
    submit_button: SubmitBackupButton,
    button_needs_redraw: bool,
    scroll_bar: ScrollBar,
    first_draw: bool,
}

impl EnteredWords {
    /// Calculate the actual content height based on n_completed
    fn calculate_content_height(&self) -> u32 {
        let n_completed = self.words.borrow().n_completed();
        // Show up to n_completed + 1 words (the one being edited)
        let visible_words = (n_completed + 1).min(TOTAL_WORDS);
        TOP_PADDING + (visible_words as u32 * (FONT_SIZE.height + VERTICAL_PAD))
    }

    pub fn new(
        framebuffer: Rc<RefCell<Fb>>,
        visible_size: Size,
        words: Rc<RefCell<Bip39Words>>,
    ) -> Self {
        // Create submit button (full screen width)
        let button_rect = Rectangle::new(
            Point::zero(),
            Size::new(visible_size.width, SUBMIT_BUTTON_HEIGHT),
        );

        // Get the submit button state from words
        let button_state = words.borrow().get_submit_button_state();
        let submit_button = SubmitBackupButton::new(button_rect, button_state);

        // Use a fixed thumb size for now
        let thumb_size = crate::Frac::from_ratio(1, 4); // 25% of scrollbar height
        let scroll_bar = ScrollBar::new(thumb_size);

        Self {
            framebuffer: framebuffer.clone(),
            words: words.clone(),
            scroll_position: 0,
            visible_size,
            needs_redraw: true,
            submit_button,
            button_needs_redraw: true, // Draw button on first frame
            scroll_bar,
            first_draw: true,
        }
    }

    pub fn scroll_to_word_at_top(&mut self, word_index: usize) {
        // Get the actual number of visible words
        let n_completed = self.words.borrow().n_completed();
        let visible_words = (n_completed + 1).min(TOTAL_WORDS);

        // Clamp the word index to what's actually visible
        let clamped_word_index = word_index.min(visible_words.saturating_sub(1));

        let row_height = FONT_SIZE.height + VERTICAL_PAD;

        // Calculate scroll to show word at top of scrollable area
        let desired_scroll = clamped_word_index as i32 * row_height as i32;

        // Get dynamic content height
        let content_height = self.calculate_content_height();

        // Calculate max scroll based on dynamic content
        let scrollable_height = self.visible_size.height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        let max_scroll = (content_height as i32)
            .saturating_sub(scrollable_height)
            .max(0);
        self.scroll_position = desired_scroll.clamp(0, max_scroll);
        let fraction = if max_scroll > 0 {
            crate::Rat::from_ratio(self.scroll_position as u32, max_scroll as u32)
        } else {
            crate::Rat::ZERO
        };
        self.scroll_bar.set_scroll_position(fraction);
        self.needs_redraw = true;
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut crate::SuperDrawTarget<D, Rgb565>,
    ) {
        if !self.needs_redraw {
            return;
        }

        let bounds = target.bounding_box();

        // Clear entire screen on first draw
        if self.first_draw {
            let _ = bounds
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.background)
                        .build(),
                )
                .draw(target);
        }

        // Create a cropped target that matches the framebuffer width, centered
        let cropped_rect = Rectangle::new(
            Point::new(WORD_LIST_LEFT_PAD, 0),
            Size::new(FB_WIDTH, bounds.size.height),
        );

        let mut cropped_target = target.clone().crop(cropped_rect);
        let cropped_bounds = cropped_target.bounding_box();

        // Get dynamic content height
        let content_height = self.calculate_content_height();

        // Draw words framebuffer in scrollable area only
        if self.scroll_position < content_height as i32 {
            let scrollable_height = (self.visible_size.height - SUBMIT_BUTTON_HEIGHT) as i32;
            let skip_pixels = (self.scroll_position.max(0) as usize) * FB_WIDTH as usize;
            let words_visible_height =
                (content_height as i32 - self.scroll_position).min(scrollable_height) as usize;
            let take_pixels = words_visible_height * FB_WIDTH as usize;

            {
                let fb = self.framebuffer.try_borrow().unwrap();

                let framebuffer_pixels = RawDataSlice::<RawU2, LittleEndian>::new(fb.data())
                    .into_iter()
                    .skip(skip_pixels)
                    .take(take_pixels)
                    .map(|pixel| match Gray2::from(pixel).luma() {
                        0x00 => PALETTE.background,
                        0x01 => PALETTE.outline, // Numbers
                        0x02 => PALETTE.on_background,
                        0x03 => PALETTE.on_background,
                        _ => PALETTE.background,
                    });

                let words_rect = Rectangle::new(
                    Point::zero(),
                    Size::new(cropped_bounds.size.width, words_visible_height as u32),
                );
                let _ = cropped_target.fill_contiguous(&words_rect, framebuffer_pixels);
            } // fb borrow is dropped here
        }

        // Calculate button position
        let button_y = bounds.size.height as i32 - SUBMIT_BUTTON_HEIGHT as i32;

        // Draw submit button at fixed position at bottom of screen (full width)
        let button_rect = Rectangle::new(
            Point::new(0, button_y),
            Size::new(SUBMIT_BUTTON_WIDTH, SUBMIT_BUTTON_HEIGHT),
        );

        // Only draw the button if it needs redrawing
        if self.button_needs_redraw {
            let mut button_target = target.clone().crop(button_rect);
            let _ = self.submit_button.draw(
                &mut button_target,
                Rectangle::new(Point::zero(), button_rect.size),
            );
            self.button_needs_redraw = false;
        }

        // Draw scroll bar
        const SCROLLBAR_MARGIN: u32 = 0; // No margin from right edge
        const SCROLLBAR_TOP_MARGIN: u32 = 30; // Increased top margin
        const SCROLLBAR_BOTTOM_MARGIN: u32 = 2; // Bottom margin

        let scrollbar_x = bounds.size.width as i32 - (SCROLLBAR_WIDTH + SCROLLBAR_MARGIN) as i32;
        let scrollbar_y = SCROLLBAR_TOP_MARGIN as i32;
        let scrollbar_height = (bounds.size.height - SUBMIT_BUTTON_HEIGHT)
            - SCROLLBAR_TOP_MARGIN
            - SCROLLBAR_BOTTOM_MARGIN;

        let scrollbar_area = Rectangle::new(
            Point::new(scrollbar_x, scrollbar_y),
            Size::new(SCROLLBAR_WIDTH, scrollbar_height),
        );
        self.scroll_bar
            .draw(&mut target.clone().crop(scrollbar_area));

        self.needs_redraw = false;
        self.first_draw = false;
    }

    pub fn update_button_state(&mut self) {
        // Check if button state needs updating
        let new_state = self.words.borrow().get_submit_button_state();
        if self.submit_button.update_state(new_state) {
            self.button_needs_redraw = true;
        }
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        // Check submit button first (fixed at bottom, full width)
        let button_y = self.visible_size.height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        if point.y >= button_y {
            let button_point = Point::new(point.x, point.y - button_y);
            if self.submit_button.handle_touch(button_point) {
                let rect = Rectangle::new(
                    Point::new(0, button_y),
                    Size::new(self.visible_size.width, SUBMIT_BUTTON_HEIGHT),
                );
                return Some(KeyTouch::new(Key::Submit, rect));
            }
            return None; // Button area but not a valid touch
        }

        // Check if touch is within the content area
        if point.x >= WORD_LIST_LEFT_PAD && point.x < WORD_LIST_LEFT_PAD + FB_WIDTH as i32 {
            // Adjust point for content offset
            let content_point =
                Point::new(point.x - WORD_LIST_LEFT_PAD, point.y + self.scroll_position);

            // Calculate which word was touched using row height with padding
            // Account for TOP_PADDING in the framebuffer
            let row_height = (FONT_SIZE.height + VERTICAL_PAD) as i32;
            let adjusted_y = content_point.y - TOP_PADDING as i32;
            let word_index = if adjusted_y >= 0 {
                (adjusted_y / row_height) as usize
            } else {
                return None; // Touch is in the top padding area
            };

            // Get the number of visible words
            let n_completed = self.words.borrow().n_completed();
            let visible_words = (n_completed + 1).min(TOTAL_WORDS);

            if word_index < visible_words {
                // Check if this word can be edited (should always be true for visible words)
                let can_edit = self.words.borrow().can_edit_at(word_index);

                if can_edit {
                    // Create a rectangle for the touched word (includes padding)
                    // Add TOP_PADDING since words are offset in the framebuffer
                    let y = TOP_PADDING as i32 + (word_index as i32 * row_height)
                        - self.scroll_position;
                    let button_y = self.visible_size.height as i32 - SUBMIT_BUTTON_HEIGHT as i32;

                    // Clip the rectangle height if it would extend into the button area
                    let max_height = (button_y - y).max(0) as u32;
                    let rect_height = (FONT_SIZE.height + VERTICAL_PAD).min(max_height);

                    // Only return a touch if the rectangle has some height
                    if rect_height > 0 {
                        // Calculate width excluding scrollbar area
                        let touch_width = self.visible_size.width - SCROLLBAR_WIDTH;

                        let rect = Rectangle::new(
                            Point::new(0, y), // x=0 as requested
                            Size::new(touch_width, rect_height),
                        );
                        Some(KeyTouch::new(Key::EditWord(word_index), rect))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);

        // Only scroll if there's a meaningful delta
        if delta.abs() > 0 {
            self.scroll(delta);
        }
    }

    fn scroll(&mut self, amount: i32) {
        // Get dynamic content height
        let content_height = self.calculate_content_height();

        // Scrollable area is screen height minus button height
        let scrollable_height = self.visible_size.height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        let max_scroll = (content_height as i32)
            .saturating_sub(scrollable_height)
            .max(0);
        let new_scroll_position = (self.scroll_position - amount).clamp(0, max_scroll);

        // Only redraw if position actually changed
        if new_scroll_position != self.scroll_position {
            self.scroll_position = new_scroll_position;
            let fraction = if max_scroll > 0 {
                crate::Rat::from_ratio(self.scroll_position as u32, max_scroll as u32)
            } else {
                crate::Rat::ZERO
            };
            self.scroll_bar.set_scroll_position(fraction);
            self.needs_redraw = true;
        }
    }
}
