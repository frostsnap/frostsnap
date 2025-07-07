use crate::graphics::{widgets::FONT_SMALL, palette::COLORS};
use embedded_graphics::{
    pixelcolor::{Gray2, Rgb565},
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::U8g2TextStyle;

#[derive(Debug)]
pub enum SubmitBackupState {
    Complete { words: [&'static str; FROSTSNAP_BACKUP_WORDS] },
    Incomplete { words_entered: usize },
    InvalidChecksum,
}

#[derive(Debug)]
pub struct SubmitBackupButton {
    bounds: Rectangle,
    state: SubmitBackupState,
}

pub const SUBMIT_BUTTON_HEIGHT: u32 = 80; // Height of the button area

impl SubmitBackupButton {
    
    pub fn new(bounds: Rectangle, state: SubmitBackupState) -> Self {
        Self { bounds, state }
    }
    
    pub fn draw(&self, target: &mut impl DrawTarget<Color = Gray2>) {
        // Clear background
        let _ = self.bounds
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Gray2::BLACK)
                    .build(),
            )
            .draw(target);
        
        match &self.state {
            SubmitBackupState::Complete { .. } => {
                // Draw "Submit" button with light gray
                let button_rect = Rectangle::new(
                    Point::new(
                        self.bounds.center().x - 60,
                        self.bounds.center().y - 20,
                    ),
                    Size::new(120, 40),
                );
                
                let _ = RoundedRectangle::with_equal_corners(
                    button_rect,
                    Size::new(8, 8),
                )
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(Gray2::new(0x03)) // Brightest gray
                        .build(),
                )
                .draw(target);
                
                let _ = Text::with_text_style(
                    "Submit",
                    button_rect.center(),
                    U8g2TextStyle::new(FONT_SMALL, Gray2::BLACK),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
            SubmitBackupState::Incomplete { words_entered } => {
                // Draw error text in medium gray
                let error_text = format!("Enter all {} words\n({}/{} completed)", FROSTSNAP_BACKUP_WORDS, words_entered, FROSTSNAP_BACKUP_WORDS);
                let _ = Text::with_text_style(
                    &error_text,
                    self.bounds.center(),
                    U8g2TextStyle::new(FONT_SMALL, Gray2::new(0x02)),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
            SubmitBackupState::InvalidChecksum => {
                // Draw error text in medium gray
                let _ = Text::with_text_style(
                    "Invalid checksum!\nDouble-check words",
                    self.bounds.center(),
                    U8g2TextStyle::new(FONT_SMALL, Gray2::new(0x02)),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
        }
    }
    
    pub fn is_complete(&self) -> bool {
        matches!(self.state, SubmitBackupState::Complete { .. })
    }
    
    pub fn handle_touch(&self, point: Point) -> bool {
        // Only handle touch if we're in complete state
        if self.bounds.contains(point) && self.is_complete() {
            true
        } else {
            false
        }
    }
    
    pub fn draw_rgb(&self, target: &mut impl DrawTarget<Color = Rgb565>, bounds: Rectangle) {
        // Clear background
        let _ = bounds
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(COLORS.background)
                    .build(),
            )
            .draw(target);
        
        match &self.state {
            SubmitBackupState::Complete { .. } => {
                // Draw "Submit" button with green
                let button_rect = Rectangle::new(
                    Point::new(
                        bounds.center().x - 60,
                        bounds.center().y - 20,
                    ),
                    Size::new(120, 40),
                );
                
                let _ = RoundedRectangle::with_equal_corners(
                    button_rect,
                    Size::new(8, 8),
                )
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(COLORS.success)
                        .build(),
                )
                .draw(target);
                
                let _ = Text::with_text_style(
                    "Submit",
                    button_rect.center(),
                    U8g2TextStyle::new(FONT_SMALL, COLORS.background),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
            SubmitBackupState::Incomplete { words_entered } => {
                // Draw error text
                let error_text = format!("Enter all {} words\n({}/{} completed)", FROSTSNAP_BACKUP_WORDS, words_entered, FROSTSNAP_BACKUP_WORDS);
                let _ = Text::with_text_style(
                    &error_text,
                    bounds.center(),
                    U8g2TextStyle::new(FONT_SMALL, COLORS.error),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
            SubmitBackupState::InvalidChecksum => {
                // Draw error text
                let _ = Text::with_text_style(
                    "Invalid checksum!\nDouble-check words",
                    bounds.center(),
                    U8g2TextStyle::new(FONT_SMALL, COLORS.error),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target);
            }
        }
    }
}