use crate::HOLD_TO_CONFIRM_TIME_SHORT_MS;
use crate::{
    gray4_style::Gray4TextStyle, palette::PALETTE, prelude::*, HoldToConfirm, Padding,
    ProgressIndicator,
};

use alloc::{boxed::Box, format, string::String, string::ToString};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565};
use frostsnap_fonts::{NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_MONO_17_REGULAR};

/// Hold to confirm widget for firmware upgrades
/// Displays the firmware hash and size
#[derive(frostsnap_macros::Widget)]
pub struct FirmwareUpgradeConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<
        Column<(
            Text<Gray4TextStyle>, // Title
            Container<
                Padding<
                    Column<(
                        Text<Gray4TextStyle>, // Hash line 1
                        Text<Gray4TextStyle>, // Hash line 2
                        Text<Gray4TextStyle>, // Hash line 3
                        Text<Gray4TextStyle>, // Hash line 4
                    )>,
                >,
            >,
            Text<Gray4TextStyle>, // Size text
            Text<Gray4TextStyle>, // "Hold to Upgrade" text
        )>,
    >,
}

impl FirmwareUpgradeConfirm {
    pub fn new(firmware_digest: [u8; 32], size_bytes: u32) -> Self {
        // Format the full hash as 4 lines of 16 hex chars each
        let mut chunks = firmware_digest.chunks(8);
        let hash1 = Text::new(
            chunks
                .next()
                .unwrap()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>(),
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );
        let hash2 = Text::new(
            chunks
                .next()
                .unwrap()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>(),
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );
        let hash3 = Text::new(
            chunks
                .next()
                .unwrap()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>(),
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );
        let hash4 = Text::new(
            chunks
                .next()
                .unwrap()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>(),
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );

        // Format size in KB or MB
        let size_text = if size_bytes < 1024 * 1024 {
            format!("{} KB", size_bytes / 1024)
        } else {
            format!("{:.1} MB", size_bytes as f32 / (1024.0 * 1024.0))
        };

        // Create the title text
        let title_text = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        // Create size text
        let version_size_display = Text::new(
            size_text,
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        // Create "Hold to upgrade" text at the bottom
        let hold_text = Text::new(
            "Hold to upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        // Put hash lines in a container with rounded border (no fill)
        let hash_column = Column::new((hash1, hash2, hash3, hash4))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let hash_with_padding = Padding::symmetric(10, 5, hash_column);
        let hash_container = Container::new(hash_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_corner_radius(Size::new(8, 8))
            .with_aa_background(PALETTE.background);

        // Create main column with SpaceEvenly alignment
        let content = Column::new((title_text, hash_container, version_size_display, hold_text))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

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
        widget: Box<
            Center<
                Padding<
                    Column<(
                        Text<Gray4TextStyle>, // Title
                        Text<Gray4TextStyle>, // Status
                        SizedBox<Rgb565>,     // Spacer
                        ProgressIndicator,    // Progress bar
                    )>,
                >,
            >,
        >,
    },
    /// Passive state - just show text
    Passive {
        widget: Center<Text<Gray4TextStyle>>,
    },
}

impl FirmwareUpgradeProgress {
    /// Create a new firmware upgrade progress widget in erasing state
    pub fn erasing(progress: f32) -> Self {
        let title = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let status = Text::new(
            "Preparing device".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 15));

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        let column = Column::new((title, status, spacer, progress_indicator))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let padded = Padding::symmetric(20, 20, column);
        let centered = Center::new(padded);

        Self::Active {
            widget: alloc::boxed::Box::new(centered),
        }
    }

    pub fn downloading(progress: f32) -> Self {
        let title = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let status = Text::new(
            "Receiving and verifying".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 15));

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        let column = Column::new((title, status, spacer, progress_indicator))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let padded = Padding::symmetric(20, 20, column);
        let centered = Center::new(padded);

        Self::Active {
            widget: Box::new(centered),
        }
    }

    /// Create a new firmware upgrade progress widget in passive state
    pub fn passive() -> Self {
        let text = Text::new(
            "Firmware Upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.primary),
        );
        let widget = Center::new(text);

        Self::Passive { widget }
    }

    /// Update the progress for active states
    pub fn update_progress(&mut self, progress: f32) {
        if let Self::Active { widget } = self {
            // Update the progress indicator through the center -> padding -> column -> progress indicator path
            widget
                .child
                .child
                .children
                .3
                .set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));
        }
    }
}
