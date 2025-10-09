use crate::{
    device_name::DeviceName, icons::IconWidget, palette::PALETTE, prelude::*,
    share_index::ShareIndexWidget, BmpImage,
};
use crate::{DefaultTextStyle, FONT_LARGE};
use alloc::string::{String, ToString};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_iconoir::prelude::IconoirNewIcon;
use frostsnap_core::message::HeldShare;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-icon-80x96.bmp");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            BmpImage,
            Option<
                Row<(
                    IconWidget<
                        embedded_iconoir::Icon<
                            Rgb565,
                            embedded_iconoir::icons::size24px::actions::WarningTriangle,
                        >,
                    >,
                    Text,
                )>,
            >,
            Row<(
                IconWidget<
                    embedded_iconoir::Icon<
                        Rgb565,
                        embedded_iconoir::icons::size24px::finance::Wallet,
                    >,
                >,
                Text,
            )>,
            ShareIndexWidget,
            DeviceName,
        )>,
    >,
}

impl Standby {
    pub fn new(device_name: impl Into<String>, held_share: HeldShare) -> Self {
        let key_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);

        let wallet_icon = IconWidget::new(embedded_iconoir::icons::size24px::finance::Wallet::new(
            PALETTE.on_surface_variant,
        ));
        let key_text = Text::new(held_share.key_name.to_string(), key_style);

        let key_row = Row::builder().push(wallet_icon).gap(8).push(key_text);

        let recovery_warning = if held_share.access_structure_ref.is_none() {
            let warning_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.warning);
            let warning_icon = IconWidget::new(
                embedded_iconoir::icons::size24px::actions::WarningTriangle::new(PALETTE.warning),
            );
            let warning_text = Text::new("recovery mode", warning_style);
            Some(Row::builder().push(warning_icon).gap(8).push(warning_text))
        } else {
            None
        };

        let share_index: u16 = held_share.share_image.index.try_into().unwrap();
        let share_index_widget = ShareIndexWidget::new(share_index, FONT_LARGE);

        let device_name_widget = DeviceName::new(device_name);

        // Load BMP logo with color mapping
        let logo = BmpImage::new(LOGO_DATA, PALETTE.logo);

        let column = Column::new((
            logo,
            recovery_warning,
            key_row,
            share_index_widget,
            device_name_widget,
        ))
        .with_main_axis_alignment(crate::MainAxisAlignment::SpaceEvenly);

        let content = Center::new(column);

        Self { content }
    }
}
