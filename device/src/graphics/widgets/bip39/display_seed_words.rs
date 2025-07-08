use crate::graphics::palette::COLORS;
use crate::graphics::widgets::{icons, Key, KeyTouch, FONT_LARGE, FONT_SMALL};
use alloc::{string::ToString, vec::Vec};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_iconoir::size32px::navigation::{NavArrowLeft, NavArrowRight};
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::{fonts, U8g2TextStyle};

const BUTTON_SIZE: u32 = 60;
const WORDS_PER_PAGE: usize = 3;

#[derive(Debug)]
pub struct DisplaySeedWords {
    words: [&'static str; 25],
    share_index: u16,
    current_index: usize,
    area: Rectangle,
    prev_button_rect: Rectangle,
    next_button_rect: Rectangle,
    touches: Vec<KeyTouch>,
    needs_redraw: bool,
}

impl DisplaySeedWords {
    pub fn new(area: Size, words: [&'static str; 25], share_index: u16) -> Self {
        let area_rect = Rectangle::new(Point::zero(), area);
        
        // Calculate button positions - close to bottom
        let button_y = area.height - 55;
        let button_spacing = 120;
        let center_x = area.width / 2;
        
        let prev_button_rect = Rectangle::new(
            Point::new((center_x - button_spacing / 2 - BUTTON_SIZE / 2) as i32, button_y as i32),
            Size::new(BUTTON_SIZE, BUTTON_SIZE),
        );
        let next_button_rect = Rectangle::new(
            Point::new((center_x + button_spacing / 2 - BUTTON_SIZE / 2) as i32, button_y as i32),
            Size::new(BUTTON_SIZE, BUTTON_SIZE),
        );
        
        Self {
            words,
            share_index,
            current_index: 0,
            area: area_rect,
            prev_button_rect,
            next_button_rect,
            touches: Vec::new(),
            needs_redraw: true,
        }
    }
    
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D, current_time: crate::Instant) {
        if self.needs_redraw {
            // Clear background
            let _ = self.area
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.background)
                        .build(),
                )
                .draw(target);
            
            if self.current_index == 0 {
                // First page: show share index
                let center_y = self.area.center().y;
                
                // Draw "Share number" label above in medium font
                let _ = Text::with_text_style(
                    "Share number",
                    Point::new(self.area.center().x, center_y - 60),
                    U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, COLORS.primary),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
                
                // Draw share index number in extra large text
                let share_text = format!("#{}", self.share_index);
                let _ = Text::with_text_style(
                    &share_text,
                    Point::new(self.area.center().x, center_y),
                    U8g2TextStyle::new(fonts::u8g2_font_inr42_mf, COLORS.primary),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
                
                // Draw "Write it down" below
                let _ = Text::with_text_style(
                    "Write it down",
                    Point::new(self.area.center().x, center_y + 50),
                    U8g2TextStyle::new(FONT_SMALL, COLORS.primary),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            } else {
                // Other pages: show words
                let start_y = 60;
                let line_spacing = 65;
                let number_x = 65; // Fixed x position for numbers
                let dot_x = 70;    // Fixed x position for dots
                
                // Calculate which words to show (accounting for share index page)
                let word_start_index = (self.current_index - 1) * WORDS_PER_PAGE;
                
                // Show up to 3 words
                for i in 0..WORDS_PER_PAGE {
                    let word_index = word_start_index + i;
                    if word_index < FROSTSNAP_BACKUP_WORDS {
                        let y_pos = start_y + (i as i32 * line_spacing);
                        
                        // Draw the number right-aligned
                        let number_text = format!("{}", word_index + 1);
                    let _ = Text::with_text_style(
                        &number_text,
                        Point::new(number_x, y_pos),
                        U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
                        TextStyleBuilder::new()
                            .alignment(Alignment::Right)
                            .baseline(Baseline::Middle)
                            .build(),
                    )
                    .draw(target);
                    
                        // Draw the dot
                        let _ = Text::with_text_style(
                            ".",
                            Point::new(dot_x, y_pos),
                            U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
                            TextStyleBuilder::new()
                                .alignment(Alignment::Center)
                                .baseline(Baseline::Middle)
                                .build(),
                        )
                        .draw(target);
                        
                        // Draw the word
                        let _ = Text::with_text_style(
                            self.words[word_index],
                            Point::new(dot_x + 10, y_pos),
                            U8g2TextStyle::new(FONT_LARGE, COLORS.primary),
                            TextStyleBuilder::new()
                                .alignment(Alignment::Left)
                                .baseline(Baseline::Middle)
                                .build(),
                        )
                        .draw(target);
                    }
                }
            }
            
            // Draw previous button (only if not at first word)
            if self.current_index > 0 {
                self.draw_prev_button(target);
            }
            
            // Draw next button (only if not on last page)
            // Total pages = 1 (share index) + 9 (word pages)
            if self.current_index < 9 {
                self.draw_next_button(target);
            }
            
            // Draw page counter between buttons
            let current_page = self.current_index + 1;  // Pages are 1-indexed
            let total_pages = 1 + (FROSTSNAP_BACKUP_WORDS + WORDS_PER_PAGE - 1) / WORDS_PER_PAGE; // 1 + 9 = 10
            let counter_text = format!("{}/{}", current_page, total_pages);
            let counter_position = Point::new(
                self.area.center().x,
                self.prev_button_rect.center().y
            );
            
            let _ = Text::with_text_style(
                &counter_text,
                counter_position,
                U8g2TextStyle::new(FONT_SMALL, COLORS.primary),
                TextStyleBuilder::new()
                    .alignment(Alignment::Center)
                    .baseline(Baseline::Middle)
                    .build(),
            )
            .draw(target);
            
            self.needs_redraw = false;
        }
        
        // Draw touches
        for touch in &mut self.touches {
            touch.draw(target, current_time);
        }
        
        // Remove finished touches
        self.touches.retain(|touch| !touch.is_finished());
    }
    
    fn draw_prev_button<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) {
        // Draw left arrow icon
        icons::Icon::<NavArrowLeft>::default()
            .with_color(COLORS.primary)
            .with_center(self.prev_button_rect.center())
            .draw(target);
    }
    
    fn draw_next_button<D: DrawTarget<Color = Rgb565>>(&self, target: &mut D) {
        // Draw right arrow icon
        icons::Icon::<NavArrowRight>::default()
            .with_color(COLORS.primary)
            .with_center(self.next_button_rect.center())
            .draw(target);
    }
    
    
    pub fn handle_touch(&mut self, point: Point, current_time: crate::Instant, lift_up: bool) {
        if lift_up {
            // Process button release
            if let Some(active_touch) = self.touches.iter_mut().rev().find(|t| !t.has_been_let_go()) {
                if let Some(key) = active_touch.let_go(current_time) {
                    match key {
                        Key::NavBack => self.navigate_prev(),
                        Key::NavForward => self.navigate_next(),
                        _ => {}
                    }
                }
            }
        } else {
            // Handle new touch
            let key_touch = if self.current_index > 0 && self.prev_button_rect.contains(point) {
                Some(KeyTouch::new(Key::NavBack, self.prev_button_rect))
            } else if self.current_index < 9 && self.next_button_rect.contains(point) {
                Some(KeyTouch::new(Key::NavForward, self.next_button_rect))
            } else {
                None
            };
            
            if let Some(key_touch) = key_touch {
                // Cancel any existing touch and add new one
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
    }
    
    pub fn navigate_prev(&mut self) {
        if self.current_index > 0 {
            self.current_index -= 1;
            self.needs_redraw = true;
        }
    }
    
    pub fn navigate_next(&mut self) {
        if self.current_index < 9 {
            self.current_index += 1;
            self.needs_redraw = true;
        }
    }
}