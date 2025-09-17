use crate::{
    bitmap::EncodedImage, image::Image, vec_framebuffer::VecFramebuffer,
    fonts::{Gray4TextStyle, NOTO_SANS_14_LIGHT, NOTO_SANS_18_MEDIUM, NOTO_SANS_24_BOLD, WARNING_ICON},
    palette::PALETTE, prelude::*, SizedBox,
};
use alloc::string::{String, ToString};
use embedded_graphics::{geometry::Size, pixelcolor::{BinaryColor, Rgb565}};
use frostsnap_core::message::HeldShare;

// Font constants for standby screen
const FONT_WALLET_LABEL: &crate::fonts::Gray4Font = &NOTO_SANS_14_LIGHT;
const FONT_KEY_NAME: &crate::fonts::Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_DEVICE_NAME: &crate::fonts::Gray4Font = &NOTO_SANS_24_BOLD;
const FONT_SHARE_INDEX: &crate::fonts::Gray4Font = &NOTO_SANS_14_LIGHT;
const FONT_WARNING: &crate::fonts::Gray4Font = &NOTO_SANS_14_LIGHT;

// Logo asset
const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A widget that displays the Frostsnap logo with a key name and device name
#[derive(frostsnap_macros::Widget)]
pub struct Standby {
    #[widget_delegate]
    content: Center<
        Column<(
            Image<VecFramebuffer<BinaryColor>, Rgb565>,  // Logo
            SizedBox<Rgb565>,   // Spacer after logo
            Option<Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>>, // Optional recovery warning
            Text<Gray4TextStyle<'static>>,  // "Wallet" label
            SizedBox<Rgb565>,   // Small spacer
            Text<Gray4TextStyle<'static>>,  // Key name
            SizedBox<Rgb565>,   // Spacer
            Text<Gray4TextStyle<'static>>,  // Share index
            SizedBox<Rgb565>,   // Spacer between sections
            Text<Gray4TextStyle<'static>>,  // Device name
        )>,
    >,
}

impl Standby {
    /// Create from simple data - useful for demos and testing
    pub fn new_simple(
        device_name: impl Into<String>,
        key_name: impl Into<String>,
        share_index: u16,
        is_recovery_mode: bool,
    ) -> Self {
        // Load logo from binary asset
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo = Image::with_color_map(framebuffer, |color| match color {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create spacers
        let spacer_after_logo = SizedBox::new(Size::new(0, 15));
        let small_spacer_1 = SizedBox::new(Size::new(0, 4));
        let small_spacer_2 = SizedBox::new(Size::new(0, 4));
        let large_spacer = SizedBox::new(Size::new(0, 20));

        // Create recovery mode warning if in recovery mode
        let recovery_warning = if is_recovery_mode {
            let warning_icon = Text::new(
                "⚠".to_string(),  // Warning icon from the WARNING_ICON font
                Gray4TextStyle::new(&WARNING_ICON, PALETTE.warning),
            );
            let icon_spacer = SizedBox::<Rgb565>::new(Size::new(5, 0));
            let warning_text = Text::new(
                "recovery mode".to_string(),
                Gray4TextStyle::new(FONT_WARNING, PALETTE.warning),
            );
            Some(Row::new((warning_icon, icon_spacer, warning_text))
                .with_cross_axis_alignment(CrossAxisAlignment::Center))
        } else {
            None
        };

        // Create "Wallet" label
        let wallet_label = Text::new(
            "Wallet".to_string(),
            Gray4TextStyle::new(FONT_WALLET_LABEL, PALETTE.text_secondary),
        );

        // Create key name
        let key_name = Text::new(
            key_name.into(),
            Gray4TextStyle::new(FONT_KEY_NAME, PALETTE.on_surface_variant),
        );

        // Create share index display
        let share_index_text = Text::new(
            format!("Key #{}", share_index),
            Gray4TextStyle::new(FONT_SHARE_INDEX, PALETTE.text_secondary),
        );

        // Create device name in primary color (large size)
        let device_name_text = Text::new(
            device_name.into(),
            Gray4TextStyle::new(FONT_DEVICE_NAME, PALETTE.primary),
        );

        // Create main column with all elements
        let column = Column::new((
            logo,
            spacer_after_logo,
            recovery_warning,
            wallet_label,
            small_spacer_1,
            key_name,
            large_spacer,
            share_index_text,
            small_spacer_2,
            device_name_text,
        ))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let content = Center::new(column);

        Self { content }
    }

    /// Create from HeldShare - for use in actual device
    pub fn new(device_name: impl Into<String>, held_share: HeldShare) -> Self {
        let share_index: u16 = held_share.share_image.index.try_into().unwrap();
        let is_recovery_mode = held_share.access_structure_ref.is_none();

        Self::new_simple(
            device_name,
            held_share.key_name,
            share_index,
            is_recovery_mode,
        )
    }
}
