use crate::palette::PALETTE;
use crate::DefaultTextStyle;
use crate::{any_of::AnyOf, prelude::*, FONT_MED, FONT_SMALL};
use alloc::string::ToString;
use frost_backup::NUM_WORDS;

pub const STATUS_BAR_HEIGHT: u32 = 55;

#[derive(Debug, Clone)]
pub enum BackupStatus {
    Incomplete { words_entered: usize },
    InvalidChecksum,
    Valid,
}

// Widget for incomplete status
type IncompleteWidget = Container<Center<Column<(Text, Text)>>>;

// Widget for invalid checksum status
type InvalidChecksumWidget = Container<Center<Column<(Text, Text)>>>;

// Widget for valid status
type ValidWidget = Container<Center<Column<(Text, Text)>>>;

#[derive(frostsnap_macros::Widget)]
pub struct BackupStatusBar {
    widget: AnyOf<(IncompleteWidget, InvalidChecksumWidget, ValidWidget)>,
}

impl BackupStatusBar {
    pub fn new(status: BackupStatus) -> Self {
        match status {
            BackupStatus::Incomplete { words_entered } => {
                let text = if words_entered == 0 {
                    "Enter backup words".to_string()
                } else {
                    format!("{}/{} words entered", words_entered, NUM_WORDS)
                };

                let main_text = Text::new(
                    text,
                    DefaultTextStyle::new(FONT_MED, PALETTE.on_surface_variant),
                )
                .with_alignment(embedded_graphics::text::Alignment::Center);

                let hint_text = Text::new(
                    "Tap word to edit",
                    DefaultTextStyle::new(FONT_SMALL, PALETTE.on_surface_variant),
                )
                .with_alignment(embedded_graphics::text::Alignment::Center);

                use crate::layout::MainAxisAlignment;
                let column = Column::new((main_text, hint_text))
                    .with_main_axis_alignment(MainAxisAlignment::Center);

                let center = Center::new(column);
                let mut container = Container::new(center).with_border(PALETTE.surface_variant, 2);
                container.set_fill(PALETTE.surface_variant);

                Self {
                    widget: AnyOf::new(container),
                }
            }
            BackupStatus::InvalidChecksum => {
                // Create column with two text elements
                let invalid_text = Text::new(
                    "Invalid Backup",
                    DefaultTextStyle::new(FONT_MED, PALETTE.on_error),
                )
                .with_alignment(embedded_graphics::text::Alignment::Center);

                let tap_text = Text::new(
                    "Tap word to edit",
                    DefaultTextStyle::new(FONT_SMALL, PALETTE.on_error),
                )
                .with_alignment(embedded_graphics::text::Alignment::Center);

                use crate::layout::MainAxisAlignment;
                let column = Column::new((invalid_text, tap_text))
                    .with_main_axis_alignment(MainAxisAlignment::Center);

                let center = Center::new(column);
                let mut container = Container::new(center).with_border(PALETTE.error, 2);
                container.set_fill(PALETTE.error);

                Self {
                    widget: AnyOf::new(container),
                }
            }
            BackupStatus::Valid => {
                let main_text = Text::new(
                    "Success",
                    DefaultTextStyle::new(FONT_MED, PALETTE.on_tertiary_container),
                )
                .with_alignment(embedded_graphics::text::Alignment::Center);

                let sub_text = Text::new(
                    "Valid Backup",
                    DefaultTextStyle::new(FONT_SMALL, PALETTE.on_tertiary_container),
                )
                .with_alignment(embedded_graphics::text::Alignment::Center);

                use crate::layout::MainAxisAlignment;
                let column = Column::new((main_text, sub_text))
                    .with_main_axis_alignment(MainAxisAlignment::Center);

                let center = Center::new(column);
                let mut container =
                    Container::new(center).with_border(PALETTE.tertiary_container, 2);
                container.set_fill(PALETTE.tertiary_container);

                Self {
                    widget: AnyOf::new(container),
                }
            }
        }
    }
}
