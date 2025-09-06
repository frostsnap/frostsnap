use crate::{
    bitmap::EncodedImage, device_name::DeviceName, icons::IconWidget, image::Image,
    palette::PALETTE, prelude::*, share_index::ShareIndexWidget, vec_framebuffer::VecFramebuffer,
};
use alloc::string::String;
use embedded_graphics::pixelcolor::{BinaryColor, Rgb565};
use embedded_iconoir::prelude::IconoirNewIcon;
use frostsnap_core::message::HeldShare;
use u8g2_fonts::U8g2TextStyle;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            Image<VecFramebuffer<BinaryColor>, Rgb565>,
            Option<
                Row<(
                    IconWidget<
                        embedded_iconoir::Icon<
                            Rgb565,
                            embedded_iconoir::icons::size24px::actions::WarningTriangle,
                        >,
                    >,
                    Text<U8g2TextStyle<Rgb565>>,
                )>,
            >,
            Row<(
                IconWidget<
                    embedded_iconoir::Icon<
                        Rgb565,
                        embedded_iconoir::icons::size24px::finance::Wallet,
                    >,
                >,
                Text<U8g2TextStyle<Rgb565>>,
            )>,
            ShareIndexWidget,
            DeviceName,
        )>,
    >,
}

impl Standby {
    pub fn new(device_name: impl Into<String>, held_share: HeldShare) -> Self {
        // Create text styles
        // Medium emphasis grey for key name (medium size)
        let key_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);

        // Create wallet icon and key name row
        let wallet_icon = IconWidget::new(embedded_iconoir::icons::size24px::finance::Wallet::new(
            PALETTE.on_surface_variant,
        ));
        let key_text = Text::new(held_share.key_name.clone(), key_style);

        let key_row = Row::builder().push(wallet_icon).gap(8).push(key_text);

        // Create recovery mode warning if access structure is missing
        let recovery_warning = if held_share.access_structure_ref.is_none() {
            let warning_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.warning);
            let warning_icon = IconWidget::new(
                embedded_iconoir::icons::size24px::actions::WarningTriangle::new(PALETTE.warning),
            );
            let warning_text = Text::new("recovery mode", warning_style);
            Some(Row::builder().push(warning_icon).gap(8).push(warning_text))
        } else {
            None
        };

        // Extract share index and create the widget with medium font
        let share_index: u16 = held_share.share_image.index.try_into().unwrap();
        let share_index_widget = ShareIndexWidget::new_medium(share_index);

        // Create DeviceName widget
        let device_name_widget = DeviceName::new(device_name);

        // Load logo
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo = Image::with_color_map(framebuffer, |color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create column with spacing
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
