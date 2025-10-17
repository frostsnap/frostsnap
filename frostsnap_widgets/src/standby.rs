use crate::DefaultTextStyle;
use crate::{device_name::DeviceName, palette::PALETTE, prelude::*, BmpImage};
use alloc::string::{String, ToString};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565, text::Alignment};
use frostsnap_core::message::HeldShare;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-icon-80x96.bmp");
const WARNING_ICON_DATA: &[u8] = include_bytes!("../assets/warning-icon-24x24.bmp");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            BmpImage,
            SizedBox<Rgb565>,
            Option<Row<(BmpImage, SizedBox<Rgb565>, Text)>>,
            SizedBox<Rgb565>,
            Text, // Key name
            SizedBox<Rgb565>,
            Text, // "Key #42" display
            SizedBox<Rgb565>,
            DeviceName,
        )>,
    >,
}

impl Standby {
    pub fn new(device_name: impl Into<String>, held_share: HeldShare) -> Self {
        // Load BMP logo with color mapping
        let logo = BmpImage::new(LOGO_DATA, PALETTE.logo);

        let recovery_warning = if held_share.access_structure_ref.is_none() {
            // Use the warning icon BMP
            let warning_icon = BmpImage::new(WARNING_ICON_DATA, PALETTE.warning);

            let icon_spacer = SizedBox::new(Size::new(4, 0)); // 4px horizontal spacing

            let warning_text = Text::new(
                "Recovery Mode",
                DefaultTextStyle::new(crate::FONT_MED, PALETTE.warning),
            );

            Some(
                Row::new((warning_icon, icon_spacer, warning_text))
                    .with_cross_axis_alignment(crate::CrossAxisAlignment::End),
            )
        } else {
            None
        };

        // Create key name in medium emphasis grey
        let key_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);
        let key_text =
            Text::new(held_share.key_name.to_string(), key_style).with_alignment(Alignment::Center);

        // Extract share index and create the "Key #42" display
        let share_index: u16 = held_share.share_image.index.try_into().unwrap();
        let key_index_text = Text::new(
            format!("Key #{}", share_index),
            DefaultTextStyle::new(crate::FONT_SMALL, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let device_name_widget = DeviceName::new(device_name);

        // Create spacers with fixed heights
        let spacer1 = SizedBox::new(Size::new(0, 15)); // Space after logo
        let spacer2 = SizedBox::new(Size::new(0, 8)); // Space after recovery warning (or before key name if no warning)
        let spacer3 = SizedBox::new(Size::new(0, 12)); // Space after key name
        let spacer4 = SizedBox::new(Size::new(0, 4)); // Small space between share index and device name

        // Create column with fixed spacing
        let column = Column::new((
            logo,
            spacer1,
            recovery_warning,
            spacer2,
            key_text,
            spacer3,
            key_index_text,
            spacer4,
            device_name_widget,
        ))
        .with_cross_axis_alignment(crate::CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }
}
