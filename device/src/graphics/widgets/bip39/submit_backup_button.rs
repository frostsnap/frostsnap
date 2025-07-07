use crate::graphics::{
    palette::COLORS,
    widgets::{FONT_LARGE, FONT_SMALL},
};
use alloc::{boxed::Box, format};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::U8g2TextStyle;

pub const SUBMIT_BUTTON_HEIGHT: u32 = 80; // Height of the button area
pub const SUBMIT_BUTTON_WIDTH: u32 = 240; // Full screen width

// Framebuffer type for the button
type ButtonFb = Framebuffer<
    Gray2,
    RawU2,
    LittleEndian,
    { SUBMIT_BUTTON_WIDTH as usize },
    { SUBMIT_BUTTON_HEIGHT as usize },
    { buffer_size::<Gray2>(SUBMIT_BUTTON_WIDTH as usize, SUBMIT_BUTTON_HEIGHT as usize) },
>;

#[derive(Debug)]
pub enum SubmitBackupState {
    Complete {
        words: [&'static str; FROSTSNAP_BACKUP_WORDS],
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
    framebuffer: Box<ButtonFb>,
}

impl SubmitBackupButton {
    pub fn new(bounds: Rectangle, state: SubmitBackupState) -> Self {
        let mut button = Self {
            bounds,
            state,
            framebuffer: Box::new(ButtonFb::new()),
        };
        button.render_to_framebuffer();
        button
    }

    fn render_to_framebuffer(&mut self) {
        // Clear framebuffer to background
        let _ = self.framebuffer.clear(Gray2::BLACK);

        match &self.state {
            SubmitBackupState::Complete { .. } => {
                // Fill entire button area with success color
                let button_rect = RoundedRectangle::with_equal_corners(
                    Rectangle::new(
                        Point::new(4, 4),
                        Size::new(SUBMIT_BUTTON_WIDTH - 8, SUBMIT_BUTTON_HEIGHT - 8),
                    ),
                    Size::new(35, 35), // 35px corner radius
                );

                let _ = button_rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(Gray2::new(0x03)) // Brightest gray for success
                            .stroke_color(Gray2::new(0x02))
                            .stroke_width(2)
                            .build(),
                    )
                    .draw(&mut *self.framebuffer);

                let _ = Text::with_text_style(
                    "SUBMIT",
                    Point::new(
                        (SUBMIT_BUTTON_WIDTH / 2) as i32,
                        (SUBMIT_BUTTON_HEIGHT / 2) as i32,
                    ),
                    U8g2TextStyle::new(FONT_LARGE, Gray2::BLACK),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(&mut *self.framebuffer);
            }
            SubmitBackupState::Incomplete { words_entered } => {
                // Fill entire button area with disabled gray
                let button_rect = RoundedRectangle::with_equal_corners(
                    Rectangle::new(
                        Point::new(4, 4),
                        Size::new(SUBMIT_BUTTON_WIDTH - 8, SUBMIT_BUTTON_HEIGHT - 8),
                    ),
                    Size::new(35, 35), // 35px corner radius
                );

                let _ = button_rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(Gray2::new(0x01)) // Dark gray for disabled
                            .stroke_color(Gray2::new(0x02))
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(&mut *self.framebuffer);

                // Draw count in large text
                let count_text = format!("{}/{}", words_entered, FROSTSNAP_BACKUP_WORDS);
                let _ = Text::with_text_style(
                    &count_text,
                    Point::new(
                        (SUBMIT_BUTTON_WIDTH / 2) as i32,
                        (SUBMIT_BUTTON_HEIGHT / 2) as i32,
                    ),
                    U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)), // Medium gray for text
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(&mut *self.framebuffer);
            }
            SubmitBackupState::InvalidChecksum => {
                // Fill entire button area with disabled gray
                let button_rect = RoundedRectangle::with_equal_corners(
                    Rectangle::new(
                        Point::new(4, 4),
                        Size::new(SUBMIT_BUTTON_WIDTH - 8, SUBMIT_BUTTON_HEIGHT - 8),
                    ),
                    Size::new(35, 35), // 35px corner radius
                );

                let _ = button_rect
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(Gray2::new(0x01)) // Dark gray for disabled
                            .stroke_color(Gray2::new(0x02))
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(&mut *self.framebuffer);

                // Draw error text
                let _ = Text::with_text_style(
                    "Invalid checksum",
                    Point::new(
                        (SUBMIT_BUTTON_WIDTH / 2) as i32,
                        (SUBMIT_BUTTON_HEIGHT / 2) as i32,
                    ),
                    U8g2TextStyle::new(FONT_SMALL, Gray2::new(0x02)), // Medium gray for text
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(&mut *self.framebuffer);
            }
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &self,
        target: &mut D,
        bounds: Rectangle,
    ) -> Result<(), D::Error> {
        // Always draw - the framebuffer contains the rendered content
        // Map Gray2 values to RGB colors
        let fb_data = self.framebuffer.data();
        let pixels = RawDataSlice::<RawU2, LittleEndian>::new(fb_data)
            .into_iter()
            .map(|pixel| match Gray2::from(pixel).luma() {
                0x01 => Rgb565::new(6, 12, 6), // Dark gray -> disabled gray (neutral gray in RGB565)
                0x02 => COLORS.primary,        // Medium gray -> normal text
                0x03 => COLORS.success,        // Bright gray -> success green
                _ => COLORS.background,
            });

        target.fill_contiguous(&bounds, pixels)
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
            self.render_to_framebuffer();
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
