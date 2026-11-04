use crate::HOLD_TO_CONFIRM_TIME_SHORT_MS;
use crate::{
    any_of::AnyOf, gray4_style::Gray4TextStyle, palette::PALETTE, prelude::*,
    super_draw_target::SuperDrawTarget, DynWidget, FadeConfig, FadeSwitcher, Frac, GrayToAlpha,
    HoldToConfirm, IconWidget, Image, Instant, KeyTouch, Padding, ProgressIndicator, Sizing,
};
use alloc::{boxed::Box, format, string::String, string::ToString};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::pixelcolor::{Gray8, Rgb565};
use embedded_graphics::primitives::Rectangle;
use embedded_iconoir::prelude::*;
use embedded_iconoir::{size24px::identity::Fingerprint, size48px::navigation::ArrowUpCircle};
use frostsnap_fonts::{
    Gray4Font, NOTO_SANS_14_LIGHT, NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_24_BOLD,
    NOTO_SANS_MONO_14_REGULAR, NOTO_SANS_MONO_17_REGULAR,
};
use tinybmp::Bmp;

const WARNING_ICON_DATA: &[u8] = include_bytes!("../assets/warning-icon-24x24.bmp");

const BOX_SIZE: Size = Size::new(230, 146);
const DIGEST_PANEL_SIZE: Size = Size::new(184, 90);
const FINGERPRINT_BYTES: usize = 4;

/// The firmware an upgrade installs, as the confirm screen names it.
#[derive(Clone)]
pub struct UpgradeTarget {
    pub version: String,
    pub dev: bool,
}

type Label = Text<Gray4TextStyle>;
type Icon<I> = IconWidget<embedded_iconoir::Icon<Rgb565, I>>;
type Chip = Container<Padding<Row<(Icon<Fingerprint>, Label)>>>;

type TargetContent = Center<Column<(Icon<ArrowUpCircle>, Column<(Label, Option<Label>)>, Chip)>>;

type DigestContent = Center<Container<Center<Column<(Row<(Label, Label)>, Label, Label, Label)>>>>;

fn label(text: impl Into<String>, font: &'static Gray4Font, color: Rgb565) -> Label {
    Text::new(text.into(), Gray4TextStyle::new(font, color))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn target_content(target: &UpgradeTarget, firmware_digest: &[u8; 32]) -> TargetContent {
    let chip = Container::new(Padding::symmetric(
        8,
        3,
        Row::builder()
            .push(IconWidget::new(Fingerprint::new(PALETTE.text_secondary)))
            .gap(6)
            .push(label(
                hex(&firmware_digest[..FINGERPRINT_BYTES]),
                &NOTO_SANS_MONO_14_REGULAR,
                PALETTE.text_secondary,
            ))
            .with_cross_axis_alignment(CrossAxisAlignment::Center),
    ))
    .with_border(PALETTE.surface_variant, 1)
    .with_corner_radius(Size::new(11, 11));
    Center::new(
        Column::new((
            IconWidget::new(ArrowUpCircle::new(PALETTE.primary)),
            Column::new((
                label(
                    target.version.clone(),
                    &NOTO_SANS_24_BOLD,
                    PALETTE.on_background,
                ),
                target
                    .dev
                    .then(|| label("development build", &NOTO_SANS_14_LIGHT, PALETTE.warning)),
            ))
            .with_cross_axis_alignment(CrossAxisAlignment::Center),
            chip,
        ))
        .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly)
        .with_cross_axis_alignment(CrossAxisAlignment::Center),
    )
}

fn digest_content(firmware_digest: &[u8; 32]) -> DigestContent {
    let row = |i: usize| {
        label(
            hex(&firmware_digest[i * 8..(i + 1) * 8]),
            &NOTO_SANS_MONO_17_REGULAR,
            PALETTE.on_surface,
        )
    };
    let first = Row::new((
        label(
            hex(&firmware_digest[..FINGERPRINT_BYTES]),
            &NOTO_SANS_MONO_17_REGULAR,
            PALETTE.primary,
        ),
        label(
            hex(&firmware_digest[FINGERPRINT_BYTES..8]),
            &NOTO_SANS_MONO_17_REGULAR,
            PALETTE.on_surface,
        ),
    ));
    Center::new(
        Container::with_size(
            Center::new(
                Column::new((first, row(1), row(2), row(3)))
                    .with_cross_axis_alignment(CrossAxisAlignment::Center),
            ),
            DIGEST_PANEL_SIZE,
        )
        .with_border(PALETTE.outline, 2)
        .with_corner_radius(Size::new(8, 8)),
    )
}

/// The box naming what's being installed. It has a fixed size so that switching what it shows
/// can't move the rest of the screen.
pub struct DigestBox {
    container: Container<FadeSwitcher<AnyOf<(TargetContent, DigestContent)>>>,
    firmware_digest: [u8; 32],
    target: Option<UpgradeTarget>,
    showing_digest: bool,
    pressed: bool,
}

impl DigestBox {
    fn new(firmware_digest: [u8; 32], target: Option<UpgradeTarget>) -> Self {
        let content = match &target {
            Some(target) => AnyOf::new(target_content(target, &firmware_digest)),
            None => AnyOf::new(digest_content(&firmware_digest)),
        };
        let container = Container::with_size(
            FadeSwitcher::new(content).with_fade_config(FadeConfig::new(200)),
            BOX_SIZE,
        );
        Self {
            container,
            firmware_digest,
            showing_digest: target.is_none(),
            target,
            pressed: false,
        }
    }

    fn toggle(&mut self) {
        let Some(target) = &self.target else {
            return;
        };
        self.showing_digest = !self.showing_digest;
        let content = if self.showing_digest {
            AnyOf::new(digest_content(&self.firmware_digest))
        } else {
            AnyOf::new(target_content(target, &self.firmware_digest))
        };
        self.container.child.switch_to(content);
    }
}

impl DynWidget for DigestBox {
    fn set_constraints(&mut self, max_size: Size) {
        self.container.set_constraints(max_size);
    }

    fn sizing(&self) -> Sizing {
        self.container.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        let inside = Rectangle::new(Point::zero(), BOX_SIZE).contains(point);
        // A column sends a release to every child, wherever it lands, so a finger lifted off the
        // hold button also arrives here.
        if !is_release {
            self.pressed = inside;
        } else if core::mem::take(&mut self.pressed) && inside {
            self.toggle();
        }
        None
    }

    fn force_full_redraw(&mut self) {
        self.container.force_full_redraw();
    }
}

impl Widget for DigestBox {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.container.draw(target, current_time)
    }
}

#[derive(frostsnap_macros::Widget)]
pub struct FirmwareUpgradeConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<Column<(DigestBox, Label)>>,
}

impl FirmwareUpgradeConfirm {
    pub fn new(firmware_digest: [u8; 32], target: Option<UpgradeTarget>) -> Self {
        let content = Column::new((
            DigestBox::new(firmware_digest, target),
            label(
                "Hold to upgrade",
                &NOTO_SANS_14_LIGHT,
                PALETTE.text_secondary,
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self {
            hold_to_confirm: HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_SHORT_MS, content),
        }
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
    /// Passive state - title and status, no progress bar
    Passive {
        widget: Box<Center<Padding<Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>>>>,
    },
    /// Upgrade was refused - warning icon, title, reason, recovery instruction
    Rejected {
        widget: Box<
            Center<
                Padding<
                    Column<(
                        Image<GrayToAlpha<Bmp<'static, Gray8>, Rgb565>>,
                        Text<Gray4TextStyle>,
                        Text<Gray4TextStyle>,
                        Text<Gray4TextStyle>,
                    )>,
                >,
            >,
        >,
    },
}

impl FirmwareUpgradeProgress {
    fn new_active(status_text: &str, progress: Frac) -> Self {
        let title = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let status = Text::new(
            status_text.to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let mut progress_indicator = ProgressIndicator::new();
        progress_indicator.set_progress(progress);

        let mut column = Column::new((title, status, progress_indicator))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_gap(1, 15);

        let padded = Padding::symmetric(20, 20, column);

        Self::Active {
            widget: Box::new(Center::new(padded)),
        }
    }

    pub fn erasing(progress: Frac) -> Self {
        Self::new_active("Preparing device", progress)
    }

    pub fn downloading(progress: Frac) -> Self {
        Self::new_active("Receiving and verifying", progress)
    }

    pub fn passive() -> Self {
        let title = Text::new(
            "Firmware upgrade".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.on_background),
        );

        let status = Text::new(
            "Waiting".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let column = Column::new((title, status))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let padded = Padding::symmetric(20, 20, column);
        let widget = Center::new(padded);

        Self::Passive {
            widget: Box::new(widget),
        }
    }

    /// A refusal must read as a failure, not a neutral status: warning icon and
    /// red title, with the reason in primary text so it carries the information.
    /// The device parks on this screen, so it also says how to recover.
    pub fn rejected(status: &str) -> Self {
        let warning_bmp =
            Bmp::<Gray8>::from_slice(WARNING_ICON_DATA).expect("Failed to load warning icon BMP");
        let icon = Image::new(GrayToAlpha::new(warning_bmp, PALETTE.error));

        let title = Text::new(
            "Upgrade failed".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.error),
        );

        let status = Text::new(
            status.to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.on_background),
        );

        let instruction = Text::new(
            "Unplug to try again".to_string(),
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        );

        let column = Column::builder()
            .push(icon)
            .gap(12)
            .push(title)
            .gap(6)
            .push(status)
            .gap(16)
            .push(instruction)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let padded = Padding::symmetric(20, 20, column);
        let widget = Center::new(padded);

        Self::Rejected {
            widget: Box::new(widget),
        }
    }

    /// Update the progress for active states
    pub fn update_progress(&mut self, progress: Frac) {
        if let Self::Active { widget } = self {
            widget.child.child.children.2.set_progress(progress);
        }
    }
}
