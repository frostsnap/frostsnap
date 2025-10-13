use crate::{
    any_of::AnyOf, layout::Column, p2tr_address_display::P2trAddressDisplay, palette::PALETTE,
    text::Text, DefaultTextStyle, MainAxisAlignment, FONT_HUGE_MONO, FONT_LARGE,
};
use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use embedded_graphics::text::Alignment;
use frostsnap_fonts::Gray4Font;
use frostsnap_macros::Widget;

// Font for displaying addresses - uses monospace for better readability
const ADDRESS_FONT: &Gray4Font = FONT_HUGE_MONO;

/// A widget that displays only a Bitcoin address (without derivation path)
#[derive(Widget)]
pub struct AddressDisplay {
    #[widget_delegate]
    container: Box<AnyOf<(P2trAddressDisplay, Text)>>,
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
                DefaultTextStyle::new(ADDRESS_FONT, PALETTE.on_background),
            )
            .with_alignment(Alignment::Center);

            let container = Box::new(AnyOf::new(address_text));
            Self { container }
        }
    }

    pub fn set_rand_highlight(&mut self, rand_highlight: u32) {
        // Try to downcast to P2trAddressDisplay and apply highlighting
        if let Some(p2tr_display) = self.container.downcast_mut::<P2trAddressDisplay>() {
            p2tr_display.set_rand_highlight(rand_highlight);
        }
    }
}

/// A widget that displays a Bitcoin address with its index
#[derive(Widget)]
pub struct AddressWithPath {
    #[widget_delegate]
    container: Column<(Text, AddressDisplay)>,
}

impl AddressWithPath {
    pub fn new(address: bitcoin::Address, address_index: u32) -> Self {
        let address_display = AddressDisplay::new(address);

        // Create the address index text (e.g., "Address #5")
        let index_text = Text::new(
            alloc::format!("Address #{}", address_index),
            DefaultTextStyle::new(FONT_LARGE, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        // Create a column to stack them vertically (index above address)
        let container = Column::builder()
            .push(index_text)
            .gap(8)
            .push(address_display)
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        Self { container }
    }

    pub fn set_rand_highlight(&mut self, rand_highlight: u32) {
        // Apply highlighting to the address display
        self.container.children.1.set_rand_highlight(rand_highlight);
    }
}
