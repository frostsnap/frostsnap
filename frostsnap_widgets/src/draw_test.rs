use alloc::format;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Baseline, Text as EgText},
};

use crate::{DynWidget, Instant, SuperDrawTarget, Widget};

const COLOR_SWITCH_MS: u64 = 1000;
const COLORS: [Rgb565; 3] = [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE];

pub struct DrawTest {
    max_size: Size,
    color_index: usize,
    last_switch_time: Option<Instant>,
    last_fill_ms: u64,
}

impl Default for DrawTest {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawTest {
    pub fn new() -> Self {
        Self {
            max_size: Size::zero(),
            color_index: 0,
            last_switch_time: None,
            last_fill_ms: 0,
        }
    }

    fn current_color(&self) -> Rgb565 {
        COLORS[self.color_index]
    }

    #[cfg(target_arch = "riscv32")]
    fn measure_fill_ms<D: DrawTarget<Color = Rgb565>>(
        target: &mut SuperDrawTarget<D, Rgb565>,
        color: Rgb565,
    ) -> Result<u64, D::Error> {
        let start = esp_hal::time::Instant::now();
        target.clear(color)?;
        let end = esp_hal::time::Instant::now();
        Ok((end - start).as_millis())
    }

    #[cfg(not(target_arch = "riscv32"))]
    fn measure_fill_ms<D: DrawTarget<Color = Rgb565>>(
        target: &mut SuperDrawTarget<D, Rgb565>,
        color: Rgb565,
    ) -> Result<u64, D::Error> {
        target.clear(color)?;
        Ok(0)
    }
}

impl DynWidget for DrawTest {
    fn set_constraints(&mut self, max_size: Size) {
        self.max_size = max_size;
    }

    fn sizing(&self) -> crate::Sizing {
        self.max_size.into()
    }
}

impl Widget for DrawTest {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        let should_switch = match self.last_switch_time {
            None => true,
            Some(last) => current_time.saturating_duration_since(last) >= COLOR_SWITCH_MS,
        };

        if should_switch {
            if self.last_switch_time.is_some() {
                self.color_index = (self.color_index + 1) % COLORS.len();
            }
            self.last_fill_ms = Self::measure_fill_ms(target, self.current_color())?;
            self.last_switch_time = Some(current_time);
        }

        let text = format!("fill: {} ms", self.last_fill_ms);
        let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let text_width = (text.len() as i32) * 10;
        let text_height = 20i32;
        let padding_x = 8i32;
        let padding_y = 6i32;
        let box_w = (text_width + 2 * padding_x) as u32;
        let box_h = (text_height + 2 * padding_y) as u32;

        Rectangle::new(Point::new(0, 0), Size::new(box_w, box_h))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build(),
            )
            .draw(target)?;

        EgText::with_baseline(
            &text,
            Point::new(padding_x, padding_y),
            text_style,
            Baseline::Top,
        )
        .draw(target)?;

        Ok(())
    }
}
