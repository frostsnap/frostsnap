use crate::{
    Column, Row, text::Text, MainAxisAlignment, palette::PALETTE
};
use alloc::{string::{String, ToString}, vec::Vec, collections::BTreeSet};
use embedded_graphics::{
    pixelcolor::Rgb565,
};
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying P2WPKH (native segwit) addresses
/// P2WPKH addresses are 42 characters long (starting with bc1q)
/// Display format: 42 chars = 36 chars (3 rows × 3 chunks) + 6 leftover
/// - 3 rows with 3 chunks each (36 chars)
/// - 1 row with last 6 chars: 4 in left column, 2 in center column
/// - 2 empty rows to match P2TR/P2WSH height for consistent positioning
#[derive(frostsnap_macros::Widget)]
pub struct P2wpkhAddressDisplay {
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

impl P2wpkhAddressDisplay {
    pub fn new(address: &str) -> Self {
        // Use default seed but provide API for passing time/randomness
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: &str, seed: u32) -> Self {
        // P2WPKH addresses are 42 characters (ASCII)
        // First 36 characters as regular chunks
        let chunks: Vec<String> = (0..36).step_by(4).map(|start| {
            let end = (start + 4).min(address.len());
            let chunk = &address[start..end];
            // Pad to 4 characters with spaces if needed
            format!("{:4}", chunk)
        }).collect();

        // Select two random chunks to highlight (excluding first chunk)
        // Valid chunks are indices 1-8 (9 full chunks total, excluding 0 which is "bc1q")
        // We don't highlight the partial last row chunks
        let mut highlighted_chunks = BTreeSet::new();

        // Use provided seed for randomness (e.g., current timestamp)
        // This prevents address poisoning attacks by making highlights unpredictable

        // Select first highlight chunk (indices 1-8)
        let first_highlight = 1 + (seed % 8) as usize;
        highlighted_chunks.insert(first_highlight);

        // Select second highlight chunk (different from first)
        let mut second_highlight = 1 + ((seed.wrapping_mul(7).wrapping_add(3)) % 8) as usize;
        while second_highlight == first_highlight {
            second_highlight = 1 + ((second_highlight + 1) % 8) as usize;
        }
        highlighted_chunks.insert(second_highlight);

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

        // Row 0: chunks 0, 1, 2 (bc1q and first two data chunks)
        let row0 = Row::new((
            Text::new(chunks[0].clone(), get_style(0)),
            Text::new(chunks[1].clone(), get_style(1)),
            Text::new(chunks[2].clone(), get_style(2)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 1: chunks 3, 4, 5
        let row1 = Row::new((
            Text::new(chunks[3].clone(), get_style(3)),
            Text::new(chunks[4].clone(), get_style(4)),
            Text::new(chunks[5].clone(), get_style(5)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 2: chunks 6, 7, 8
        let row2 = Row::new((
            Text::new(chunks[6].clone(), get_style(6)),
            Text::new(chunks[7].clone(), get_style(7)),
            Text::new(chunks[8].clone(), get_style(8)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 3: Last 6 characters
        // Characters 36-39 (4 chars) in left column
        // Characters 40-41 (2 chars) as first 2 chars of center column
        // Right column empty
        let last_4_chars = &address[36..40];
        let last_2_chars = &address[40..42];

        let row3 = Row::new((
            Text::new(format!("{:4}", last_4_chars), normal_style.clone()),  // Left column: last 4 chars (not highlighted)
            Text::new(format!("{}  ", last_2_chars), normal_style.clone()),  // Center column: last 2 chars + 2 spaces (not highlighted)
            Text::new("    ".to_string(), normal_style.clone()),             // Right column: empty (4 spaces)
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 4: Empty row for consistent spacing
        let row4 = Row::new((
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 5: Empty row for consistent spacing (matches P2TR/P2WSH 6-row height)
        let row5 = Row::new((
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
            Text::new("    ".to_string(), normal_style.clone()),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Create column with all 6 rows to match P2TR/P2WSH height
        let column = Column::new((row0, row1, row2, row3, row4, row5));

        Self { column }
    }
}