use crate::palette::PALETTE;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};

#[derive(Debug)]
pub struct ProgressBars {
    total_bar_number: usize,
    progress: usize,
    pub(crate) redraw: bool,
}

impl ProgressBars {
    pub fn new(total_bar_number: usize) -> Self {
        Self {
            total_bar_number,
            progress: 0,
            redraw: true,
        }
    }

    pub fn progress(&mut self, progress: usize) {
        self.redraw = self.redraw || progress != self.progress;
        self.progress = progress;
    }

    pub(crate) fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        display: &mut D,
    ) -> Result<(), D::Error> {
        if !self.redraw {
            return Ok(());
        }

        const GAP_WIDTH: u32 = 2; // Smaller gap for 24 bars
        let size = display.bounding_box().size;

        let bar_width = (size.width - (self.total_bar_number as u32 - 1) * GAP_WIDTH)
            / self.total_bar_number as u32;
        let bar_height = size.height;

        for i in 0..self.total_bar_number {
            let x_offset = i as u32 * (bar_width + GAP_WIDTH);

            let color = if i < self.progress {
                PALETTE.tertiary // Draw green for progress
            } else {
                PALETTE.surface_variant // Draw grey for remaining bars
            };

            let bar = Rectangle::new(
                Point::new(x_offset as i32, 0),
                Size::new(bar_width, bar_height),
            );

            bar.into_styled(PrimitiveStyle::with_fill(color))
                .draw(display)?;
        }

        self.redraw = false;
        Ok(())
    }
}
