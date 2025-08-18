use crate::{
    Column, Row, text::Text, MainAxisAlignment, palette::PALETTE
};
use alloc::{string::{String, ToString}, vec::Vec, collections::BTreeSet};
use embedded_graphics::{
    pixelcolor::Rgb565,
};
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying P2SH (Pay-to-Script-Hash) addresses
/// P2SH addresses start with '3' and are typically 34 characters long
/// Display format: Simple chunking, left to right, no special handling
#[derive(frostsnap_macros::Widget)]
pub struct P2shAddressDisplay {
    #[widget_delegate]
    column: Column<(
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
    )>,
}

impl P2shAddressDisplay {
    pub fn new(address: &str) -> Self {
        // Use default seed but provide API for passing time/randomness
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: &str, seed: u32) -> Self {
        // P2SH addresses are typically 34 characters
        // Simple chunking: divide into 4-character chunks
        let mut chunks: Vec<String> = Vec::new();
        let chars: Vec<char> = address.chars().collect();

        // Create chunks of 4 characters each
        for i in (0..chars.len()).step_by(4) {
            let end = (i + 4).min(chars.len());
            let chunk: String = chars[i..end].iter().collect();
            // Pad to 4 characters with spaces if needed
            chunks.push(format!("{:4}", chunk));
        }

        // Count actual non-empty chunks (34 chars = ~9 chunks)
        let actual_chunks = chunks.len();

        // Ensure we have at least 18 chunks (6 rows × 3 columns) for consistent display
        while chunks.len() < 18 {
            chunks.push("    ".to_string());
        }

        // Select two random chunks to highlight (excluding first chunk and empty chunks)
        // For 34 chars, we have about 9 chunks (0-8), so valid indices are 1-7
        // (excluding first chunk at index 0 and partial last chunk at index 8)
        let mut highlighted_chunks = BTreeSet::new();

        if actual_chunks > 2 {
            // Use provided seed for randomness
            let max_idx = (actual_chunks - 2).min(7); // Don't highlight last partial chunk

            // Select first highlight chunk (indices 1 to max_idx)
            let first_highlight = 1 + (seed % max_idx as u32) as usize;
            highlighted_chunks.insert(first_highlight);

            // Select second highlight chunk (different from first)
            let mut second_highlight = 1 + ((seed.wrapping_mul(7).wrapping_add(3)) % max_idx as u32) as usize;
            while second_highlight == first_highlight {
                second_highlight = 1 + ((second_highlight) % max_idx) as usize;
            }
            highlighted_chunks.insert(second_highlight);
        }

        // Create text styles - blue for normal, white for highlighted
        let normal_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.primary);
        let highlight_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.on_background);

        // Helper to get the appropriate style for a chunk index
        let get_style = |idx: usize| -> U8g2TextStyle<Rgb565> {
            if highlighted_chunks.contains(&idx) {
                highlight_style.clone()
            } else {
                normal_style.clone()
            }
        };

        // Create 6 rows with 3 chunks each
        let row0 = Row::new((
            Text::new(chunks[0].clone(), get_style(0)),
            Text::new(chunks[1].clone(), get_style(1)),
            Text::new(chunks[2].clone(), get_style(2)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        let row1 = Row::new((
            Text::new(chunks[3].clone(), get_style(3)),
            Text::new(chunks[4].clone(), get_style(4)),
            Text::new(chunks[5].clone(), get_style(5)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        let row2 = Row::new((
            Text::new(chunks[6].clone(), get_style(6)),
            Text::new(chunks[7].clone(), get_style(7)),
            Text::new(chunks[8].clone(), get_style(8)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // For a 34-character address, we'll have:
        // Row 0-2: 36 chars (9 chunks × 4 chars)
        // So row 3 will be empty or have minimal content
        let row3 = Row::new((
            Text::new(chunks.get(9).cloned().unwrap_or("    ".to_string()), get_style(9)),
            Text::new(chunks.get(10).cloned().unwrap_or("    ".to_string()), get_style(10)),
            Text::new(chunks.get(11).cloned().unwrap_or("    ".to_string()), get_style(11)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Empty rows for consistent 6-row height
        let row4 = Row::new((
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        let row5 = Row::new((
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Create column with all rows
        let column = Column::new((row0, row1, row2, row3, row4, row5));

        Self { column }
    }
}