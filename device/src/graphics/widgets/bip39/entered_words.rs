use crate::graphics::palette::COLORS;
use crate::graphics::widgets::{Key, KeyTouch};
use alloc::rc::Rc;
use core::cell::RefCell;
use embedded_graphics::{
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
};

use super::{
    bip39_input_preview::{Bip39Words, Fb, FONT_SIZE, FB_WIDTH, TOTAL_WORDS, VERTICAL_PAD},
    submit_backup_button::{SubmitBackupButton, SUBMIT_BUTTON_HEIGHT, SUBMIT_BUTTON_WIDTH},
};

const WORD_LIST_LEFT_PAD: i32 = 5; // Left padding for word list

#[derive(Debug)]
pub struct EnteredWords {
    framebuffer: Rc<RefCell<Fb>>,
    words: Rc<RefCell<Bip39Words>>,
    scroll_position: i32,
    visible_height: u32,
    visible_width: u32,
    needs_redraw: bool,
    submit_button: SubmitBackupButton,
}

impl EnteredWords {
    pub fn new(framebuffer: Rc<RefCell<Fb>>, visible_size: Size, words: Rc<RefCell<Bip39Words>>) -> Self {
        // Create submit button (full screen width)
        let button_rect = Rectangle::new(
            Point::zero(),
            Size::new(visible_size.width, SUBMIT_BUTTON_HEIGHT)
        );
        
        // Get the submit button state from words
        let button_state = words.borrow().get_submit_button_state();
        let submit_button = SubmitBackupButton::new(button_rect, button_state);
        
        Self {
            framebuffer: framebuffer.clone(),
            words: words.clone(),
            scroll_position: 0,
            visible_height: visible_size.height,
            visible_width: visible_size.width,
            needs_redraw: true,
            submit_button,
        }
    }
    
    pub fn scroll_to_word_at_bottom(&mut self, word_index: usize) {
        let row_height = FONT_SIZE.height + VERTICAL_PAD;
        
        // Calculate scroll to show word at bottom of scrollable area (above button)
        let scrollable_height = self.visible_height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        let word_bottom = (word_index + 1) as i32 * row_height as i32;
        let desired_scroll = word_bottom - scrollable_height;
        
        // Calculate max scroll (words only)
        let total_content_height = TOTAL_WORDS as i32 * row_height as i32;
        let max_scroll = total_content_height.saturating_sub(scrollable_height);
        self.scroll_position = desired_scroll.clamp(0, max_scroll);
        self.needs_redraw = true;
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if !self.needs_redraw {
            return;
        }

        let bounds = target.bounding_box();

        // Calculate horizontal offset with minimal left padding
        let x_offset = WORD_LIST_LEFT_PAD;

        // Create a cropped target that matches the framebuffer width, centered
        let cropped_rect = Rectangle::new(
            Point::new(x_offset, 0),
            Size::new(FB_WIDTH, bounds.size.height)
        );
        
        let mut cropped_target = target.cropped(&cropped_rect);
        let cropped_bounds = cropped_target.bounding_box();

        // Calculate total content height (words only, not including button)
        let words_height = TOTAL_WORDS as i32 * (FONT_SIZE.height + VERTICAL_PAD) as i32;
        
        // Draw words framebuffer in scrollable area only
        if self.scroll_position < words_height {
            let scrollable_height = (self.visible_height - SUBMIT_BUTTON_HEIGHT) as i32;
            let skip_pixels = (self.scroll_position.max(0) as usize) * FB_WIDTH as usize;
            let words_visible_height = (words_height - self.scroll_position).min(scrollable_height) as usize;
            let take_pixels = words_visible_height * FB_WIDTH as usize;

            {
                let fb = self.framebuffer.try_borrow().unwrap();
                
                let framebuffer_pixels = RawDataSlice::<RawU2, LittleEndian>::new(fb.data())
                    .into_iter()
                    .skip(skip_pixels)
                    .take(take_pixels)
                    .map(|pixel| match Gray2::from(pixel).luma() {
                        0x00 => COLORS.background,
                        0x01 => Rgb565::new(20, 41, 22),
                        0x02 => COLORS.primary,
                        0x03 => COLORS.primary,
                        _ => COLORS.background,
                    });

                let words_rect = Rectangle::new(
                    Point::zero(),
                    Size::new(cropped_bounds.size.width, words_visible_height as u32)
                );
                let _ = cropped_target.fill_contiguous(&words_rect, framebuffer_pixels);
            } // fb borrow is dropped here
        }
        
        // Calculate button position
        let button_y = bounds.size.height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        
        // Draw submit button at fixed position at bottom of screen (full width)
        let button_rect = Rectangle::new(
            Point::new(0, button_y),
            Size::new(SUBMIT_BUTTON_WIDTH, SUBMIT_BUTTON_HEIGHT)
        );
        
        // Draw the button
        let _ = self.submit_button.draw(target, button_rect);
        
        // Fill the remaining areas on both sides with background color (only up to button area)
        
        // Left side (only up to button)
        if x_offset > 0 {
            let left_rect = Rectangle::new(
                Point::zero(),
                Size::new(x_offset as u32, button_y as u32)
            );
            let _ = left_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.background)
                        .build(),
                )
                .draw(target);
        }
        
        // Right side (only up to button)
        let right_x = x_offset + FB_WIDTH as i32;
        let right_width = (bounds.size.width as i32 - right_x).max(0) as u32;
        if right_width > 0 {
            let right_rect = Rectangle::new(
                Point::new(right_x, 0),
                Size::new(right_width, button_y as u32)
            );
            let _ = right_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.background)
                        .build(),
                )
                .draw(target);
        }
        
        // Draw scroll indicator on the right side
        self.draw_scroll_indicator(target);
        
        self.needs_redraw = false;
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        // Check submit button first (fixed at bottom, full width)
        let button_y = self.visible_height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        if point.y >= button_y {
            let button_point = Point::new(point.x, point.y - button_y);
            if self.submit_button.handle_touch(button_point) {
                let rect = Rectangle::new(
                    Point::new(0, button_y),
                    Size::new(self.visible_width, SUBMIT_BUTTON_HEIGHT)
                );
                return Some(KeyTouch::new(Key::Submit, rect));
            }
            return None; // Button area but not a valid touch
        }
        
        // Use same horizontal offset as in draw
        let x_offset = WORD_LIST_LEFT_PAD;
        
        // Check if touch is within the content area
        if point.x >= x_offset && point.x < x_offset + FB_WIDTH as i32 {
            // Adjust point for content offset
            let content_point = Point::new(point.x - x_offset, point.y + self.scroll_position);
            
            // Calculate which word was touched using row height with padding
            let row_height = (FONT_SIZE.height + VERTICAL_PAD) as i32;
            let word_index = (content_point.y / row_height) as usize;
            
            if word_index < TOTAL_WORDS {
                // Create a rectangle for the touched word (includes padding)
                let y = (word_index as i32 * row_height) - self.scroll_position;
                let button_y = self.visible_height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
                
                // Clip the rectangle height if it would extend into the button area
                let max_height = (button_y - y).max(0) as u32;
                let rect_height = (FONT_SIZE.height + VERTICAL_PAD).min(max_height);
                
                // Only return a touch if the rectangle has some height
                if rect_height > 0 {
                    let rect = Rectangle::new(
                        Point::new(x_offset, y),
                        Size::new(FB_WIDTH, rect_height)
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
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        
        // Only scroll if there's a meaningful delta
        if delta.abs() > 0 {
            self.scroll(delta);
        }
    }

    fn scroll(&mut self, amount: i32) {
        // Scrollable area is screen height minus button height
        let scrollable_height = self.visible_height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        let total_content_height = TOTAL_WORDS as i32 * (FONT_SIZE.height + VERTICAL_PAD) as i32;
        let max_scroll = total_content_height.saturating_sub(scrollable_height);
        let new_scroll_position = (self.scroll_position - amount).clamp(0, max_scroll);
        
        // Only redraw if position actually changed
        if new_scroll_position != self.scroll_position {
            self.scroll_position = new_scroll_position;
            self.needs_redraw = true;
        }
    }
    
    fn draw_scroll_indicator<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) {
        let bounds = target.bounding_box();
        
        // Scroll bar dimensions
        const SCROLLBAR_WIDTH: u32 = 4;
        const SCROLLBAR_MARGIN: u32 = 2;
        const SCROLLBAR_TOP_MARGIN: u32 = 10; // Margin at top
        const SCROLLBAR_BOTTOM_MARGIN: u32 = 2; // Small margin at bottom
        const MIN_INDICATOR_HEIGHT: u32 = 20;
        
        // Calculate if we need a scroll bar
        let content_height = TOTAL_WORDS as i32 * (FONT_SIZE.height + VERTICAL_PAD) as i32;
        let scrollable_height = self.visible_height as i32 - SUBMIT_BUTTON_HEIGHT as i32;
        
        if content_height <= scrollable_height {
            return; // No scroll needed
        }
        
        // Calculate scroll bar position (only for scrollable area, not button)
        let scrollbar_x = bounds.size.width as i32 - (SCROLLBAR_WIDTH + SCROLLBAR_MARGIN) as i32;
        let scrollbar_y = SCROLLBAR_TOP_MARGIN as i32;
        let scrollbar_height = (self.visible_height - SUBMIT_BUTTON_HEIGHT) - SCROLLBAR_TOP_MARGIN - SCROLLBAR_BOTTOM_MARGIN;
        
        // Draw scroll track (background)
        let track_rect = Rectangle::new(
            Point::new(scrollbar_x, scrollbar_y),
            Size::new(SCROLLBAR_WIDTH, scrollbar_height)
        );
        let _ = track_rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::new(8, 8, 8))
                    .build(),
            )
            .draw(target);
        
        // Calculate indicator size and position
        let visible_ratio = scrollable_height as f32 / content_height as f32;
        let indicator_height = ((scrollbar_height as f32 * visible_ratio) as u32).max(MIN_INDICATOR_HEIGHT);
        
        let scroll_ratio = self.scroll_position as f32 / (content_height - scrollable_height) as f32;
        let indicator_y = scrollbar_y + ((scrollbar_height - indicator_height) as f32 * scroll_ratio) as i32;
        
        // Draw scroll indicator (thumb)
        let indicator_rect = RoundedRectangle::with_equal_corners(
            Rectangle::new(
                Point::new(scrollbar_x, indicator_y),
                Size::new(SCROLLBAR_WIDTH, indicator_height)
            ),
            Size::new(2, 2)
        );
        let _ = indicator_rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::new(20, 20, 20))
                    .build(),
            )
            .draw(target);
    }
}