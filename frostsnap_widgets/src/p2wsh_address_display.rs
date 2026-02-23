use crate::address_framebuffer::AddressFramebuffer;
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

/// A widget for displaying P2WSH addresses.
/// P2WSH addresses are 62 characters long (same format as P2TR).
#[derive(frostsnap_macros::Widget)]
pub struct P2wshAddressDisplay {
    #[widget_delegate]
    framebuffer: AddressFramebuffer,
}

impl P2wshAddressDisplay {
    pub fn new(title: &str, address: &str) -> Self {
        Self::new_with_seed(title, address, 0)
    }

    pub fn new_with_seed(title: &str, address: &str, seed: u32) -> Self {
        // P2WSH addresses are 62 characters (same as P2TR)
        let mut chunks: Vec<String> = (0..address.len())
            .step_by(4)
            .map(|start| {
                let end = (start + 4).min(address.len());
                format!("{:4}", &address[start..end])
            })
            .collect();

        // Select two random chunks to highlight (indices 1-14)
        let mut highlighted_chunks = BTreeSet::new();
        let first_highlight = 1 + (seed % 14) as usize;
        highlighted_chunks.insert(first_highlight);
        let mut second_highlight = 1 + ((seed.wrapping_mul(7).wrapping_add(5)) % 14) as usize;
        while second_highlight == first_highlight {
            second_highlight = 1 + ((second_highlight + 1) % 14);
        }
        highlighted_chunks.insert(second_highlight);

        // Row 5: last 2 chars centered in middle column
        let last_chunk = &address[60..62];
        if chunks.len() > 15 {
            chunks.truncate(15);
        }
        chunks.push("    ".into());
        chunks.push(format!(" {} ", last_chunk));
        chunks.push("    ".into());

        let highlighted: Vec<bool> = (0..chunks.len())
            .map(|i| highlighted_chunks.contains(&i))
            .collect();

        let chunk_refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
        let framebuffer = AddressFramebuffer::from_chunks(title, &chunk_refs, &highlighted, 6);

        Self { framebuffer }
    }
}
