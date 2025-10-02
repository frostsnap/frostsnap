use crate::{
    fonts::{Gray4TextStyle, NOTO_SANS_24_BOLD, NOTO_SANS_MONO_28_BOLD},
    layout::{MainAxisAlignment, Row},
    palette::PALETTE,
    text::Text,
};

/// A widget that displays a share index with "#" in secondary color and the number in primary
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexWidget {
    #[widget_delegate]
    row: Row<(Text<Gray4TextStyle<'static>>, Text<Gray4TextStyle<'static>>)>,
}

impl ShareIndexWidget {
    /// Create with large font (default, used in backup display)
    pub fn new(share_index: u16) -> Self {
        Self::new_large(share_index)
    }

    /// Create with large font (28pt mono)
    pub fn new_large(share_index: u16) -> Self {
        // "#" in secondary color
        let hash_text = Text::new(
            "#",
            Gray4TextStyle::new(&NOTO_SANS_MONO_28_BOLD, PALETTE.text_secondary),
        );

        // Index number in primary color
        let index_text = Text::new(
            format!("{}", share_index),
            Gray4TextStyle::new(&NOTO_SANS_MONO_28_BOLD, PALETTE.primary),
        );

        let row = Row::builder()
            .push(hash_text)
            .push(index_text)
            .with_main_axis_alignment(MainAxisAlignment::Center);

        Self { row }
    }

    /// Create with medium font (24pt, for more compact displays)
    pub fn new_medium(share_index: u16) -> Self {
        // "#" in secondary color
        let hash_text = Text::new(
            "#",
            Gray4TextStyle::new(&NOTO_SANS_24_BOLD, PALETTE.text_secondary),
        );

        // Index number in primary color
        let index_text = Text::new(
            format!("{}", share_index),
            Gray4TextStyle::new(&NOTO_SANS_24_BOLD, PALETTE.primary),
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
