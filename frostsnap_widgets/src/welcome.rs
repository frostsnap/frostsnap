use crate::DefaultTextStyle;
use crate::{
    bitmap::EncodedImage, image::Image, palette::PALETTE, prelude::*,
    vec_framebuffer::VecFramebuffer,
};
use embedded_graphics::{
    pixelcolor::{BinaryColor, Rgb565},
    text::Alignment,
};

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A welcome screen widget showing the Frostsnap logo and getting started text
#[derive(frostsnap_macros::Widget)]
pub struct Welcome {
    #[widget_delegate]
    content: Center<Column<(Image<VecFramebuffer<BinaryColor>, Rgb565>, Text, Text)>>,
}

impl Welcome {
    pub fn new() -> Self {
        // Create text styles with colors directly
        let text_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.on_background);
        let url_style = DefaultTextStyle::new(crate::FONT_MED, PALETTE.primary);

        // Create text widgets with colored styles
        let text1 = Text::new("Get started with\nFrostsnap at", text_style.clone())
            .with_alignment(Alignment::Center);
        let url_text = Text::new("frostsnap.com/start", url_style).with_underline(PALETTE.primary);

        // Load logo
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo = Image::with_color_map(framebuffer, |color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create column with spacing
        let column = Column::new((logo, text1, url_text))
            .with_main_axis_alignment(crate::MainAxisAlignment::SpaceEvenly);

        let content = Center::new(column);

        Self { content }
    }
}

impl Default for Welcome {
    fn default() -> Self {
        Self::new()
    }
}
