use super::{Column, Text};
use crate::{
    bitmap::EncodedImage, image::Image, palette::PALETTE, vec_framebuffer::VecFramebuffer, Center,
};
use alloc::string::String;
use embedded_graphics::{
    pixelcolor::{BinaryColor, Rgb565},
    text::Alignment,
};
use u8g2_fonts::U8g2TextStyle;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            Image<VecFramebuffer<BinaryColor>, Rgb565>,
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>,
        )>,
    >,
}

impl Standby {
    pub fn new(key_name: impl Into<String>, device_name: impl Into<String>) -> Self {
        // Create text styles
        // Medium emphasis grey for key name (medium size)
        let key_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);

        // High emphasis for device name (large size)
        let device_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.on_background);

        // Create text widgets
        let key_text = Text::new(key_name.into(), key_style).with_alignment(Alignment::Center);
        let device_text =
            Text::new(device_name.into(), device_style).with_alignment(Alignment::Center);

        // Load logo
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo = Image::with_color_map(framebuffer, |color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create column with spacing
        let column = Column::new((logo, key_text, device_text))
            .with_main_axis_alignment(crate::MainAxisAlignment::SpaceEvenly);

        let content = Center::new(column);

        Self { content }
    }
}
