use crate::{
    Column, Row, text::Text, MainAxisAlignment, palette::PALETTE, sized_box::SizedBox
};
use alloc::{string::{String, ToString}, vec::Vec, collections::BTreeSet};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
};
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying P2WSH addresses
/// P2WSH addresses are 62 characters long (same as P2TR)
/// Uses the same display format as P2TR addresses
#[derive(frostsnap_macros::Widget)]
pub struct P2wshAddressDisplay {
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

impl P2wshAddressDisplay {
    pub fn new(address: &str) -> Self {
        // Use default seed but provide API for passing time/randomness
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: &str, seed: u32) -> Self {
        // P2WSH addresses are 62 characters (same as P2TR)
        // Split into chunks of 4 characters, padding with spaces as needed
        let chunks: Vec<String> = (0..address.len()).step_by(4).map(|start| {
            let end = (start + 4).min(address.len());
            let chunk = &address[start..end];
            // Pad to 4 characters with spaces
            format!("{:4}", chunk)
        }).collect();

        // Select two random chunks to highlight (excluding first, last, and empty chunks)
        // Valid chunks are indices 1-14 (15 chunks total, excluding 0 and the special last one)
        let mut highlighted_chunks = BTreeSet::new();

        // Use provided seed for randomness (e.g., current timestamp)
        // This prevents address poisoning attacks by making highlights unpredictable

        // Select first highlight chunk (indices 1-14)
        let first_highlight = 1 + (seed % 14) as usize;
        highlighted_chunks.insert(first_highlight);

        // Select second highlight chunk (different from first)
        let mut second_highlight = 1 + ((seed.wrapping_mul(7).wrapping_add(5)) % 14) as usize;
        while second_highlight == first_highlight {
            second_highlight = 1 + ((second_highlight + 1) % 14) as usize;
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

        // Create rows with 3 chunks each
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

        // Row 3: chunks 9, 10, 11
        let row3 = Row::new((
            Text::new(chunks[9].clone(), get_style(9)),
            Text::new(chunks[10].clone(), get_style(10)),
            Text::new(chunks[11].clone(), get_style(11)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 4: chunks 12, 13, 14
        let row4 = Row::new((
            Text::new(chunks[12].clone(), get_style(12)),
            Text::new(chunks[13].clone(), get_style(13)),
            Text::new(chunks[14].clone(), get_style(14)),
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Row 5: Last 2 characters centered in middle column (same as P2TR)
        // The last chunk (chunks[15]) is already padded to 4 chars, but we want to center it
        let last_chunk = &address[60..62]; // Get the actual last 2 characters
        let centered_last_chunk = format!(" {} ", last_chunk); // Center within 4 chars: " XY "

        let row5 = Row::new((
            Text::new("    ".to_string(), normal_style.clone()), // 4 spaces for empty left column
            Text::new(centered_last_chunk, normal_style.clone()), // Last 2 chars centered in their chunk (never highlighted)
            Text::new("    ".to_string(), normal_style.clone()), // 4 spaces for empty right column
        )).with_main_axis_alignment(MainAxisAlignment::SpaceAround);

        // Create column with all rows
        let column = Column::new((row0, row1, row2, row3, row4, row5));

        Self { column }
    }
}