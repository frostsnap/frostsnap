use crate::{
    any_of::AnyOf,
    gray4_style::Gray4TextStyle,
    layout::{Column, CrossAxisAlignment, MainAxisAlignment},
    p2pkh_address_display::P2pkhAddressDisplay,
    p2sh_address_display::P2shAddressDisplay,
    p2tr_address_display::P2trAddressDisplay,
    p2wpkh_address_display::P2wpkhAddressDisplay,
    p2wsh_address_display::P2wshAddressDisplay,
    palette::PALETTE,
    text::Text,
};
use alloc::{
    boxed::Box,
    string::{String, ToString},
};
use embedded_graphics::text::Alignment;
use frostsnap_fonts::{Gray4Font, NOTO_SANS_14_LIGHT};
use frostsnap_macros::Widget;

// Font constant for derivation path display
const FONT_DERIVATION_PATH: &Gray4Font = &NOTO_SANS_14_LIGHT;

/// A widget that displays only a Bitcoin address (without derivation path)
#[derive(Widget)]
pub struct AddressDisplay {
    #[widget_delegate]
    container: Box<
        AnyOf<(
            P2trAddressDisplay,
            P2wshAddressDisplay,
            P2wpkhAddressDisplay,
            P2shAddressDisplay,
            P2pkhAddressDisplay,
        )>,
    >,
}

impl AddressDisplay {
    pub fn new(address: bitcoin::Address) -> Self {
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: bitcoin::Address, seed: u32) -> Self {
        use bitcoin::AddressType;

        let address_str = address.to_string();

        // Route to the appropriate specialized display widget based on address type
        match address.address_type() {
            Some(AddressType::P2tr) => {
                let container = Box::new(AnyOf::new(P2trAddressDisplay::new_with_seed(
                    &address_str,
                    seed,
                )));
                Self { container }
            }
            Some(AddressType::P2wsh) => {
                let container = Box::new(AnyOf::new(P2wshAddressDisplay::new_with_seed(
                    &address_str,
                    seed,
                )));
                Self { container }
            }
            Some(AddressType::P2wpkh) => {
                let container = Box::new(AnyOf::new(P2wpkhAddressDisplay::new_with_seed(
                    &address_str,
                    seed,
                )));
                Self { container }
            }
            Some(AddressType::P2sh) => {
                let container = Box::new(AnyOf::new(P2shAddressDisplay::new_with_seed(
                    &address_str,
                    seed,
                )));
                Self { container }
            }
            Some(AddressType::P2pkh) => {
                let container = Box::new(AnyOf::new(P2pkhAddressDisplay::new_with_seed(
                    &address_str,
                    seed,
                )));
                Self { container }
            }
            None | Some(_) => {
                // Fallback for unknown address types - should never happen in practice
                panic!(
                    "Unsupported Bitcoin address type: {:?}",
                    address.address_type()
                )
            }
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
                    Text<Gray4TextStyle>,
                    crate::SizedBox<embedded_graphics::pixelcolor::Rgb565>,
                    AddressDisplay,
                    crate::SizedBox<embedded_graphics::pixelcolor::Rgb565>,
                    Text<Gray4TextStyle>,
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
        use embedded_graphics::pixelcolor::Rgb565;
        use frostsnap_fonts::NOTO_SANS_18_LIGHT;

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
