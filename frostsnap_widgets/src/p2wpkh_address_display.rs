use crate::{
    fonts::{Gray4TextStyle, NOTO_SANS_MONO_24_BOLD},
    palette::PALETTE,
    text::Text,
    Column, MainAxisAlignment, Row, SizedBox,
};
use alloc::{
    collections::BTreeSet,
    string::{String, ToString},
    vec::Vec,
};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565};

// Font and spacing constants for addresses
const FONT_BITCOIN_ADDRESS: &crate::fonts::Gray4Font = &NOTO_SANS_MONO_24_BOLD;
const ADDRESS_HORIZONTAL_SPACING: u32 = 15; // Horizontal spacing between chunks
const ADDRESS_VERTICAL_SPACING: u32 = 3; // Vertical spacing between rows

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
        Row<(
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
        )>,
        SizedBox<Rgb565>,
        Row<(
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
        )>,
        SizedBox<Rgb565>,
        Row<(
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
        )>,
        SizedBox<Rgb565>,
        Row<(
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
        )>,
        SizedBox<Rgb565>,
        Row<(
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
        )>,
        SizedBox<Rgb565>,
        Row<(
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
            SizedBox<Rgb565>,
            Text<Gray4TextStyle<'static>>,
        )>,
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
        let chunks: Vec<String> = (0..36)
            .step_by(4)
            .map(|start| {
                let end = (start + 4).min(address.len());
                let chunk = &address[start..end];
                // Pad to 4 characters with spaces if needed
                format!("{:4}", chunk)
            })
            .collect();

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
            second_highlight = 1 + ((second_highlight + 1) % 8);
        }
        highlighted_chunks.insert(second_highlight);

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

        // Row 0: chunks 0, 1, 2 (bc1q and first two data chunks)
        let row0 = Row::new((
            Text::new(
                chunks[0].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(0)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[1].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(1)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[2].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(2)),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 1: chunks 3, 4, 5
        let row1 = Row::new((
            Text::new(
                chunks[3].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(3)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[4].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(4)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[5].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(5)),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 2: chunks 6, 7, 8
        let row2 = Row::new((
            Text::new(
                chunks[6].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(6)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[7].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(7)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[8].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(8)),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 3: Last 6 characters
        // Characters 36-39 (4 chars) in left column
        // Characters 40-41 (2 chars) as first 2 chars of center column
        // Right column empty
        let last_4_chars = &address[36..40];
        let last_2_chars = &address[40..42];

        let row3 = Row::new((
            Text::new(
                format!("{:4}", last_4_chars),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ), // Left column: last 4 chars (not highlighted)
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                format!("{}  ", last_2_chars),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ), // Center column: last 2 chars + 2 spaces (not highlighted)
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ), // Right column: empty (4 spaces)
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 4: Empty row for consistent spacing
        let row4 = Row::new((
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 5: Empty row for consistent spacing (matches P2TR/P2WSH 6-row height)
        let row5 = Row::new((
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Create column with all 6 rows to match P2TR/P2WSH height with vertical spacing
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
