use crate::DefaultTextStyle;
use crate::{palette::PALETTE, prelude::*, BmpImage};
use embedded_graphics::{geometry::Size, text::Alignment};

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-icon-80x96.bmp");

/// A welcome screen widget showing the Frostsnap logo and getting started text
#[derive(frostsnap_macros::Widget)]
pub struct Welcome {
    #[widget_delegate]
    content: Center<
        Column<(
            BmpImage,
            SizedBox<embedded_graphics::pixelcolor::Rgb565>,
            Text,
            SizedBox<embedded_graphics::pixelcolor::Rgb565>,
            Text,
            SizedBox<embedded_graphics::pixelcolor::Rgb565>,
            Text,
        )>,
    >,
}

impl Welcome {
    pub fn new() -> Self {
        // Load BMP logo with color mapping
        let logo = BmpImage::new(LOGO_DATA, PALETTE.logo);

        // Create spacers with fixed heights
        let spacer1 = SizedBox::new(Size::new(0, 20)); // Space between logo and first text line
        let spacer2 = SizedBox::new(Size::new(0, 4)); // Space between text lines
        let spacer3 = SizedBox::new(Size::new(0, 16)); // Space between second text line and URL

        // Create text widgets with colors directly
        let text_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_background);
        let url_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.primary);

        let text_line1 =
            Text::new("Get started with", text_style.clone()).with_alignment(Alignment::Center);

        let text_line2 =
            Text::new("your Frostsnap at", text_style).with_alignment(Alignment::Center);

        let url_text =
            Text::new("frostsnap.com/start", url_style).with_alignment(Alignment::Center);

        // Create column with fixed spacing and center alignment
        let column = Column::new((
            logo, spacer1, text_line1, spacer2, text_line2, spacer3, url_text,
        ))
        .with_cross_axis_alignment(crate::CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}
