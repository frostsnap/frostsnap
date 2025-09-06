use crate::super_draw_target::SuperDrawTarget;
use crate::{
    string_ext::StringFixed, Container, DynWidget, Instant, Switcher, Text as TextWidget, Widget,
};
use core::fmt::Write;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor},
};

// Constants for FPS text dimensions - "FPS: 999" is max 8 chars
const FPS_MAX_CHARS: usize = 3;

type FpsDisplay = Container<Switcher<TextWidget<u8g2_fonts::U8g2TextStyle<Rgb565>>>>;

/// A widget that displays frames per second using simple frame counting
pub struct Fps {
    display: FpsDisplay,
    frame_count: u32,
    last_fps_time: Option<Instant>,
    last_display_update: Option<Instant>,
    current_fps: u32,
    update_interval_ms: u64,
}

impl Fps {
    /// Create a new FPS counter widget with green text
    pub fn new(update_interval_ms: u64) -> Self {
        let text_style = u8g2_fonts::U8g2TextStyle::new(crate::FONT_SMALL, Rgb565::GREEN);
        let text = TextWidget::new("000", text_style);
        let switcher = Switcher::new(text);
        let display = Container::new(switcher);

        Self {
            display,
            frame_count: 0,
            update_interval_ms,
            last_fps_time: None,
            last_display_update: None,
            current_fps: 0,
        }
    }
}

impl DynWidget for Fps {
    fn set_constraints(&mut self, max_size: Size) {
        self.display.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.display.sizing()
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn force_full_redraw(&mut self) {
        self.display.force_full_redraw();
    }
}

impl Widget for Fps {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Count frames
        self.frame_count += 1;

        // Calculate FPS every second
        let should_calculate = match self.last_fps_time {
            Some(last_time) => {
                let elapsed = current_time.saturating_duration_since(last_time);
                elapsed >= 1000
            }
            None => true,
        };

        if should_calculate {
            if let Some(last_time) = self.last_fps_time {
                let elapsed_ms = current_time.saturating_duration_since(last_time);
                if elapsed_ms > 0 {
                    // Calculate FPS: frames * 1000 / elapsed_ms
                    let fps = (self.frame_count as u64 * 1000) / elapsed_ms;
                    self.current_fps = fps as u32;
                }
            }

            // Reset counter for next second
            self.frame_count = 0;
            self.last_fps_time = Some(current_time);
        }

        // Update display at the configured interval
        let should_update_display = match self.last_display_update {
            Some(last_update) => {
                current_time.saturating_duration_since(last_update) >= self.update_interval_ms
            }
            None => true,
        };

        if should_update_display {
            // Format and update the display
            let mut buf = StringFixed::<FPS_MAX_CHARS>::new();
            write!(&mut buf, "{}", self.current_fps).ok();

            // Create new text widget with updated text
            let text_style = u8g2_fonts::U8g2TextStyle::new(crate::FONT_SMALL, Rgb565::GREEN);
            let text = TextWidget::new(buf.as_str(), text_style);
            self.display.child.switch_to(text);

            self.last_display_update = Some(current_time);
        }

        // Draw the display
        self.display.draw(target, current_time)
    }
}
