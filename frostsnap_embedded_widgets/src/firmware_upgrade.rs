use crate::super_draw_target::SuperDrawTarget;
use crate::MainAxisAlignment;
use crate::{
    palette::PALETTE, Center, Column, Container, Expanded, Fader, HoldToConfirm, Padding,
    ProgressIndicator, SizedBox, Text, FONT_MED, FONT_SMALL, Instant, Widget, DynWidget, Sizing,
};
use alloc::{boxed::Box, format, string::{String, ToString}};
use embedded_graphics::{geometry::{Size, Point}, pixelcolor::Rgb565, text::Alignment, prelude::*};
use u8g2_fonts::{fonts, U8g2TextStyle};

// Use small font (17px) for the hash
const HASH_FONT: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;

/// Hold to confirm widget for firmware upgrades
/// Displays the firmware hash and size
#[derive(frostsnap_macros::Widget)]
pub struct FirmwareUpgradeConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<Center<Padding<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        Container<Padding<Column<(
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>
        )>>>,
        Text<U8g2TextStyle<Rgb565>>
    )>>>>,
}

impl FirmwareUpgradeConfirm {
    pub fn new(firmware_digest: [u8; 32], version: &str, size_bytes: u32) -> Self {
        // Format the full hash as 4 lines of 16 hex chars each
        let hash_line1 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[0], firmware_digest[1], firmware_digest[2], firmware_digest[3],
            firmware_digest[4], firmware_digest[5], firmware_digest[6], firmware_digest[7]
        );
        let hash_line2 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[8], firmware_digest[9], firmware_digest[10], firmware_digest[11],
            firmware_digest[12], firmware_digest[13], firmware_digest[14], firmware_digest[15]
        );
        let hash_line3 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[16], firmware_digest[17], firmware_digest[18], firmware_digest[19],
            firmware_digest[20], firmware_digest[21], firmware_digest[22], firmware_digest[23]
        );
        let hash_line4 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[24], firmware_digest[25], firmware_digest[26], firmware_digest[27],
            firmware_digest[28], firmware_digest[29], firmware_digest[30], firmware_digest[31]
        );

        // Format version and size together
        let version_size_text = if size_bytes < 1024 * 1024 {
            format!("{} ({} KB)", version, size_bytes / 1024)
        } else {
            format!("{} ({:.1} MB)", version, size_bytes as f32 / (1024.0 * 1024.0))
        };

        // Create the content with title, hash lines, and size
        let title = Text::new(
            "Upgrade firmware?",
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
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

        let version_size = Text::new(
            version_size_text,
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        // Put just the hash lines in a container with rounded border, fill, and padding
        let hash_column = Column::new((hash1, hash2, hash3, hash4));
        let hash_with_padding = Padding::all(5, hash_column);
        let hash_container = Container::new(hash_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(10, 10));

        // Create main column with title, container, and version+size
        let content = Column::builder()
            .push(title)
            .push_with_gap(hash_container, 8)
            .push_with_gap(version_size, 8)
            .with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Add padding - add 5px top padding to move content down, keep 40px bottom for consistency
        let padded_content = Padding::only(content).top(5).bottom(40).build();

        // Center the content like in address pages
        let centered_content = Center::new(padded_content);

        // Create hold to confirm with 1 second hold time
        let hold_to_confirm = HoldToConfirm::new(1000, centered_content);

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
        widget: Fader<Center<Padding<Column<(Text<U8g2TextStyle<Rgb565>>, SizedBox<Rgb565>, ProgressIndicator)>>>>,
    },
    /// Passive state - just show text
    Passive {
        widget: Center<Text<U8g2TextStyle<Rgb565>>>,
    },
    /// Animated waiting state with cycling dots
    AnimatedWaiting {
        widget: Fader<AnimatedWaitingWidget>,
    },
}

/// Widget that shows static waiting text
pub struct AnimatedWaitingWidget {
    center: Center<Text<U8g2TextStyle<Rgb565>>>,
}

impl AnimatedWaitingWidget {
    pub fn new() -> Self {
        Self::with_text("Waiting for\ncoordinator")
    }

    pub fn with_text(text_content: &str) -> Self {
        let text = Text::new(
            text_content.to_string(),
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        Self {
            center: Center::new(text),
        }
    }
}

impl DynWidget for AnimatedWaitingWidget {
    fn set_constraints(&mut self, max_size: Size) {
        self.center.set_constraints(max_size);
    }

    fn sizing(&self) -> Sizing {
        self.center.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.center.force_full_redraw();
    }
}

impl Widget for AnimatedWaitingWidget {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Simply draw the static centered text
        self.center.draw(target, current_time)
    }
}

impl FirmwareUpgradeProgress {
    /// Create a new firmware upgrade progress widget in erasing state
    pub fn erasing(progress: f32) -> Self {
        let title = Text::new(
            "Preparing for\nupgrade",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 20)); // 20px vertical spacing

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        let column = Column::new((title, spacer, progress_indicator));
        let padded = Padding::symmetric(0, 20, column); // Add horizontal padding
        let centered = Center::new(padded);

        // Create a fader that starts faded out and immediately starts fading in
        let mut widget = Fader::new_faded_out(centered);
        widget.start_fade_in(800, 50, PALETTE.background); // 800ms fade in, 50ms redraw interval

        Self::Active { widget }
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
            // Update the progress indicator (now wrapped in Fader, at index 2 after spacer)
            widget.child.child.child.children.2.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));
        }
    }

    /// Create a new firmware upgrade progress widget in downloading state
    pub fn downloading(progress: f32) -> Self {
        let title = Text::new(
            "Downloading\nfirmware",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 20)); // 20px vertical spacing

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(crate::Frac::from_ratio((progress * 100.0) as u32, 100));

        let column = Column::new((title, spacer, progress_indicator));
        let padded = Padding::symmetric(0, 20, column); // Add horizontal padding
        let centered = Center::new(padded);

        // Create a fader that starts faded out and immediately starts fading in
        let mut widget = Fader::new_faded_out(centered);
        widget.start_fade_in(800, 50, PALETTE.background); // 800ms fade in, 50ms redraw interval

        Self::Active { widget }
    }

    /// Create a new firmware upgrade progress widget in waiting state
    pub fn waiting() -> Self {
        let animated = AnimatedWaitingWidget::new();

        // Create a fader that starts faded out and immediately starts fading in
        let mut widget = Fader::new_faded_out(animated);
        widget.start_fade_in(800, 50, PALETTE.background); // 800ms fade in, 50ms redraw interval

        Self::AnimatedWaiting { widget }
    }

    /// Create a new firmware upgrade progress widget in rebooting state
    pub fn rebooting() -> Self {
        let animated = AnimatedWaitingWidget::with_text("Restarting\ndevice");

        // Create a fader that starts faded out and immediately starts fading in
        let mut widget = Fader::new_faded_out(animated);
        widget.start_fade_in(800, 50, PALETTE.background); // 800ms fade in, 50ms redraw interval

        Self::AnimatedWaiting { widget }
    }
}

