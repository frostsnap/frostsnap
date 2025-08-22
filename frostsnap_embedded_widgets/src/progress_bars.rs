use crate::{container::Container, layout::Row, palette::PALETTE, MainAxisAlignment};
use alloc::vec::Vec;
use embedded_graphics::prelude::*;
use frostsnap_macros::Widget;

#[derive(Widget)]
pub struct ProgressBars {
    #[widget_delegate]
    row: Row<Vec<Container<()>>>,
}

impl core::fmt::Debug for ProgressBars {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProgressBars")
            .field("bar_count", &self.row.children.len())
            .finish()
    }
}

impl ProgressBars {
    pub fn new(total_bar_number: usize) -> Self {
        // Create containers for each bar
        let mut bars = Vec::with_capacity(total_bar_number);
        let flex_scores = vec![1; total_bar_number];

        for _ in 0..total_bar_number {
            // Create container with the "off" color (surface_variant gray)
            // Fixed height of 8px, width will be determined by flex
            let container =
                Container::with_size((), Size::new(u32::MAX, 8)).with_fill(PALETTE.surface_variant);
            bars.push(container);
        }

        // Create the row
        let mut row = Row::new(bars).with_main_axis_alignment(MainAxisAlignment::Center);

        // Set flex scores
        row.flex_scores = flex_scores;

        // Set uniform 2px gap between bars
        row.set_uniform_gap(2);

        Self { row }
    }

    pub fn progress(&mut self, new_progress: usize) {
        // Clamp progress to valid range
        let new_progress = new_progress.min(self.row.children.len());

        // Iterate over all containers and set their color based on the progress
        for (i, container) in self.row.children.iter_mut().enumerate() {
            if i < new_progress {
                // This bar should be "on" - set to green
                if container.fill_color() != Some(PALETTE.tertiary) {
                    container.set_fill(PALETTE.tertiary);
                }
            } else {
                // This bar should be "off" - set to gray
                if container.fill_color() != Some(PALETTE.surface_variant) {
                    container.set_fill(PALETTE.surface_variant);
                }
            }
        }
    }
}
