use super::{
    clear_rect, CardCopy, Dots, Para, TextBlock, BLOCK_GAP, COPY_GATE, COPY_GATE_PREPARING,
    FONT_BODY, FONT_SMALL_LINE, FONT_TITLE, SCREEN_WIDTH, TEXT_END_NONE, TEXT_TOP,
};
use crate::{palette::PALETTE, prelude::*};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

/// Words in the phone round, one dot each.
pub const GATE_DOTS: u8 = 5;
const DOT_DIAMETER: u32 = 14;
const DOT_GAP: u32 = 12;

/// The device's screen during the phone round: "Answer on your phone", a dot per word the
/// device has released so far, and the rules. While the device computes the next opening
/// it says so instead. The firmware updates it in place with [`GateScreen::set`].
pub struct GateScreen {
    dots: Dots,
    preparing: bool,
    title: TextBlock,
    rest: TextBlock,
    dots_rect: Rectangle,
    /// The previous mode's text is still on screen.
    stale: bool,
}

impl GateScreen {
    pub fn new(dots: u8, preparing: bool) -> Self {
        let (title, rest, dots_rect) = Self::layout(preparing);
        let mut gate_dots = Dots::new(GATE_DOTS, DOT_DIAMETER, DOT_GAP);
        gate_dots.set_filled(dots);
        Self {
            dots: gate_dots,
            preparing,
            title,
            rest,
            dots_rect,
            stale: false,
        }
    }

    fn layout(preparing: bool) -> (TextBlock, TextBlock, Rectangle) {
        let copy: CardCopy = if preparing {
            COPY_GATE_PREPARING
        } else {
            COPY_GATE
        };
        let title = TextBlock::new(
            &[Para::new(copy.title, FONT_TITLE, PALETTE.on_background)],
            TEXT_TOP,
            SCREEN_WIDTH,
        );
        let dots_width = GATE_DOTS as u32 * DOT_DIAMETER + (GATE_DOTS as u32 - 1) * DOT_GAP;
        let dots_rect = Rectangle::new(
            Point::new(
                (SCREEN_WIDTH as i32 - dots_width as i32) / 2,
                (title.bottom() + BLOCK_GAP) as i32,
            ),
            Size::new(dots_width, DOT_DIAMETER),
        );
        let rest = TextBlock::new(
            &[
                Para::new(copy.body, FONT_BODY, PALETTE.on_surface_variant),
                Para::new(copy.small, FONT_SMALL_LINE, PALETTE.text_secondary),
            ],
            dots_rect.top_left.y as u32 + DOT_DIAMETER + BLOCK_GAP,
            SCREEN_WIDTH,
        );
        (title, rest, dots_rect)
    }

    /// Show `dots` released words, and whether the next one is being prepared.
    pub fn set(&mut self, dots: u8, preparing: bool) {
        if preparing != self.preparing {
            let (title, rest, dots_rect) = Self::layout(preparing);
            self.title = title;
            self.rest = rest;
            self.dots_rect = dots_rect;
            self.stale = true;
            self.preparing = preparing;
            self.dots.force_full_redraw();
        }
        self.dots.set_filled(dots);
    }

    pub fn dots(&self) -> u8 {
        self.dots.filled()
    }

    pub fn is_preparing(&self) -> bool {
        self.preparing
    }

    #[cfg(test)]
    pub(crate) fn text_bottom(&self) -> u32 {
        self.rest.bottom()
    }

    #[cfg(test)]
    pub(crate) fn widest_line(&self) -> u32 {
        self.title.widest_line().max(self.rest.widest_line())
    }
}

impl DynWidget for GateScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.title.set_constraints(max_size);
        self.rest.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.title.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.title.force_full_redraw();
        self.rest.force_full_redraw();
        self.dots.force_full_redraw();
    }
}

impl Widget for GateScreen {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if core::mem::take(&mut self.stale) {
            let content = Rectangle::new(
                Point::new(0, TEXT_TOP as i32),
                Size::new(SCREEN_WIDTH, TEXT_END_NONE - TEXT_TOP),
            );
            clear_rect(target, content)?;
        }
        self.title.draw(target, current_time)?;
        self.rest.draw(target, current_time)?;

        self.dots.draw(target, self.dots_rect.top_left)?;
        Ok(())
    }
}
