use crate::palette::PALETTE;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Keyboard(char),
    WordSelector(usize),
    EditWord(usize),
    NavBack,
    NavForward,
    Submit,
}

#[derive(Debug)]
pub struct KeyTouch {
    pub key: Key,
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
    pub fn new(key: Key, rect: Rectangle) -> Self {
        Self {
            key,
            rect,
            let_go: None,
            last_draw: None,
            finished: false,
            cancel: false,
        }
    }
    pub fn let_go(&mut self, current_time: crate::Instant) -> Option<Key> {
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
            .stroke_color(PALETTE.background)
            .stroke_alignment(StrokeAlignment::Inside)
            .stroke_width(2)
            .build();

        if self.cancel {
            // Draw rounded rectangle with corner radius
            const CORNER_RADIUS: u32 = 8;
            let rounded_rect = RoundedRectangle::with_equal_corners(
                self.rect,
                Size::new(CORNER_RADIUS, CORNER_RADIUS),
            );
            let _ = rounded_rect.into_styled(highlight_style).draw(target);
            self.finished = true;
            return;
        }

        let fade_progress = match self.let_go {
            Some(let_go) => {
                let elapsed = current_time
                    .checked_duration_since(let_go)
                    .unwrap_or_else(|| crate::Duration::from_millis(0));

                let fade_duration = 500;
                (elapsed.to_millis() as f32 / fade_duration as f32).clamp(0.0, 1.0)
            }
            None => 0.0,
        };

        // At the end of fade, use the exact background color
        if fade_progress >= 1.0 {
            highlight_style.stroke_color = Some(PALETTE.background);
        } else {
            // Calculate fade from primary to background color
            let fade_factor = 1.0 - fade_progress;
            
            // Extract RGB components from both colors (in 5-6-5 format)
            let primary_r = PALETTE.primary.r();
            let primary_g = PALETTE.primary.g();
            let primary_b = PALETTE.primary.b();
            
            let bg_r = PALETTE.background.r();
            let bg_g = PALETTE.background.g();
            let bg_b = PALETTE.background.b();
            
            // Interpolate from primary to background
            let r = (primary_r as f32 * fade_factor + bg_r as f32 * fade_progress + 0.5) as u8;
            let g = (primary_g as f32 * fade_factor + bg_g as f32 * fade_progress + 0.5) as u8;
            let b = (primary_b as f32 * fade_factor + bg_b as f32 * fade_progress + 0.5) as u8;
            
            // Check if we need to redraw
            if let Some(last_draw) = self.last_draw {
                if r == last_draw && fade_progress < 1.0 {
                    return;
                }
            }
            
            self.last_draw = Some(r);
            let fade_color = Rgb565::new(r, g, b);
            highlight_style.stroke_color = Some(fade_color);
        }

        // Draw rounded rectangle with corner radius
        const CORNER_RADIUS: u32 = 8;
        let rounded_rect = RoundedRectangle::with_equal_corners(
            self.rect,
            Size::new(CORNER_RADIUS, CORNER_RADIUS),
        );
        let _ = rounded_rect.into_styled(highlight_style).draw(target);

        self.finished = fade_progress >= 1.0;
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}
