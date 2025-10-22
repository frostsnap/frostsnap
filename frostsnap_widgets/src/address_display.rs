use crate::{
    gray4_style::Gray4TextStyle,
    layout::{Column, CrossAxisAlignment, MainAxisAlignment},
    palette::PALETTE,
    text::Text,
    Row,
};
use alloc::{
    boxed::Box,
    collections::BTreeSet,
    string::{String, ToString},
};
use embedded_graphics::{pixelcolor::Rgb565, text::Alignment};
use frostsnap_fonts::{Gray4Font, NOTO_SANS_14_LIGHT, NOTO_SANS_MONO_24_BOLD};
use frostsnap_macros::Widget;

const FONT_BITCOIN_ADDRESS: &Gray4Font = &NOTO_SANS_MONO_24_BOLD;
const FONT_DERIVATION_PATH: &Gray4Font = &NOTO_SANS_14_LIGHT;
const ADDRESS_HORIZONTAL_SPACING: u32 = 15;
const ADDRESS_VERTICAL_SPACING: u32 = 3;
const EMPTY_CHUNK: &str = "    ";

type AddressRow = Row<(
    Text<Gray4TextStyle>,
    Text<Gray4TextStyle>,
    Text<Gray4TextStyle>,
)>;

/// Displays a Bitcoin address in a 6×3 grid of 4-character chunks with random highlighting.
#[derive(Clone, Widget)]
pub struct ChunkedAddressDisplay {
    #[widget_delegate]
    column: Column<(
        Box<AddressRow>,
        Box<AddressRow>,
        Box<AddressRow>,
        Box<AddressRow>,
        Box<AddressRow>,
        Box<AddressRow>,
    )>,
}

impl ChunkedAddressDisplay {
    pub fn new(chunks: [String; 18], highlighted: BTreeSet<usize>) -> Self {
        let normal_color = PALETTE.primary;
        let highlight_color = PALETTE.on_background;

        let color_for = |idx: usize| -> Rgb565 {
            if highlighted.contains(&idx) {
                highlight_color
            } else {
                normal_color
            }
        };

        let make_chunk = |idx: usize| -> Text<Gray4TextStyle> {
            Text::new(
                chunks[idx].clone(),
                Gray4TextStyle::new(FONT_BITCOIN_ADDRESS, color_for(idx)),
            )
        };

        let make_row = |i: usize| -> Box<AddressRow> {
            let base = i * 3;
            let mut row = Row::new((make_chunk(base), make_chunk(base + 1), make_chunk(base + 2)))
                .with_main_axis_alignment(MainAxisAlignment::Center);
            row.set_uniform_gap(ADDRESS_HORIZONTAL_SPACING);
            Box::new(row)
        };

        let mut column = Column::new((
            make_row(0),
            make_row(1),
            make_row(2),
            make_row(3),
            make_row(4),
            make_row(5),
        ));
        column.set_uniform_gap(ADDRESS_VERTICAL_SPACING);

        Self { column }
    }
}

/// Chunks an address string into 4-char pieces padded to exactly 18 slots.
/// For addresses whose last chunk has fewer than 4 chars, the remainder is
/// centered in the middle column of the last row.
fn chunk_address(address: &str) -> [String; 18] {
    let mut chunks: [String; 18] = core::array::from_fn(|_| EMPTY_CHUNK.to_string());
    let mut i = 0;
    let mut pos = 0;
    while pos < address.len() && i < 18 {
        let end = (pos + 4).min(address.len());
        let chunk = &address[pos..end];
        chunks[i] = alloc::format!("{:4}", chunk);
        i += 1;
        pos = end;
    }

    let total_full_chunks = address.len() / 4;
    let remainder = address.len() % 4;

    if remainder > 0 && total_full_chunks < 18 {
        // ⚖️ center the short last chunk in the middle column of its row
        let last_full_row_start = (total_full_chunks / 3) * 3;
        let col_in_row = total_full_chunks - last_full_row_start;

        if col_in_row != 1 {
            let tail = &address[total_full_chunks * 4..];
            let padded = alloc::format!("{:^4}", tail);

            chunks[total_full_chunks] = EMPTY_CHUNK.to_string();
            chunks[last_full_row_start] = EMPTY_CHUNK.to_string();
            chunks[last_full_row_start + 1] = padded;
            chunks[last_full_row_start + 2] = EMPTY_CHUNK.to_string();
        }
    }

    chunks
}

/// Picks 2 distinct highlighted chunk indices from range `1..num_chunks-1`.
fn pick_highlights(seed: u32, num_chunks: usize) -> BTreeSet<usize> {
    let mut highlighted = BTreeSet::new();
    if num_chunks <= 2 {
        return highlighted;
    }
    let range = (num_chunks - 2) as u32; // exclude first and last
    let first = 1 + (seed % range) as usize;
    highlighted.insert(first);

    let mut second = 1 + ((seed.wrapping_mul(7).wrapping_add(5)) % range) as usize;
    while second == first {
        second = 1 + ((second as u32 + 1) % range) as usize;
    }
    highlighted.insert(second);
    highlighted
}

/// A widget that displays a Bitcoin address with random chunk highlighting.
#[derive(Clone, Widget)]
pub struct AddressDisplay {
    #[widget_delegate]
    inner: Box<ChunkedAddressDisplay>,
}

impl AddressDisplay {
    pub fn new(address: bitcoin::Address) -> Self {
        Self::new_with_seed(address, 0)
    }

    pub fn new_with_seed(address: bitcoin::Address, seed: u32) -> Self {
        let address_str = address.to_string();
        let chunks = chunk_address(&address_str);

        let num_real_chunks = chunks.iter().filter(|c| !c.trim().is_empty()).count();
        let highlighted = pick_highlights(seed, num_real_chunks);

        Self {
            inner: Box::new(ChunkedAddressDisplay::new(chunks, highlighted)),
        }
    }
}

/// A widget that displays a Bitcoin address with its derivation path for receive flow
#[derive(Widget)]
pub struct AddressWithPath {
    #[widget_delegate]
    container: Box<
        crate::Center<
            crate::Padding<Column<(Text<Gray4TextStyle>, AddressDisplay, Text<Gray4TextStyle>)>>,
        >,
    >,
}

impl AddressWithPath {
    pub fn new(address: bitcoin::Address, derivation_path: String) -> Self {
        Self::new_with_index(address, derivation_path, 0)
    }

    pub fn new_with_index(
        address: bitcoin::Address,
        derivation_path: String,
        index: usize,
    ) -> Self {
        Self::new_with_seed(address, derivation_path, index, 0)
    }

    pub fn new_with_seed(
        address: bitcoin::Address,
        derivation_path: String,
        index: usize,
        seed: u32,
    ) -> Self {
        use frostsnap_fonts::NOTO_SANS_18_LIGHT;

        let title = Text::new(
            alloc::format!("Receive Address #{}", index),
            Gray4TextStyle::new(&NOTO_SANS_18_LIGHT, PALETTE.text_secondary),
        );

        let address_display = AddressDisplay::new_with_seed(address, seed);

        let path_text = Text::new(
            derivation_path,
            Gray4TextStyle::new(FONT_DERIVATION_PATH, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let mut column = Column::new((title, address_display, path_text))
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_gap(0, 10);
        column.set_gap(1, 15);

        let padded = crate::Padding::only(column).bottom(40).build();
        let centered = crate::Center::new(padded);

        Self {
            container: Box::new(centered),
        }
    }
}
