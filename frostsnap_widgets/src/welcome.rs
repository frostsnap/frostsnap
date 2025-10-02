use crate::{
    bitmap::EncodedImage,
    fonts::{Gray4TextStyle, NOTO_SANS_18_LIGHT, NOTO_SANS_18_MEDIUM},
    image::Image,
    palette::PALETTE,
    prelude::*,
    vec_framebuffer::VecFramebuffer,
    SizedBox,
};
use alloc::string::ToString;
use embedded_graphics::{
    geometry::Size,
    pixelcolor::{BinaryColor, Rgb565},
};

// Logo asset
const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

// Font constants for welcome screen - matching the deviceui branch
const FONT_PAGE_HEADER: &crate::fonts::Gray4Font = &NOTO_SANS_18_LIGHT;
const FONT_CONFIRM_TITLE: &crate::fonts::Gray4Font = &NOTO_SANS_18_MEDIUM;

/// A welcome screen widget showing the Frostsnap logo and getting started text
#[derive(frostsnap_macros::Widget)]
pub struct Welcome {
    #[widget_delegate]
    content: Center<
        Column<(
            Image<VecFramebuffer<BinaryColor>, Rgb565>, // Logo
            SizedBox<Rgb565>,                           // Spacer
            Text<Gray4TextStyle<'static>>,              // First line of text
            SizedBox<Rgb565>,                           // Small spacer
            Text<Gray4TextStyle<'static>>,              // Second line of text
            SizedBox<Rgb565>,                           // Spacer
            Text<Gray4TextStyle<'static>>,              // URL text
        )>,
    >,
}

impl Welcome {
    pub fn new() -> Self {
        // Load logo from binary asset
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo = Image::with_color_map(framebuffer, |color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create text widgets using Gray4 fonts - split across two lines
        let text_line1 = Text::new(
            "Get started with".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.on_background),
        );

        let text_line2 = Text::new(
            "your Frostsnap at".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.on_background),
        );

        let url_text = Text::new(
            "frostsnap.com/start".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.primary),
        );

        // Create spacers with fixed heights
        let spacer1 = SizedBox::new(Size::new(0, 20)); // Space between logo and first text line
        let spacer2 = SizedBox::new(Size::new(0, 4)); // Space between text lines
        let spacer3 = SizedBox::new(Size::new(0, 16)); // Space between second text line and URL

        // Create column with fixed spacing and center alignment
        let column = Column::new((
            logo, spacer1, text_line1, spacer2, text_line2, spacer3, url_text,
        ))
        .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}
