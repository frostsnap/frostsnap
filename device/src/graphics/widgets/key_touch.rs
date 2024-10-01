use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, StrokeAlignment},
};
use fugit::Duration;

#[derive(Debug)]
pub struct KeyTouch {
    pub key: char,
    rect: Rectangle,
    let_go: Option<crate::Instant>,
    last_draw: Option<u8>,
    finished: bool,
    cancel: bool,
}

impl KeyTouch {
    pub fn translate(&mut self, point: Point) {
        self.rect.top_left += point;
    }
    // Create a new KeyTouch
    pub fn new(key: char, rect: Rectangle) -> Self {
        Self {
            key,
            rect,
            let_go: None,
            last_draw: None,
            finished: false,
            cancel: false,
        }
    }
    pub fn let_go(&mut self, current_time: crate::Instant) -> Option<char> {
        if self.cancel || self.let_go.is_some() {
            return None;
        }
        self.let_go = Some(current_time);
        Some(self.key)
    }

    pub fn cancel(&mut self) {
        self.cancel = true;
    }

    pub fn has_been_let_go(&self) -> bool {
        self.let_go.is_some()
    }

    // Draw the highlight with a fade-out effect
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        if self.finished {
            return;
        }

        let mut highlight_style = PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::BLACK)
            .stroke_alignment(StrokeAlignment::Inside)
            .stroke_width(2)
            .build();

        if self.cancel {
            let _ = self.rect.into_styled(highlight_style).draw(target);
            self.finished = true;
            return;
        }

        let fade_progress = match self.let_go {
            Some(let_go) => {
                let elapsed = current_time
                    .checked_duration_since(let_go)
                    .unwrap_or_else(|| Duration::<u64, 1, 1_000_000>::millis(0));

                let fade_duration = 500;
                (elapsed.to_millis() as f32 / fade_duration as f32).clamp(0.0, 1.0)
            }
            None => 0.0,
        };

        let whiteness = (31 - (fade_progress * 31.0) as u32) as u8;
        if let Some(last_draw) = self.last_draw {
            if whiteness == last_draw {
                return;
            }
        }
        let gray_color = Rgb565::new(whiteness, whiteness << 1, whiteness); // Notice the shift for green

        highlight_style.stroke_color = Some(gray_color);

        let _ = self.rect.into_styled(highlight_style).draw(target);

        self.last_draw = Some(whiteness);
        self.finished = fade_progress >= 1.0;
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}
