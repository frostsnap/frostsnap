use crate::super_draw_target::SuperDrawTarget;
use crate::{palette::PALETTE, FONT_LARGE, FONT_SMALL};
use alloc::{boxed::Box, format};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::U8g2TextStyle;

pub const SUBMIT_BUTTON_HEIGHT: u32 = 80; // Height of the button area
pub const SUBMIT_BUTTON_WIDTH: u32 = 240; // Full screen width

#[derive(Debug)]
pub enum SubmitBackupState {
    Complete {
        words: Box<[&'static str; FROSTSNAP_BACKUP_WORDS]>,
    },
    Incomplete {
        words_entered: usize,
    },
    InvalidChecksum,
}

#[derive(Debug)]
pub struct SubmitBackupButton {
    bounds: Rectangle,
    state: SubmitBackupState,
    button_rect: RoundedRectangle,
}

impl SubmitBackupButton {
    pub fn new(bounds: Rectangle, state: SubmitBackupState) -> Self {
        // Create the rounded rectangle once
        let button_rect = RoundedRectangle::with_equal_corners(
            Rectangle::new(
                bounds.top_left + Point::new(4, 4),
                Size::new(bounds.size.width - 8, bounds.size.height - 8),
            ),
            Size::new(35, 35), // 35px corner radius
        );

        Self {
            bounds,
            state,
            button_rect,
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        bounds: Rectangle,
    ) -> Result<(), D::Error> {
        // Clear the background
        bounds
            .into_styled(PrimitiveStyle::with_fill(PALETTE.background))
            .draw(target)?;

        match &self.state {
            SubmitBackupState::Complete { .. } => {
                // Fill button with success color
                self.button_rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.tertiary_container)
                            .stroke_color(PALETTE.tertiary)
                            .stroke_width(2)
                            .build(),
                    )
                    .draw(target)?;

                let _ = Text::with_text_style(
                    "SUBMIT",
                    bounds.center(),
                    U8g2TextStyle::new(FONT_LARGE, PALETTE.on_tertiary_container),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target)?;
            }
            SubmitBackupState::Incomplete { words_entered } => {
                // Fill button with disabled color
                self.button_rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.surface_variant)
                            .stroke_color(PALETTE.outline)
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(target)?;

                // Draw count text
                let count_text = format!("{}/{}", words_entered, FROSTSNAP_BACKUP_WORDS);
                let _ = Text::with_text_style(
                    &count_text,
                    bounds.center(),
                    U8g2TextStyle::new(FONT_LARGE, PALETTE.on_surface_variant),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target)?;
            }
            SubmitBackupState::InvalidChecksum => {
                // Fill button with disabled color
                self.button_rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(PALETTE.surface_variant)
                            .stroke_color(PALETTE.outline)
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(target)?;

                // Draw error text
                let _ = Text::with_text_style(
                    "Invalid checksum",
                    bounds.center(),
                    U8g2TextStyle::new(FONT_SMALL, PALETTE.error),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(target)?;
            }
        }

        Ok(())
    }

    pub fn update_state(&mut self, new_state: SubmitBackupState) -> bool {
        // Check if state actually changed
        let changed = match (&self.state, &new_state) {
            (SubmitBackupState::Complete { .. }, SubmitBackupState::Complete { .. }) => false,
            (
                SubmitBackupState::Incomplete { words_entered: a },
                SubmitBackupState::Incomplete { words_entered: b },
            ) => a != b,
            (SubmitBackupState::InvalidChecksum, SubmitBackupState::InvalidChecksum) => false,
            _ => true,
        };

        if changed {
            self.state = new_state;
        }

        changed
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.state, SubmitBackupState::Complete { .. })
    }

    pub fn handle_touch(&self, point: Point) -> bool {
        // Handle touch for the entire button area, but only return true if complete
        self.bounds.contains(point) && self.is_complete()
    }
}
