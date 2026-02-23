use crate::address_framebuffer::AddressFramebuffer;
use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

/// A widget for displaying P2PKH (Pay-to-Pubkey-Hash) addresses.
/// P2PKH addresses start with '1' and are typically 34 characters long.
#[derive(frostsnap_macros::Widget)]
pub struct P2pkhAddressDisplay {
    #[widget_delegate]
    framebuffer: AddressFramebuffer,
}

impl P2pkhAddressDisplay {
    pub fn new(title: &str, address: &str) -> Self {
        Self::new_with_seed(title, address, 0)
    }

    pub fn new_with_seed(title: &str, address: &str, seed: u32) -> Self {
        let chars: Vec<char> = address.chars().collect();
        let mut chunks: Vec<String> = (0..chars.len())
            .step_by(4)
            .map(|i| {
                let end = (i + 4).min(chars.len());
                let chunk: String = chars[i..end].iter().collect();
                format!("{:4}", chunk)
            })
            .collect();

        let actual_chunks = chunks.len();

        // Select two random chunks to highlight
        let mut highlighted_chunks = BTreeSet::new();
        if actual_chunks > 2 {
            let max_idx = (actual_chunks - 2).min(7);
            let first_highlight = 1 + (seed % max_idx as u32) as usize;
            highlighted_chunks.insert(first_highlight);
            let mut second_highlight =
                1 + ((seed.wrapping_mul(7).wrapping_add(3)) % max_idx as u32) as usize;
            while second_highlight == first_highlight {
                second_highlight = 1 + ((second_highlight) % max_idx);
            }
            highlighted_chunks.insert(second_highlight);
        }

        // Pad to 18 chunks (6 rows × 3 columns)
        while chunks.len() < 18 {
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
