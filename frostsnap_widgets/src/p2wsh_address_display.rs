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

/// A widget for displaying P2WSH addresses
/// P2WSH addresses are 62 characters long (same as P2TR)
/// Uses the same display format as P2TR addresses
#[derive(frostsnap_macros::Widget)]
pub struct P2wshAddressDisplay {
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

impl P2wshAddressDisplay {
    pub fn new(address: &str) -> Self {
        // Use default seed but provide API for passing time/randomness
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: &str, seed: u32) -> Self {
        // P2WSH addresses are 62 characters (same as P2TR)
        // Split into chunks of 4 characters, padding with spaces as needed
        let chunks: Vec<String> = (0..address.len())
            .step_by(4)
            .map(|start| {
                let end = (start + 4).min(address.len());
                let chunk = &address[start..end];
                // Pad to 4 characters with spaces
                format!("{:4}", chunk)
            })
            .collect();

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
            second_highlight = 1 + ((second_highlight + 1) % 14);
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

        // Create rows with 3 chunks each
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

        // Row 3: chunks 9, 10, 11
        let row3 = Row::new((
            Text::new(
                chunks[9].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(9)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[10].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(10)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[11].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(11)),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 4: chunks 12, 13, 14
        let row4 = Row::new((
            Text::new(
                chunks[12].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(12)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[13].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(13)),
            ),
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                chunks[14].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, get_color(14)),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

        // Row 5: Last 2 characters centered in middle column (same as P2TR)
        // The last chunk (chunks[15]) is already padded to 4 chars, but we want to center it
        let last_chunk = &address[60..62]; // Get the actual last 2 characters
        let centered_last_chunk = format!(" {} ", last_chunk); // Center within 4 chars: " XY "

        let row5 = Row::new((
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ), // 4 spaces for empty left column
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                centered_last_chunk,
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ), // Last 2 chars centered in their chunk (never highlighted)
            SizedBox::<Rgb565>::new(Size::new(ADDRESS_HORIZONTAL_SPACING, 1)),
            Text::new(
                "    ".to_string(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, normal_color),
            ), // 4 spaces for empty right column
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center);

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
