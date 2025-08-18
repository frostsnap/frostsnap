use crate::palette::PALETTE;
use crate::{icons, Key, KeyTouch, FONT_LARGE};

use alloc::string::String;
use embedded_graphics::{
    geometry::AnchorX,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use u8g2_fonts::U8g2TextStyle;

#[derive(Debug)]
pub struct WordSelector {
    words: &'static [&'static str],
    prefix: String,
    needs_redraw: bool,
    size: Size,
    backspace_rect: Rectangle,
}

impl WordSelector {
    pub fn new(size: Size, words: &'static [&'static str], prefix: String) -> Self {
        // Backspace button in the same position as input preview
        let backspace_width = size.width / 4;
        let backspace_height = 60; // Same height as input preview
        let backspace_rect = Rectangle::new(
            Point::new(size.width as i32 - backspace_width as i32, 0),
            Size {
                width: backspace_width,
                height: backspace_height,
            },
        );

        Self {
            words,
            prefix,
            needs_redraw: true,
            size,
            backspace_rect,
        }
    }

    /// Get the touch rectangle for a word at the given index
    fn word_rect(&self, index: usize) -> Rectangle {
        let text_y_start = 30;
        let available_height = self.size.height - text_y_start as u32;
        
        // Calculate rows needed for two columns
        let rows_needed = (self.words.len() + 1) / 2; // +1 to round up
        let word_height = available_height / rows_needed as u32;
        
        // Determine column and row
        let column = index % 2;
        let row = index / 2;
        
        // Calculate width for each column (minus backspace button area)
        let column_width = (self.size.width - self.backspace_rect.size.width) / 2;
        
        let x_pos = (column as u32 * column_width) as i32;
        let y_pos = text_y_start + (row as u32 * word_height) as i32;
        
        Rectangle::new(
            Point::new(x_pos, y_pos),
            Size::new(column_width, word_height),
        )
    }

    /// Draw the full-screen word selector
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) {
        if !self.needs_redraw {
            return;
        }

        // Clear the entire screen
        let bounds = Rectangle::new(Point::zero(), self.size);
        let _ = bounds
            .into_styled(PrimitiveStyle::with_fill(PALETTE.background))
            .draw(target);

        // Draw backspace button
        icons::backspace()
            .with_color(PALETTE.error)
            .with_center(
                self.backspace_rect
                    .resized_width(self.backspace_rect.size.width / 2, AnchorX::Left)
                    .center(),
            )
            .draw(target);

        if self.words.is_empty() {
            self.needs_redraw = false;
            return;
        }

        // Draw each word in two columns
        for (i, &word) in self.words.iter().enumerate() {
            let rect = self.word_rect(i);
            
            // Add padding for better visual alignment
            // Left column gets more padding from left edge
            // Right column gets centered in its rectangle
            let column = i % 2;
            let padding_x = if column == 0 {
                20 // Left column padding
            } else {
                10 // Right column padding (closer to center)
            };
            
            let text_pos = Point::new(rect.top_left.x + padding_x, rect.center().y);

            // First draw the full word in green (same as progress bar)
            let _ = Text::with_text_style(
                word,
                text_pos,
                U8g2TextStyle::new(FONT_LARGE, PALETTE.tertiary), // Green color
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Middle)
                    .build(),
            )
            .draw(target);

            // Then draw the prefix in primary color on top (if we have a prefix)
            if !self.prefix.is_empty() {
                let _ = Text::with_text_style(
                    &self.prefix,
                    text_pos,
                    U8g2TextStyle::new(FONT_LARGE, PALETTE.on_background),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Left)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
        }

        self.needs_redraw = false;
    }

    /// Handle touch input and return a KeyTouch for the selected word or backspace
    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        // Check backspace button first
        if self.backspace_rect.contains(point) {
            return Some(KeyTouch::new(Key::Keyboard('⌫'), self.backspace_rect));
        }

        // Check word buttons using word_rect function
        for (i, _) in self.words.iter().enumerate() {
            let rect = self.word_rect(i);
            if rect.contains(point) {
                // Return a WordSelector key with the index
                return Some(KeyTouch::new(Key::WordSelector(i), rect));
            }
        }
        None
    }

    /// Get word by index (used when processing the key touch)
    pub fn get_word_by_index(&self, index: usize) -> Option<&'static str> {
        self.words.get(index).copied()
    }
}
