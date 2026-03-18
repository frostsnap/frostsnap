use crate::HOLD_TO_CONFIRM_TIME_SHORT_MS;
use crate::{
    gray4_style::Gray4TextStyle, palette::PALETTE, prelude::*, HoldToConfirm, Padding,
    ProgressIndicator,
};
use alloc::{boxed::Box, format, string::String, string::ToString};
use embedded_graphics::geometry::Size;
use frostsnap_fonts::{NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_MONO_17_REGULAR};

#[derive(frostsnap_macros::Widget)]
pub struct FirmwareUpgradeConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<
        Column<(
            Text<Gray4TextStyle>,
            Container<
                Padding<
                    Column<(
                        Text<Gray4TextStyle>,
                        Text<Gray4TextStyle>,
                        Text<Gray4TextStyle>,
                        Text<Gray4TextStyle>,
                    )>,
                >,
            >,
            Text<Gray4TextStyle>,
            Text<Gray4TextStyle>,
        )>,
    >,
}

impl FirmwareUpgradeConfirm {
    pub fn new(firmware_digest: [u8; 32], size_bytes: u32) -> Self {
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

        let size_text = if size_bytes < 1024 * 1024 {
            format!("{} KB", size_bytes / 1024)
        } else {
            format!("{:.1} MB", size_bytes as f32 / (1024.0 * 1024.0))
        };

        let title_text = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let version_size_display = Text::new(
            size_text,
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let hold_text = Text::new(
            "Hold to upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let hash_column = Column::new((hash1, hash2, hash3, hash4))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let hash_with_padding = Padding::symmetric(10, 5, hash_column);
        let hash_container = Container::new(hash_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_corner_radius(Size::new(8, 8));

        let content = Column::new((title_text, hash_container, version_size_display, hold_text))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let hold_to_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_SHORT_MS, content);

        Self { hold_to_confirm }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_confirmed()
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
                        Text<Gray4TextStyle>,
                        Text<Gray4TextStyle>,
                        ProgressIndicator,
                    )>,
                >,
            >,
        >,
    },
    /// Passive state
    Passive {
        widget: Center<Text<Gray4TextStyle>>,
    },
}

impl FirmwareUpgradeProgress {
    fn new_active(status_text: &str, progress: f32) -> Self {
        let title = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let status = Text::new(
            status_text.to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        let mut column = Column::new((title, status, progress_indicator))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_gap(1, 15);

        let padded = Padding::symmetric(20, 20, column);

        Self::Active {
            widget: Box::new(Center::new(padded)),
        }
    }

    pub fn erasing(progress: f32) -> Self {
        Self::new_active("Preparing device", progress)
    }

    pub fn downloading(progress: f32) -> Self {
        Self::new_active("Receiving and verifying", progress)
    }

    pub fn passive() -> Self {
        let text = Text::new(
            "not upgrading".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.primary),
        );
        Self::Passive {
            widget: Center::new(text),
        }
    }

    pub fn update_progress(&mut self, progress: f32) {
        if let Self::Active { widget } = self {
            widget
                .child
                .child
                .children
                .2
                .set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));
        }
    }
}
