use super::{
    Para, TextBlock, BLOCK_GAP, FONT_BODY, FONT_TITLE, SCREEN_WIDTH, SIDE_PAD, TEXT_TOP, TEXT_WIDTH,
};
use crate::{palette::PALETTE, prelude::*, Frac, ProgressIndicator};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

/// Height of `ProgressIndicator`: the 20 px bar, an 8 px gap and the percentage line.
const PROGRESS_HEIGHT: u32 = 20 + 8 + 19;

/// A wait the user can't skip, with a progress bar: "Preparing your card", "Checking your
/// card...". The firmware updates the progress in place.
pub struct CardBusyScreen {
    text: TextBlock,
    progress: ProgressIndicator,
    progress_rect: Rectangle,
}

impl CardBusyScreen {
    pub fn new(title: &str, status: &str) -> Self {
        let text = TextBlock::new(
            &[
                Para::new(title, FONT_TITLE, PALETTE.on_background),
                Para::new(status, FONT_BODY, PALETTE.on_surface_variant),
            ],
            TEXT_TOP,
            SCREEN_WIDTH,
        );
        let progress_rect = Rectangle::new(
            Point::new(SIDE_PAD as i32, (text.bottom() + 2 * BLOCK_GAP) as i32),
            Size::new(TEXT_WIDTH, PROGRESS_HEIGHT),
        );
        Self {
            text,
            progress: ProgressIndicator::new(),
            progress_rect,
        }
    }

    /// `progress` runs from 0 to 1; values outside are clamped.
    pub fn set_progress(&mut self, progress: f32) {
        let per_mille = (progress.clamp(0.0, 1.0) * 1000.0) as u32;
        self.progress
            .set_progress(Frac::from_ratio(per_mille, 1000));
    }

    /// Where the progress indicator ends, in screen coordinates.
    #[cfg(test)]
    pub(crate) fn content_bottom(&self) -> u32 {
        self.progress_rect.top_left.y as u32 + PROGRESS_HEIGHT
    }

    #[cfg(test)]
    pub(crate) fn widest_line(&self) -> u32 {
        self.text.widest_line()
    }
}

impl DynWidget for CardBusyScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.text.set_constraints(max_size);
        self.progress.set_constraints(self.progress_rect.size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.text.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.text.force_full_redraw();
        self.progress.force_full_redraw();
    }
}

impl Widget for CardBusyScreen {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.text.draw(target, current_time)?;
        self.progress
            .draw(&mut target.clone().crop(self.progress_rect), current_time)
    }
}
