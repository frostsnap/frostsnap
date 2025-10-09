use crate::DefaultTextStyle;
use crate::{
    any_of::AnyOf, device_name::DeviceName, palette::PALETTE, prelude::*, FadeSwitcher, GrayToAlpha,
};
use alloc::string::{String, ToString};
use embedded_graphics::{
    pixelcolor::{Gray8, Rgb565},
    text::Alignment,
};
use frostsnap_core::message::HeldShare2;
use tinybmp::Bmp;

pub const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-icon-80x96.bmp");
const WARNING_ICON_DATA: &[u8] = include_bytes!("../assets/warning-icon-24x24.bmp");

type Image = crate::Image<GrayToAlpha<Bmp<'static, Gray8>, Rgb565>>;

/// Blank standby content - shows welcome message
#[derive(frostsnap_macros::Widget)]
pub struct StandbyBlank {
    #[widget_delegate]
    content: Center<Column<(Text, Text, Text)>>,
}

impl Default for StandbyBlank {
    fn default() -> Self {
        Self::new()
    }
}

impl StandbyBlank {
    pub fn new() -> Self {
        let text_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_background);
        let url_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.primary);

        let text_line1 = Text::new("Get started with", text_style.clone());
        let text_line2 = Text::new("Frostsnap", text_style);
        let url_text = Text::new("frostsnap.com/start", url_style);

        let column = Column::builder()
            .push(text_line1)
            .gap(4)
            .push(text_line2)
            .gap(16)
            .push(url_text)
            .with_cross_axis_alignment(crate::CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }
}

/// Standby content showing key information
#[derive(frostsnap_macros::Widget)]
pub struct StandbyHasKey {
    #[widget_delegate]
    content: Column<(Option<Row<(Image, Text)>>, Text, Text, DeviceName)>,
}

impl StandbyHasKey {
    pub fn new(device_name: impl Into<String>, held_share: HeldShare2) -> Self {
        let recovery_warning = if held_share.access_structure_ref.is_none() {
            let warning_bmp =
                Bmp::<Gray8>::from_slice(WARNING_ICON_DATA).expect("Failed to load warning BMP");
            let warning_icon = Image::new(GrayToAlpha::new(warning_bmp, PALETTE.warning));

            let warning_text = Text::new(
                "Recovery Mode",
                DefaultTextStyle::new(crate::FONT_MED, PALETTE.warning),
            );

            Some(
                Row::builder()
                    .push(warning_icon)
                    .gap(4)
                    .push(warning_text)
                    .with_cross_axis_alignment(crate::CrossAxisAlignment::End),
            )
        } else {
            None
        };

        let key_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);
        let key_text = Text::new(held_share.key_name.unwrap_or("??".to_string()), key_style)
            .with_alignment(Alignment::Center);

        let share_index: u16 = held_share.share_image.index.try_into().unwrap();
        let key_index_text = Text::new(
            format!("Key #{}", share_index),
            DefaultTextStyle::new(crate::FONT_SMALL, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let device_name_widget = DeviceName::new(device_name);

        let content = Column::builder()
            .push(recovery_warning)
            .gap(8)
            .push(key_text)
            .gap(12)
            .push(key_index_text)
            .gap(4)
            .push(device_name_widget)
            .with_cross_axis_alignment(crate::CrossAxisAlignment::Center);

        Self { content }
    }
}

/// Main standby widget that can show startup (empty), blank (welcome), or has-key content
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            Padding<Image>,
            FadeSwitcher<Option<AnyOf<(StandbyBlank, StandbyHasKey)>>>,
        )>,
    >,
}

impl Default for Standby {
    fn default() -> Self {
        Self::new()
    }
}

impl Standby {
    /// Create a new Standby widget in startup mode (just logo, empty body)
    pub fn new() -> Self {
        let logo_bmp = Bmp::<Gray8>::from_slice(LOGO_DATA).expect("Failed to load BMP");
        let logo = Image::new(GrayToAlpha::new(logo_bmp, PALETTE.logo));
        let padded_logo = Padding::only(logo).top(30).bottom(20).build();

        let fade_switcher = FadeSwitcher::new(None, 500);

        let column = Column::builder()
            .push(padded_logo)
            .push(fade_switcher)
            .with_cross_axis_alignment(crate::CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }

    /// Clear content (back to startup mode - just logo)
    pub fn clear_content(&mut self) {
        self.content.child.children.1.switch_to(None);
    }

    /// Set to welcome mode (blank with welcome message)
    pub fn set_welcome(&mut self) {
        let blank_content = StandbyBlank::new();
        self.content
            .child
            .children
            .1
            .switch_to(Some(AnyOf::new(blank_content)));
    }

    /// Set to has-key mode with key information
    pub fn set_key(&mut self, device_name: impl Into<String>, held_share: HeldShare2) {
        let has_key_content = StandbyHasKey::new(device_name, held_share);
        self.content
            .child
            .children
            .1
            .switch_to(Some(AnyOf::new(has_key_content)));
    }
}
