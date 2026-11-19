use super::clear_rect;
use crate::{palette::PALETTE, prelude::*};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
};

/// A row of progress dots with no number: the device round's five boxes and the gate's five
/// released words (spec L2.7.7, §0.6.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dots {
    count: u8,
    filled: u8,
    diameter: u32,
    gap: u32,
    dirty: bool,
}

impl Dots {
    pub fn new(count: u8, diameter: u32, gap: u32) -> Self {
        Self {
            count,
            filled: 0,
            diameter,
            gap,
            dirty: true,
        }
    }

    pub fn set_filled(&mut self, filled: u8) {
        let filled = filled.min(self.count);
        if filled != self.filled {
            self.filled = filled;
            self.dirty = true;
        }
    }

    pub fn filled(&self) -> u8 {
        self.filled
    }

    pub fn size(&self) -> Size {
        Size::new(
            self.count as u32 * self.diameter + (self.count as u32).saturating_sub(1) * self.gap,
            self.diameter,
        )
    }

    pub fn force_full_redraw(&mut self) {
        self.dirty = true;
    }

    /// Draw at `top_left`, if anything changed.
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        top_left: Point,
    ) -> Result<(), D::Error> {
        if !core::mem::take(&mut self.dirty) {
            return Ok(());
        }
        clear_rect(target, Rectangle::new(top_left, self.size()))?;
        let filled = PrimitiveStyle::with_fill(PALETTE.primary);
        let stroke = (self.diameter / 7).max(1);
        let empty = PrimitiveStyle::with_stroke(PALETTE.outline, stroke);
        for i in 0..self.count {
            let x = top_left.x + (i as u32 * (self.diameter + self.gap)) as i32;
            let circle = Circle::new(Point::new(x, top_left.y), self.diameter);
            let style = if i < self.filled { filled } else { empty };
            circle.into_styled(style).draw(target)?;
        }
        Ok(())
    }
}
