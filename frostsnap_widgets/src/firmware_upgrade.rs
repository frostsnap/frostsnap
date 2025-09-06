use crate::HOLD_TO_CONFIRM_TIME_SHORT_MS;
use crate::{
    palette::PALETTE, prelude::*, HoldToConfirm, Padding, ProgressIndicator, FONT_MED, FONT_SMALL,
};
use alloc::{boxed::Box, format};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565, text::Alignment};
use u8g2_fonts::{fonts, U8g2TextStyle};

// Use small font (17px) for the hash
const HASH_FONT: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;

/// Hold to confirm widget for firmware upgrades
/// Displays the firmware hash and size
#[derive(frostsnap_macros::Widget)]
pub struct FirmwareUpgradeConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<
        Column<(
            Text<U8g2TextStyle<Rgb565>>,
            Container<
                Padding<
                    Column<(
                        Text<U8g2TextStyle<Rgb565>>,
                        Text<U8g2TextStyle<Rgb565>>,
                        Text<U8g2TextStyle<Rgb565>>,
                        Text<U8g2TextStyle<Rgb565>>,
                    )>,
                >,
            >,
            Text<U8g2TextStyle<Rgb565>>,
        )>,
    >,
}

impl FirmwareUpgradeConfirm {
    pub fn new(firmware_digest: [u8; 32], size_bytes: u32) -> Self {
        // Format the full hash as 4 lines of 16 hex chars each
        let hash_line1 = format!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[0],
            firmware_digest[1],
            firmware_digest[2],
            firmware_digest[3],
            firmware_digest[4],
            firmware_digest[5],
            firmware_digest[6],
            firmware_digest[7]
        );
        let hash_line2 = format!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[8],
            firmware_digest[9],
            firmware_digest[10],
            firmware_digest[11],
            firmware_digest[12],
            firmware_digest[13],
            firmware_digest[14],
            firmware_digest[15]
        );
        let hash_line3 = format!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[16],
            firmware_digest[17],
            firmware_digest[18],
            firmware_digest[19],
            firmware_digest[20],
            firmware_digest[21],
            firmware_digest[22],
            firmware_digest[23]
        );
        let hash_line4 = format!(
            "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[24],
            firmware_digest[25],
            firmware_digest[26],
            firmware_digest[27],
            firmware_digest[28],
            firmware_digest[29],
            firmware_digest[30],
            firmware_digest[31]
        );

        // Format size in KB or MB
        let size_text = if size_bytes < 1024 * 1024 {
            format!("{} KB", size_bytes / 1024)
        } else {
            format!("{:.1} MB", size_bytes as f32 / (1024.0 * 1024.0))
        };

        // Create the content with title, hash lines, and size
        let title = Text::new(
            "Upgrade firmware?",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let hash1 = Text::new(
            hash_line1,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let hash2 = Text::new(
            hash_line2,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let hash3 = Text::new(
            hash_line3,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let hash4 = Text::new(
            hash_line4,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let size = Text::new(
            size_text,
            U8g2TextStyle::new(FONT_SMALL, PALETTE.on_surface_variant),
        )
        .with_alignment(Alignment::Center);

        // Put just the hash lines in a container with rounded border, fill, and padding
        let hash_column = Column::new((hash1, hash2, hash3, hash4));
        let hash_with_padding = Padding::all(5, hash_column);
        let hash_container = Container::new(hash_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(10, 10));

        // Create main column with title, container, and size
        let content = Column::builder()
            .push(title)
            .gap(8)
            .push(hash_container)
            .gap(8)
            .push(size);

        // Create hold to confirm with 1 second hold time
        let hold_to_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_SHORT_MS, content);

        Self { hold_to_confirm }
    }

    /// Check if the confirmation is complete
    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_completed()
    }
}

/// Widget for showing firmware upgrade progress
#[derive(frostsnap_macros::Widget)]
pub enum FirmwareUpgradeProgress {
    /// Actively erasing or downloading with progress
    Active {
        widget: Box<Column<(Text<U8g2TextStyle<Rgb565>>, Padding<ProgressIndicator>)>>,
    },
    /// Passive state - just show text
    Passive {
        widget: Center<Text<U8g2TextStyle<Rgb565>>>,
    },
}

impl FirmwareUpgradeProgress {
    /// Create a new firmware upgrade progress widget in erasing state
    pub fn erasing(progress: f32) -> Self {
        let title = Text::new(
            "Preparing for\nupgrade...",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        // Add horizontal padding around the progress indicator
        let padded_progress = Padding::symmetric(20, 0, progress_indicator);

        let widget = Column::new((title, padded_progress))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        Self::Active {
            widget: Box::new(widget),
        }
    }

    /// Create a new firmware upgrade progress widget in downloading state
    pub fn downloading(progress: f32) -> Self {
        let title = Text::new(
            "Downloading\nupgrade...",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        // Add horizontal padding around the progress indicator
        let padded_progress = Padding::symmetric(20, 0, progress_indicator);

        let widget = Column::new((title, padded_progress))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        Self::Active {
            widget: Box::new(widget),
        }
    }

    /// Create a new firmware upgrade progress widget in passive state
    pub fn passive() -> Self {
        // Show "Firmware Upgrade" text in passive state
        let text = Text::new(
            "Firmware\nUpgrade",
            U8g2TextStyle::new(FONT_MED, PALETTE.primary),
        )
        .with_alignment(Alignment::Center);
        let widget = Center::new(text);

        Self::Passive { widget }
    }

    /// Update the progress for active states
    pub fn update_progress(&mut self, progress: f32) {
        if let Self::Active { widget } = self {
            // Update the progress indicator through the padding wrapper
            widget
                .children
                .1
                .child
                .set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));
        }
    }
}
