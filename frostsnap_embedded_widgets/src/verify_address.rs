use crate::{
    center::Center, p2tr_address_display::P2trAddressDisplay, text::Text, Column, MainAxisAlignment, Padding, palette::PALETTE
};
use alloc::{string::ToString, format};
use embedded_graphics::{
    pixelcolor::Rgb565,
};
use u8g2_fonts::U8g2TextStyle;

/// Widget for verifying a receive address (always taproot)
#[derive(frostsnap_macros::Widget)]
pub struct VerifyAddress {
    #[widget_delegate]
    center: Center<Padding<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        P2trAddressDisplay,
    )>>>,
}

impl VerifyAddress {
    pub fn new(address: &str, address_index: usize) -> Self {
        let title = Text::new(
            format!("Address #{}", address_index + 1),
            U8g2TextStyle::new(crate::FONT_MED, PALETTE.text_secondary)
        );

        // Use address_index combined with a large prime to generate varied seeds
        // In production, this should be combined with actual system time/randomness
        let seed = (address_index as u32).wrapping_mul(2654435761); // Large prime for better distribution
        let address_display = P2trAddressDisplay::new_with_seed(address, seed);

        let column = Column::new((title, address_display))
            .with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        let padded = Padding::only(column).bottom(40).build();

        Self {
            center: Center::new(padded)
        }
    }
}