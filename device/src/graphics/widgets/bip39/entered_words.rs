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

use super::bip39_input_preview::{Fb, FONT_SIZE, FB_WIDTH, FB_HEIGHT, TOTAL_WORDS, VERTICAL_PAD};

#[derive(Debug)]
pub struct EnteredWords {
    framebuffer: Rc<RefCell<Fb>>,
    scroll_position: i32,
    visible_height: u32,
    visible_width: u32,
    needs_redraw: bool,
}

impl EnteredWords {
    pub fn new(framebuffer: Rc<RefCell<Fb>>, visible_size: Size) -> Self {
        Self {
            framebuffer,
            scroll_position: 0,
            visible_height: visible_size.height,
            visible_width: visible_size.width,
            needs_redraw: true,
        }
    }
    
    pub fn new_with_word_at_bottom(framebuffer: Rc<RefCell<Fb>>, visible_size: Size, word_index: usize) -> Self {
        let row_height = FONT_SIZE.height + VERTICAL_PAD;
        
        // Calculate scroll to show word at bottom of screen
        let word_bottom = (word_index + 1) as i32 * row_height as i32;
        let desired_scroll = word_bottom - visible_size.height as i32;
        
        // Clamp to valid scroll range
        let max_scroll = (FB_HEIGHT as i32).saturating_sub(visible_size.height as i32);
        let scroll_position = desired_scroll.clamp(0, max_scroll);
        
        Self {
            framebuffer,
            scroll_position,
            visible_height: visible_size.height,
            visible_width: visible_size.width,
            needs_redraw: true,
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if !self.needs_redraw {
            return;
        }

        let bounds = target.bounding_box();

        // Calculate horizontal offset to center the content
        let x_offset = ((bounds.size.width as i32 - FB_WIDTH as i32) / 2).max(0);

        // Create a cropped target that matches the framebuffer width, centered
        let cropped_rect = Rectangle::new(
            Point::new(x_offset, 0),
            Size::new(FB_WIDTH, bounds.size.height)
        );
        
        let mut cropped_target = target.cropped(&cropped_rect);
        let cropped_bounds = cropped_target.bounding_box();

        // Calculate skip pixels based on scroll position
        let skip_pixels = (self.scroll_position.max(0) as usize) * FB_WIDTH as usize;
        let take_pixels = cropped_bounds.size.height as usize * cropped_bounds.size.width as usize;

        // Borrow the framebuffer for reading
        let fb = self.framebuffer.borrow();
        
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

        let _ = cropped_target.fill_contiguous(&cropped_bounds, framebuffer_pixels);
        
        // Fill the remaining areas on both sides with background color
        if x_offset > 0 {
            // Left side
            let left_rect = Rectangle::new(
                Point::zero(),
                Size::new(x_offset as u32, bounds.size.height)
            );
            let _ = left_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.background)
                        .build(),
                )
                .draw(target);
            
            // Right side
            let right_rect = Rectangle::new(
                Point::new(x_offset + FB_WIDTH as i32, 0),
                Size::new(x_offset as u32, bounds.size.height)
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
        // Calculate horizontal offset for centered content
        let x_offset = ((self.visible_width as i32 - FB_WIDTH as i32) / 2).max(0);
        
        // Check if touch is within the content area
        if point.x >= x_offset && point.x < x_offset + FB_WIDTH as i32 {
            // Calculate which word was touched using row height with padding
            let row_height = (FONT_SIZE.height + VERTICAL_PAD) as i32;
            let word_index = ((point.y + self.scroll_position) / row_height) as usize;
            
            if word_index < TOTAL_WORDS {
                // Create a rectangle for the touched word (includes padding)
                let y = (word_index as i32 * row_height) - self.scroll_position;
                let rect = Rectangle::new(
                    Point::new(x_offset, y),
                    Size::new(FB_WIDTH, FONT_SIZE.height + VERTICAL_PAD)
                );
                
                Some(KeyTouch::new(Key::EditWord(word_index), rect))
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
        let max_scroll = (FB_HEIGHT as i32).saturating_sub(self.visible_height as i32);
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
        const SCROLLBAR_TOP_BOTTOM_MARGIN: u32 = 20; // Extra margin for rounded screen edges
        const MIN_INDICATOR_HEIGHT: u32 = 20;
        
        // Calculate if we need a scroll bar
        let content_height = FB_HEIGHT as i32;
        let visible_height = self.visible_height as i32;
        
        if content_height <= visible_height {
            return; // No scroll needed
        }
        
        // Calculate scroll bar position
        let scrollbar_x = bounds.size.width as i32 - (SCROLLBAR_WIDTH + SCROLLBAR_MARGIN) as i32;
        let scrollbar_y = SCROLLBAR_TOP_BOTTOM_MARGIN as i32;
        let scrollbar_height = bounds.size.height - 2 * SCROLLBAR_TOP_BOTTOM_MARGIN;
        
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
        let visible_ratio = visible_height as f32 / content_height as f32;
        let indicator_height = ((scrollbar_height as f32 * visible_ratio) as u32).max(MIN_INDICATOR_HEIGHT);
        
        let scroll_ratio = self.scroll_position as f32 / (content_height - visible_height) as f32;
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