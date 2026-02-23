use crate::address_framebuffer::AddressFramebuffer;
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

/// A widget for displaying P2WPKH (native segwit) addresses.
/// P2WPKH addresses are 42 characters long (starting with bc1q).
/// Display: 3 rows of 3 chunks (36 chars) + 1 row with remaining 6 chars + 2 empty rows.
#[derive(frostsnap_macros::Widget)]
pub struct P2wpkhAddressDisplay {
    #[widget_delegate]
    framebuffer: AddressFramebuffer,
}

impl P2wpkhAddressDisplay {
    pub fn new(title: &str, address: &str) -> Self {
        Self::new_with_seed(title, address, 0)
    }

    pub fn new_with_seed(title: &str, address: &str, seed: u32) -> Self {
        // First 36 characters as regular 4-char chunks
        let mut chunks: Vec<String> = (0..36)
            .step_by(4)
            .map(|start| {
                let end = (start + 4).min(address.len());
                format!("{:4}", &address[start..end])
            })
            .collect();

        // Select two random chunks to highlight (indices 1-8)
        let mut highlighted_chunks = BTreeSet::new();
        let first_highlight = 1 + (seed % 8) as usize;
        highlighted_chunks.insert(first_highlight);
        let mut second_highlight = 1 + ((seed.wrapping_mul(7).wrapping_add(3)) % 8) as usize;
        while second_highlight == first_highlight {
            second_highlight = 1 + ((second_highlight + 1) % 8);
        }
        highlighted_chunks.insert(second_highlight);

        // Row 3: last 6 chars — 4 in left col, 2+spaces in center, empty right
        let last_4 = &address[36..40];
        let last_2 = &address[40..42];
        chunks.push(format!("{:4}", last_4));
        chunks.push(format!("{}  ", last_2));
        chunks.push("    ".into());

        // Rows 4-5: empty for consistent 6-row height
        for _ in 0..6 {
            chunks.push("    ".into());
        }

        let highlighted: Vec<bool> = (0..chunks.len())
            .map(|i| highlighted_chunks.contains(&i))
            .collect();

        let chunk_refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
        let framebuffer = AddressFramebuffer::from_chunks(title, &chunk_refs, &highlighted, 6);

        Self { framebuffer }
    }
}
