use crate::super_draw_target::SuperDrawTarget;
use crate::{palette::PALETTE, FONT_LARGE};
use alloc::{format, string::ToString};
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use u8g2_fonts::U8g2TextStyle;

#[derive(Debug)]
pub struct ShareIndexInputDisplay {
    pub index: Option<u16>,
    changed: bool,
}

impl Default for ShareIndexInputDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl ShareIndexInputDisplay {
    pub fn new() -> Self {
        ShareIndexInputDisplay {
            index: Default::default(),
            changed: true,
        }
    }

    pub fn min_height(&self) -> u32 {
        40
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_none()
    }

    pub fn is_full(&self) -> bool {
        self.index >= Some(100)
    }

    pub fn add_digit(&mut self, digit: u8) {
        if self.is_full() {
            return;
        }
        match &mut self.index {
            Some(index) => {
                *index *= 10;
                *index += digit as u16;
                self.changed = true;
            }
            None => {
                if digit != 0 {
                    self.changed = true;
                    self.index = Some(digit as u16);
                }
            }
        }
    }

    pub fn backspace(&mut self) {
        if let Some(index) = &mut self.index {
            *index /= 10;
            if *index == 0 {
                self.index = None;
            }
        }
        self.changed = true;
    }
}

impl crate::DynWidget for ShareIndexInputDisplay {
    fn set_constraints(&mut self, _max_size: Size) {
        // ShareIndexInputDisplay has fixed size for text display
    }

    fn sizing(&self) -> crate::Sizing {
        // This widget adapts to whatever size it's given
        // Return a reasonable minimum size for the text
        crate::Sizing {
            width: 240,
            height: 60,
        }
    }
}

impl crate::Widget for ShareIndexInputDisplay {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if !self.changed {
            return Ok(());
        }
        self.changed = false;
        let display_size = target.bounding_box().size;
        target.clear(PALETTE.background)?;

        let text = format!(
            "FROST[{}]",
            match self.index {
                Some(index) => index.to_string(),
                None => " ".to_string(),
            }
        );
        Text::with_text_style(
            &text,
            Point::new((display_size.width / 2) as i32, 15),
            U8g2TextStyle::new(FONT_LARGE, Rgb565::WHITE),
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(target)?;
        Ok(())
    }
}
