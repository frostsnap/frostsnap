use crate::{
    fonts::{Gray4TextStyle, NOTO_SANS_MONO_24_BOLD},
    palette::PALETTE, text::Text, Column, MainAxisAlignment, Row, SizedBox
};
use alloc::{collections::BTreeSet, string::{String, ToString}, vec::Vec};
use embedded_graphics::{
    pixelcolor::Rgb565,
    geometry::Size,
};

// Font and spacing constants for addresses
const FONT_BITCOIN_ADDRESS: &crate::fonts::Gray4Font = &NOTO_SANS_MONO_24_BOLD;
const ADDRESS_HORIZONTAL_SPACING: u32 = 15; // Horizontal spacing between chunks
const ADDRESS_VERTICAL_SPACING: u32 = 3; // Vertical spacing between rows

/// A widget for displaying P2PKH (Pay-to-Pubkey-Hash) addresses
/// P2PKH addresses start with '1' and are typically 34 characters long
/// Display format: Simple chunking, left to right, no special handling
#[derive(frostsnap_macros::Widget)]
pub struct P2pkhAddressDisplay {
    #[widget_delegate]
    column: Column<(
        Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>,
        SizedBox<Rgb565>,
        Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>,
        SizedBox<Rgb565>,
        Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>,
        SizedBox<Rgb565>,
        Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>,
        SizedBox<Rgb565>,
        Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>,
        SizedBox<Rgb565>,
        Row<(Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>, SizedBox<Rgb565>, Text<Gray4TextStyle<'static>>)>,
    )>,
}

impl P2pkhAddressDisplay {
    pub fn new(address: &str) -> Self {
        // Use default seed but provide API for passing time/randomness
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: &str, seed: u32) -> Self {
        // P2PKH addresses are typically 34 characters
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

        // Create text colors - blue for normal, white for highlighted
        let normal_color = PALETTE.primary;
        let highlight_color = PALETTE.on_background;

        // Helper to get the appropriate color for a chunk index
        let get_color = |idx: usize| -> Rgb565 {
            if highlighted_chunks.contains(&idx) {
                highlight_color
            } else {
                normal_color
            }
        };

        // Create 6 rows with 3 chunks each
        let row0 = Row::new((
            Text::new(chunks[0].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(0))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks[1].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(1))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks[2].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(2))),
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        let row1 = Row::new((
            Text::new(chunks[3].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(3))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks[4].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(4))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks[5].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(5))),
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        let row2 = Row::new((
            Text::new(chunks[6].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(6))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks[7].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(7))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks[8].clone(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(8))),
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        // For a 34-character address, we'll have:
        // Row 0-2: 36 chars (9 chunks × 4 chars)
        // So row 3 will be empty or have minimal content
        let row3 = Row::new((
            Text::new(chunks.get(9).cloned().unwrap_or("    ".to_string()), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(9))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks.get(10).cloned().unwrap_or("    ".to_string()), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(10))),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(chunks.get(11).cloned().unwrap_or("    ".to_string()), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(11))),
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        // Empty rows for consistent 6-row height
        let row4 = Row::new((
            Text::new("    ".to_string(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color)),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new("    ".to_string(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color)),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new("    ".to_string(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color)),
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        let row5 = Row::new((
            Text::new("    ".to_string(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color)),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new("    ".to_string(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color)),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new("    ".to_string(), Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color)),
        )).with_main_axis_alignment(MainAxisAlignment::Center);

        // Create column with all rows and vertical spacing
        let column = Column::new((
            row0,
            SizedBox::<Rgb565>::new(Size::new(1, ADDRESS_VERTICAL_SPACING)),
            row1,
            SizedBox::<Rgb565>::new(Size::new(1, ADDRESS_VERTICAL_SPACING)),
            row2,
            SizedBox::<Rgb565>::new(Size::new(1, ADDRESS_VERTICAL_SPACING)),
            row3,
            SizedBox::<Rgb565>::new(Size::new(1, ADDRESS_VERTICAL_SPACING)),
            row4,
            SizedBox::<Rgb565>::new(Size::new(1, ADDRESS_VERTICAL_SPACING)),
            row5,
        ));

        Self { column }
    }
}

// All trait implementations are now generated by the derive macro