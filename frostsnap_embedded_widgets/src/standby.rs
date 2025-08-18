use super::{Widget, Text, Column};
use crate::{bitmap::EncodedImage, vec_framebuffer::VecFramebuffer, image::Image, palette::PALETTE, Center, SizedBox};
use alloc::string::String;
use embedded_graphics::{
    pixelcolor::{BinaryColor, Rgb565}, text::Alignment, prelude::*,
};
use u8g2_fonts::U8g2TextStyle;

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<Column<(
        Image<VecFramebuffer<BinaryColor>, Rgb565>,
        Column<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
        Text<U8g2TextStyle<Rgb565>>,
    )>>,
}

impl Standby {
    pub fn new(key_name: impl Into<String>, device_name: impl Into<String>) -> Self {
        // Create text styles
        // Small label style for "Wallet"
        let label_style = U8g2TextStyle::new(crate::FONT_SMALL, PALETTE.text_secondary);

        // Medium emphasis grey for key name (medium size)
        let key_style = U8g2TextStyle::new(crate::FONT_MED, PALETTE.on_surface_variant);

        // Blue color for device name (large size)
        let device_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.primary);

        // Create wallet section with label and name
        let wallet_label = Text::new("Wallet", label_style).with_alignment(Alignment::Center);
        let wallet_text = Text::new(key_name.into(), key_style).with_alignment(Alignment::Center);
        let wallet_column = Column::builder()
            .push(wallet_label)
            .push_with_gap(wallet_text, 4);  // Small gap between label and name

        // Create device text (no label)
        let device_text = Text::new(device_name.into(), device_style).with_alignment(Alignment::Center);

        // Load logo
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo = Image::with_color_map(framebuffer, |color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create main column with logo and labeled sections
        let column = Column::builder()
            .push(logo)
            .push_with_gap(wallet_column, 15)  // Gap after logo
            .push_with_gap(device_text, 20);  // Gap between wallet and device

        let content = Center::new(column);

        Self { content }
    }
}
