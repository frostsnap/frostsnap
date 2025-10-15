use crate::DefaultTextStyle;
use crate::{
    layout::{MainAxisAlignment, Row},
    palette::PALETTE,
    text::Text,
};
use frostsnap_fonts::Gray4Font;

/// A widget that displays a share index with "Key #" in secondary color and the number in primary
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexWidget {
    #[widget_delegate]
    row: Row<(Text, Text, Text)>,
}

impl ShareIndexWidget {
    pub fn new(share_index: u16, font: &'static Gray4Font) -> Self {
        let key_text = Text::new("Key ", DefaultTextStyle::new(font, PALETTE.text_secondary));

        let hash_text = Text::new("#", DefaultTextStyle::new(font, PALETTE.text_secondary));

        let index_text = Text::new(
            format!("{}", share_index),
            DefaultTextStyle::new(font, PALETTE.text_secondary),
        );

        let row = Row::builder()
            .push(key_text)
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
