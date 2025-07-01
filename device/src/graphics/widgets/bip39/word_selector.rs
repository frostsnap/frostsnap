use crate::bip39_words;
use crate::graphics::palette::COLORS;
use crate::graphics::widgets::FONT_LARGE;
use super::enter_bip39_share_screen::MAX_WORD_SELECTOR_WORDS;

use alloc::vec::Vec;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use u8g2_fonts::U8g2TextStyle;

#[derive(Debug)]
pub struct WordSelector {
    words: Vec<&'static str>,
    needs_redraw: bool,
    size: Size,
}

impl WordSelector {
    pub fn new(size: Size) -> Self {
        Self {
            words: Vec::new(),
            needs_redraw: true,
            size,
        }
    }

    /// Update the word list based on the current prefix
    pub fn update_words(&mut self, prefix: &str) -> usize {
        self.words.clear();
        
        if prefix.is_empty() {
            return 0;
        }
        
        // Get all words that start with the prefix
        let all_matching_words: Vec<_> = bip39_words::words_with_prefix(prefix).collect();
        let total_count = all_matching_words.len();
        
        // Only store up to MAX_WORD_SELECTOR_WORDS for display
        if total_count <= MAX_WORD_SELECTOR_WORDS {
            self.words = all_matching_words;
            self.needs_redraw = true;
        }
        
        total_count
    }
    
    
    /// Draw the word selector
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if !self.needs_redraw {
            return;
        }
        
        // Clear the entire target area first
        let bounds = Rectangle::new(Point::zero(), self.size);
        let _ = bounds
            .into_styled(PrimitiveStyle::with_fill(COLORS.background))
            .draw(target);
        
        if self.words.is_empty() {
            self.needs_redraw = false;
            return;
        }
        
        // Calculate button positions
        let button_height = self.size.height / self.words.len() as u32;
        
        // Draw each word button
        for (i, &word) in self.words.iter().enumerate() {
            let rect = Rectangle::new(
                Point::new(0, (i as u32 * button_height) as i32),
                Size::new(self.size.width, button_height),
            );
            
            // Draw word text (no border)
            let _ = Text::with_text_style(
                word,
                rect.center(),
                U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(Baseline::Middle)
                    .build(),
            )
            .draw(target);
        }
        
        self.needs_redraw = false;
    }
    
    /// Handle touch input and return a KeyTouch for the selected word
    pub fn handle_touch(&self, point: Point) -> Option<crate::graphics::widgets::KeyTouch> {
        if self.words.is_empty() {
            return None;
        }
        
        let button_height = self.size.height / self.words.len() as u32;
        
        for (i, &word) in self.words.iter().enumerate() {
            let rect = Rectangle::new(
                Point::new(0, (i as u32 * button_height) as i32),
                Size::new(self.size.width, button_height),
            );
            if rect.contains(point) {
                // Return a special key for word selection (using index as char)
                return Some(crate::graphics::widgets::KeyTouch::new(
                    char::from_digit(i as u32, 10).unwrap_or('0'),
                    rect,
                ));
            }
        }
        None
    }
    
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
    
    /// Get word by index (used when processing the key touch)
    pub fn get_word_by_index(&self, index: usize) -> Option<&'static str> {
        self.words.get(index).copied()
    }
}