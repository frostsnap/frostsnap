use crate::{page_by_page::PageByPage, Widget, FONT_LARGE, FONT_MED, FONT_SMALL};
use alloc::format;
use embedded_graphics::{
    pixelcolor::Gray2,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::{FROSTSNAP_BACKUP_WORDS, BIP39_WORDS};
use u8g2_fonts::{fonts, U8g2TextStyle};

const WORDS_PER_PAGE: usize = 3;

/// A widget that displays BIP39 backup words using vertical pagination
/// First page shows the share index, then subsequent pages show 3 words each
pub struct Bip39BackupDisplay {
    word_indices: [u16; 25],
    share_index: u16,
    current_page: usize,
    total_pages: usize,
    size: Size,
}

impl Bip39BackupDisplay {
    pub fn new(size: Size, word_indices: [u16; 25], share_index: u16) -> Self {
        // Calculate total pages: 1 share index page + word pages
        let total_pages = 1 + (FROSTSNAP_BACKUP_WORDS + WORDS_PER_PAGE - 1) / WORDS_PER_PAGE;
        
        Self {
            word_indices,
            share_index,
            current_page: 7,
            total_pages,
            size,
        }
    }

    fn draw_share_index_page<D: DrawTarget<Color = Gray2>>(
        &self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        let center_x = (self.size.width / 2) as i32;
        let center_y = (self.size.height / 2) as i32;

        // Draw "Share number" label above in medium font (secondary color)
        Text::with_text_style(
            "Share number",
            Point::new(center_x, center_y - 60),
            U8g2TextStyle::new(FONT_MED, Gray2::new(1)), // Gray level 1 for secondary text
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(target)?;

        // Draw share index number in extra large text (emphasized with bright color)
        let share_text = format!("#{}", self.share_index);
        Text::with_text_style(
            &share_text,
            Point::new(center_x, center_y),
            U8g2TextStyle::new(fonts::u8g2_font_inr42_mf, Gray2::new(3)), // Bright white for emphasis
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(target)?;

        // Draw "Write it down" below (secondary color)
        Text::with_text_style(
            "Write it down",
            Point::new(center_x, center_y + 50),
            U8g2TextStyle::new(FONT_SMALL, Gray2::new(1)), // Gray level 1 for secondary text
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(target)?;


        Ok(())
    }

    fn draw_words_page<D: DrawTarget<Color = Gray2>>(
        &self,
        target: &mut D,
        page_index: usize,
    ) -> Result<(), D::Error> {
        let line_spacing = 55;
        let total_height = (WORDS_PER_PAGE as i32 - 1) * line_spacing;
        let start_y = (self.size.height as i32) / 2 - total_height / 2;
        
        let number_x = 65; // Fixed x position for numbers
        let dot_x = 70; // Fixed x position for dots

        // Calculate which words to show (accounting for share index page)
        let word_start_index = (page_index - 1) * WORDS_PER_PAGE;

        // Show up to 3 words
        for i in 0..WORDS_PER_PAGE {
            let word_index = word_start_index + i;
            if word_index < FROSTSNAP_BACKUP_WORDS {
                let y_pos = start_y + (i as i32 * line_spacing);

                // Draw the number right-aligned (secondary color)
                let number_text = format!("{}", word_index + 1);
                Text::with_text_style(
                    &number_text,
                    Point::new(number_x, y_pos),
                    U8g2TextStyle::new(FONT_MED, Gray2::new(1)), // Smaller font, gray for numbers
                    TextStyleBuilder::new()
                        .alignment(Alignment::Right)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target)?;

                // Draw the dot (secondary color)
                Text::with_text_style(
                    ".",
                    Point::new(dot_x, y_pos),
                    U8g2TextStyle::new(FONT_MED, Gray2::new(1)), // Gray for dot
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target)?;

                // Draw the word (emphasized with bright color and larger font)
                let word = BIP39_WORDS[self.word_indices[word_index] as usize];
                Text::with_text_style(
                    word,
                    Point::new(dot_x + 10, y_pos),
                    U8g2TextStyle::new(FONT_LARGE, Gray2::new(3)), // Large font, bright white for emphasis
                    TextStyleBuilder::new()
                        .alignment(Alignment::Left)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target)?;
            }
        }


        Ok(())
    }
}

impl crate::DynWidget for Bip39BackupDisplay {
    fn handle_touch(&mut self, _point: Point, _current_time: crate::Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag behavior
    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
    
    fn force_full_redraw(&mut self) {
        // No state to reset
    }
}

impl Widget for Bip39BackupDisplay {
    type Color = Gray2;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Clear the background
        target.clear(Gray2::new(0))?;

        // Draw the current page
        if self.current_page == 0 {
            self.draw_share_index_page(target)?;
        } else {
            self.draw_words_page(target, self.current_page)?;
        }

        Ok(())
    }
}

impl PageByPage for Bip39BackupDisplay {
    fn has_next_page(&self) -> bool {
        self.current_page < self.total_pages - 1
    }

    fn has_prev_page(&self) -> bool {
        self.current_page > 0
    }

    fn next_page(&mut self) {
        if self.has_next_page() {
            self.current_page += 1;
        }
    }

    fn prev_page(&mut self) {
        if self.has_prev_page() {
            self.current_page -= 1;
        }
    }
    
    fn current_page(&self) -> usize {
        self.current_page
    }
    
    fn total_pages(&self) -> usize {
        self.total_pages
    }
}
