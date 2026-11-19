use crate::{backup::SuccessContent, prelude::*};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

/// How long the page stays after the checkmark starts, as on the standard check quiz.
const SUCCESS_DWELL_MS: u64 = 1800;

/// "Success!" over a line and the standard check circle, the look of "Backup Checked":
/// the card passed both checks (D8), or a later check's quiz ended (D9). It leaves by itself
/// once [`CardSuccess::is_finished`].
pub struct CardSuccess {
    content: SuccessContent,
    started: Option<Instant>,
    now: Option<Instant>,
}

impl CardSuccess {
    pub fn new(line: &str) -> Self {
        Self {
            content: SuccessContent::new(line),
            started: None,
            now: None,
        }
    }

    pub fn is_finished(&self) -> bool {
        match (self.started, self.now) {
            (Some(started), Some(now)) => {
                now.saturating_duration_since(started) >= SUCCESS_DWELL_MS
            }
            _ => false,
        }
    }
}

impl DynWidget for CardSuccess {
    fn set_constraints(&mut self, max_size: Size) {
        self.content.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.content.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.content.force_full_redraw();
    }
}

impl Widget for CardSuccess {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.now = Some(current_time);
        if self.started.is_none() {
            self.started = Some(current_time);
            let button = self.content.button_mut();
            button.force_full_redraw();
            button.checkmark_mut().start_drawing();
        }
        self.content.draw(target, current_time)
    }
}
