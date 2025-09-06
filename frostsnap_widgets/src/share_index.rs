use crate::{
    layout::{MainAxisAlignment, Row},
    palette::PALETTE,
    text::Text,
};
use embedded_graphics::pixelcolor::Rgb565;
use u8g2_fonts::{fonts, U8g2TextStyle};

// Font sizes for share index
const FONT_LARGE: fonts::u8g2_font_inr30_mf = fonts::u8g2_font_inr30_mf;
const FONT_MEDIUM: fonts::u8g2_font_inr21_mf = fonts::u8g2_font_inr21_mf;

/// A widget that displays a share index with "#" in secondary color and the number in primary
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexWidget {
    #[widget_delegate]
    row: Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
}

impl ShareIndexWidget {
    /// Create with large font (default, used in backup display)
    pub fn new(share_index: u16) -> Self {
        Self::new_large(share_index)
    }

    /// Create with large font
    pub fn new_large(share_index: u16) -> Self {
        // "#" in secondary color
        let hash_text = Text::new("#", U8g2TextStyle::new(FONT_LARGE, PALETTE.text_secondary));

        // Index number in primary color
        let index_text = Text::new(
            format!("{}", share_index),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary),
        );

        let row = Row::builder()
            .push(hash_text)
            .push(index_text)
            .with_main_axis_alignment(MainAxisAlignment::Center);

        Self { row }
    }

    /// Create with medium font (for more compact displays)
    pub fn new_medium(share_index: u16) -> Self {
        // "#" in secondary color
        let hash_text = Text::new("#", U8g2TextStyle::new(FONT_MEDIUM, PALETTE.text_secondary));

        // Index number in primary color
        let index_text = Text::new(
            format!("{}", share_index),
            U8g2TextStyle::new(FONT_MEDIUM, PALETTE.primary),
        );

        let row = Row::builder()
            .push(hash_text)
            .push(index_text)
            .with_main_axis_alignment(MainAxisAlignment::Center);

        Self { row }
    }

    /// Create with custom alignment
    pub fn with_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.row.main_axis_alignment = alignment;
        self
    }
}
