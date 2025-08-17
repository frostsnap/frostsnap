use crate::palette::PALETTE;
use crate::super_draw_target::SuperDrawTarget;
use crate::Widget;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};

// Font size matching the one used in bip39_input_preview
const FONT_SIZE: Size = Size::new(16, 24);

#[derive(Debug)]
pub struct Cursor {
    visible: bool,
    last_toggle: Option<crate::Instant>,
    pub position: Point,
}

impl Cursor {
    pub fn new(position: Point) -> Self {
        Self {
            visible: true,
            last_toggle: None,
            position,
        }
    }

    pub fn set_position(&mut self, new_position: Point) {
        if self.position != new_position {
            self.position = new_position;
            self.visible = true;
            self.last_toggle = None;
        }
    }
}

impl crate::DynWidget for Cursor {
    fn set_constraints(&mut self, _max_size: Size) {
        // Cursor has a fixed size
    }

    fn sizing(&self) -> crate::Sizing {
        // Cursor is just a small underline with fixed size
        crate::Sizing {
            width: FONT_SIZE.width - 4,
            height: FONT_SIZE.height,
        }
    }
}

impl Widget for Cursor {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Update visibility based on time
        let cursor_rect = Rectangle::new(
            Point::new(
                self.position.x,
                self.position.y + FONT_SIZE.height as i32 - 4,
            ),
            Size::new(FONT_SIZE.width - 4, 2),
        );

        if let Some(last_toggle) = self.last_toggle {
            // Check if 600ms has passed since last toggle
            if current_time.saturating_duration_since(last_toggle) >= 600 {
                self.visible = !self.visible;
                self.last_toggle = Some(current_time);

                // Draw or clear based on new visibility state
                if self.visible {
                    cursor_rect
                        .into_styled(PrimitiveStyle::with_fill(PALETTE.primary))
                        .draw(target)?;
                } else {
                    cursor_rect
                        .into_styled(PrimitiveStyle::with_fill(PALETTE.background))
                        .draw(target)?;
                }
            }
        } else {
            // First time - draw cursor
            self.last_toggle = Some(current_time);
            cursor_rect
                .into_styled(PrimitiveStyle::with_fill(PALETTE.primary))
                .draw(target)?;
        }

        Ok(())
    }
}
