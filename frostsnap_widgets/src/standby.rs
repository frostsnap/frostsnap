use crate::DefaultTextStyle;
use crate::{
    device_name::DeviceName, gray4_style::Gray4TextStyle, palette::PALETTE, prelude::*,
    share_index::ShareIndexWidget, BmpImage,
};
use alloc::string::{String, ToString};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565, text::Alignment};
use frostsnap_core::message::HeldShare;
use frostsnap_fonts::WARNING_ICON;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-icon-80x96.bmp");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            BmpImage,
            SizedBox<Rgb565>,
            Option<
                Row<(
                    Text<Gray4TextStyle>,
                    SizedBox<Rgb565>,
                    Column<(SizedBox<Rgb565>, Text)>,
                )>,
            >,
            SizedBox<Rgb565>,
            Text, // Key name
            SizedBox<Rgb565>,
            ShareIndexWidget,
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
            // Use the warning icon as a Gray4 font glyph
            let warning_icon = Text::new(
                "⚠".to_string(),
                Gray4TextStyle::new(&WARNING_ICON, PALETTE.warning),
            );

            let icon_spacer = SizedBox::new(Size::new(5, 0)); // 5px horizontal spacing

            let warning_text = Text::new(
                "Recovery Mode",
                DefaultTextStyle::new(crate::FONT_MED, PALETTE.warning),
            );

            // Add a small spacer above the text to align it with the icon
            let text_top_spacer = SizedBox::new(Size::new(0, 5)); // 5px adjustment
            let text_with_spacer = Column::new((text_top_spacer, warning_text));

            Some(Row::new((warning_icon, icon_spacer, text_with_spacer)))
        } else {
            None
        };

        // Create key name in medium emphasis grey
        let key_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);
        let key_text =
            Text::new(held_share.key_name.to_string(), key_style).with_alignment(Alignment::Center);

        // Extract share index and create the widget
        let share_index: u16 = held_share.share_image.index.try_into().unwrap();
        let share_index_widget = ShareIndexWidget::new(share_index, crate::FONT_SMALL);

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
            share_index_widget,
            spacer4,
            device_name_widget,
        ))
        .with_cross_axis_alignment(crate::CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }
}
