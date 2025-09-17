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
        use bitcoin::AddressType;

        let address_str = address.to_string();

        // Check if this is a taproot address using the proper method
        if address.address_type() == Some(AddressType::P2tr) {
            // Use P2trAddressDisplay for taproot addresses
            let container = Box::new(AnyOf::new(P2trAddressDisplay::new(&address_str)));
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

/// A widget that displays a Bitcoin address with its derivation path
#[derive(Widget)]
pub struct AddressWithPath {
    #[widget_delegate]
    container: Column<(AddressDisplay, Text<Gray4TextStyle<'static>>)>,
}

impl AddressWithPath {
    pub fn new(address: bitcoin::Address, derivation_path: String) -> Self {
        let address_display = AddressDisplay::new(address);

        // Create the derivation path text (secondary, smaller)
        let path_text = Text::new(
            derivation_path,
            Gray4TextStyle::new(FONT_DERIVATION_PATH, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        // Create a column to stack them vertically
        let mut container = Column::new((address_display, path_text));
        container.main_axis_alignment = MainAxisAlignment::Center;
        container.cross_axis_alignment = CrossAxisAlignment::Center;

        Self { container }
    }
}
