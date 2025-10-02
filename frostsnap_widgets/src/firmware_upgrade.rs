use crate::HOLD_TO_CONFIRM_TIME_SHORT_MS;
use crate::{
    fonts::{Gray4TextStyle, NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_MONO_17_REGULAR},
    palette::PALETTE,
    prelude::*,
    HoldToConfirm, Padding, ProgressIndicator,
};
use alloc::{format, string::ToString};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565};

/// Hold to confirm widget for firmware upgrades
/// Displays the firmware hash and size
#[derive(frostsnap_macros::Widget)]
pub struct FirmwareUpgradeConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<
        Column<(
            Text<Gray4TextStyle<'static>>, // Title
            Container<
                Padding<
                    Column<(
                        Text<Gray4TextStyle<'static>>, // Hash line 1
                        Text<Gray4TextStyle<'static>>, // Hash line 2
                        Text<Gray4TextStyle<'static>>, // Hash line 3
                        Text<Gray4TextStyle<'static>>, // Hash line 4
                    )>,
                >,
            >,
            Text<Gray4TextStyle<'static>>, // Size text
            Text<Gray4TextStyle<'static>>, // "Hold to Update" text
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

        // Create the title text
        let title_text = Text::new(
            "Update device firmware".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        // Create hash display with monospace font
        let hash1 = Text::new(
            hash_line1,
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );

        let hash2 = Text::new(
            hash_line2,
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );

        let hash3 = Text::new(
            hash_line3,
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );

        let hash4 = Text::new(
            hash_line4,
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        );

        // Create size text
        let version_size_display = Text::new(
            size_text,
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        // Create "Hold to Update" text at the bottom
        let hold_text = Text::new(
            "Hold to Update".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        // Put hash lines in a container with rounded border (no fill)
        let hash_column = Column::new((hash1, hash2, hash3, hash4))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let hash_with_padding = Padding::symmetric(10, 5, hash_column);
        let hash_container = Container::new(hash_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_corner_radius(Size::new(8, 8));

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
        widget: Center<
            Padding<
                Column<(
                    Text<Gray4TextStyle<'static>>, // Title
                    Text<Gray4TextStyle<'static>>, // Status
                    SizedBox<Rgb565>,              // Spacer
                    ProgressIndicator,             // Progress bar
                )>,
            >,
        >,
    },
    /// Passive state - just show text
    Passive {
        widget: Center<Text<Gray4TextStyle<'static>>>,
    },
}

impl FirmwareUpgradeProgress {
    /// Create a new firmware upgrade progress widget in erasing state
    pub fn erasing(progress: f32) -> Self {
        let title = Text::new(
            "Firmware Update".to_string(),
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

        Self::Active { widget: centered }
    }

    /// Create a new firmware upgrade progress widget in downloading state
    pub fn downloading(progress: f32) -> Self {
        let title = Text::new(
            "Firmware Update".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let status = Text::new(
            "Downloading".to_string(),
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

        Self::Active { widget: centered }
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
