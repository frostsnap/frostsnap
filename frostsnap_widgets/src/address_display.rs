use crate::{
    any_of::AnyOf,
    fonts::{Gray4TextStyle, NOTO_SANS_MONO_24_BOLD, NOTO_SANS_14_LIGHT},
    layout::{Column, CrossAxisAlignment, MainAxisAlignment},
    p2tr_address_display::P2trAddressDisplay,
    palette::PALETTE,
    text::Text,
};
use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use embedded_graphics::text::Alignment;
use frostsnap_macros::Widget;

// Font constants for address display
const FONT_BITCOIN_ADDRESS: &crate::fonts::Gray4Font = &NOTO_SANS_MONO_24_BOLD;
const FONT_DERIVATION_PATH: &crate::fonts::Gray4Font = &NOTO_SANS_14_LIGHT;

/// A widget that displays only a Bitcoin address (without derivation path)
#[derive(Widget)]
pub struct AddressDisplay {
    #[widget_delegate]
    container: Box<AnyOf<(P2trAddressDisplay, Text<Gray4TextStyle<'static>>)>>,
}

impl AddressDisplay {
    pub fn new(address: bitcoin::Address) -> Self {
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: bitcoin::Address, seed: u32) -> Self {
        use bitcoin::AddressType;

        let address_str = address.to_string();

        // Check if this is a taproot address using the proper method
        if address.address_type() == Some(AddressType::P2tr) {
            // Use P2trAddressDisplay for taproot addresses with random seed
            let container = Box::new(AnyOf::new(P2trAddressDisplay::new_with_seed(&address_str, seed)));
            Self { container }
        } else {
            // For non-taproot addresses, format with spaces every 4 characters
            let mut formatted = String::new();
            let mut space_count = 0;

            // Add spaces every 4 characters, replacing the third space with a newline
            for (i, ch) in address_str.chars().enumerate() {
                if i > 0 && i % 4 == 0 {
                    space_count += 1;
                    // Every third space becomes a newline
                    if space_count % 3 == 0 {
                        formatted.push('\n');
                    } else {
                        formatted.push(' ');
                    }
                }
                formatted.push(ch);
            }

            // Create the address text
            let address_text = Text::new(
                formatted,
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, PALETTE.on_background),
            )
            .with_alignment(Alignment::Center);

            let container = Box::new(AnyOf::new(address_text));
            Self { container }
        }
    }
}

/// A widget that displays a Bitcoin address with its derivation path for receive flow
#[derive(Widget)]
pub struct AddressWithPath {
    #[widget_delegate]
    container: Box<
        crate::Center<
            crate::Padding<
                Column<(
                    Text<Gray4TextStyle<'static>>,
                    crate::SizedBox<embedded_graphics::pixelcolor::Rgb565>,
                    AddressDisplay,
                    crate::SizedBox<embedded_graphics::pixelcolor::Rgb565>,
                    Text<Gray4TextStyle<'static>>,
                )>,
            >,
        >,
    >,
}

impl AddressWithPath {
    pub fn new(address: bitcoin::Address, derivation_path: String) -> Self {
        Self::new_with_index(address, derivation_path, 0)
    }

    pub fn new_with_index(
        address: bitcoin::Address,
        derivation_path: String,
        index: usize,
    ) -> Self {
        Self::new_with_seed(address, derivation_path, index, 0)
    }

    pub fn new_with_seed(
        address: bitcoin::Address,
        derivation_path: String,
        index: usize,
        seed: u32,
    ) -> Self {
        use crate::fonts::NOTO_SANS_18_LIGHT;
        use embedded_graphics::pixelcolor::Rgb565;

        // Header: "Receive Address #X"
        let title = Text::new(
            alloc::format!("Receive Address #{}", index),
            Gray4TextStyle::new(&NOTO_SANS_18_LIGHT, PALETTE.text_secondary),
        );

        let spacer1 = crate::SizedBox::<Rgb565>::new(embedded_graphics::geometry::Size::new(1, 10));

        // The taproot address display with random seed for anti-address-poisoning
        let address_display = AddressDisplay::new_with_seed(address, seed);

        let spacer2 = crate::SizedBox::<Rgb565>::new(embedded_graphics::geometry::Size::new(1, 15));

        // Derivation path underneath
        let path_text = Text::new(
            derivation_path,
            Gray4TextStyle::new(FONT_DERIVATION_PATH, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let column = Column::new((title, spacer1, address_display, spacer2, path_text))
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let padded = crate::Padding::only(column).bottom(40).build();
        let centered = crate::Center::new(padded);

        Self {
            container: Box::new(centered),
        }
    }
}
