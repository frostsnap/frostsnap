use crate::palette::PALETTE;
use crate::{Center, Container, Text, FONT_MED};
use alloc::string::ToString;
use embedded_graphics::pixelcolor::Rgb565;
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::U8g2TextStyle;

pub const STATUS_BAR_HEIGHT: u32 = 40;

#[derive(Debug, Clone)]
pub enum BackupStatus {
    Incomplete { words_entered: usize },
    #[allow(dead_code)]
    InvalidChecksum,
    Valid,
}

#[derive(frostsnap_macros::Widget)]
pub struct BackupStatusBar {
    container: Container<Center<Text<U8g2TextStyle<Rgb565>>>>,
}

impl BackupStatusBar {
    pub fn new(status: BackupStatus) -> Self {
        Self::from_status(status)
    }

    fn from_status(status: BackupStatus) -> Self {
        // Get background color and text based on status
        let (bg_color, text, text_color) = match status {
            BackupStatus::Incomplete { words_entered } => {
                let text = if words_entered == 0 {
                    "Enter backup words".to_string()
                } else {
                    format!("{}/{} words entered", words_entered, FROSTSNAP_BACKUP_WORDS)
                };
                (PALETTE.surface_variant, text, PALETTE.on_surface_variant)
            }
            BackupStatus::InvalidChecksum => {
                (PALETTE.error, "Invalid checksum - check words".to_string(), PALETTE.on_error)
            }
            BackupStatus::Valid => {
                (PALETTE.tertiary_container, "Backup valid ✓".to_string(), PALETTE.on_tertiary_container)
            }
        };

        // Create text widget with FONT_MED
        let text_style = U8g2TextStyle::new(FONT_MED, text_color);
        let text_widget = Text::new(text, text_style)
            .with_alignment(embedded_graphics::text::Alignment::Center);

        // Create center widget
        let center = Center::new(text_widget);

        // Create container with fill and fixed height
        let mut container = Container::new(center)
            .with_height(STATUS_BAR_HEIGHT);
        container.set_fill(bg_color);

        Self { container }
    }

}