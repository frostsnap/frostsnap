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
    last_toggle: Option<crate::Instant>,
    last_draw_rect: Option<Rectangle>,
    rect: Rectangle,
    enabled: bool,
}

impl Cursor {
    pub fn new(position: Point) -> Self {
        Self {
            last_toggle: None,
            rect: Rectangle {
                top_left: position,
                size: Size::new(FONT_SIZE.width - 4, 2),
            },
            last_draw_rect: None,
            enabled: true,
        }
    }

    pub fn set_position(&mut self, new_position: Point) {
        self.rect.top_left = new_position;
        self.last_toggle = None;
    }

    pub fn enabled(&mut self, enabled: bool) {
        if enabled == self.enabled {
            return;
        }
        if !enabled {
            self.last_toggle = None;
        }
        self.enabled = enabled;
    }
}

impl crate::DynWidget for Cursor {
    fn set_constraints(&mut self, _max_size: Size) {
        // Cursor has a fixed size
    }

    fn sizing(&self) -> crate::Sizing {
        self.rect.size.into()
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
        if let Some(last_draw_rect) = self.last_draw_rect {
            if self.rect != last_draw_rect || !self.enabled {
                last_draw_rect
                    .into_styled(PrimitiveStyle::with_fill(PALETTE.background))
                    .draw(target)?;
                self.last_draw_rect = None;
            }
        }

        if !self.enabled {
            return Ok(());
        }

        let toggle_time = match self.last_toggle {
            Some(last_toggle) => current_time.saturating_duration_since(last_toggle) >= 600,
            None => true,
        };

        if toggle_time {
            if let Some(last_draw_rect) = self.last_draw_rect.take() {
                last_draw_rect
                    .into_styled(PrimitiveStyle::with_fill(PALETTE.background))
                    .draw(target)?;
            } else {
                self.rect
                    .into_styled(PrimitiveStyle::with_fill(PALETTE.primary))
                    .draw(target)?;
                self.last_draw_rect = Some(self.rect);
            }

            self.last_toggle = Some(current_time);
        }

        Ok(())
    }
}
