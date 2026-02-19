use crate::palette::PALETTE;
use crate::super_draw_target::SuperDrawTarget;
use crate::{Container, DynWidget as _, Fader, SizedBox, Widget};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Keyboard(char),
    WordSelector(&'static str),
    EditWord(usize),
    NavBack,
    NavForward,
    ShowEnteredWords,
}

pub struct KeyTouch {
    pub key: Key,
    rect: Rectangle,
    let_go: Option<crate::Instant>,
    finished: bool,
    cancel: bool,
    widget: Fader<Container<SizedBox<Rgb565>>>,
}

impl KeyTouch {
    pub fn translate(&mut self, point: Point) {
        self.rect.top_left += point;
    }
    pub fn new(key: Key, rect: Rectangle) -> Self {
        const CORNER_RADIUS: u32 = 8;

        let sized_box = SizedBox::new(rect.size);
        let mut container = Container::with_size(sized_box, rect.size)
            .with_border(PALETTE.primary, 2)
            .with_corner_radius(Size::new(CORNER_RADIUS, CORNER_RADIUS));
        // HACK: the container is a fixed size so this doesn't matter
        container.set_constraints(Size {
            width: u32::MAX,
            height: u32::MAX,
        });
        let widget = Fader::new(container);

        Self {
            key,
            rect,
            let_go: None,
            finished: false,
            cancel: false,
            widget,
        }
    }
    pub fn let_go(&mut self, current_time: crate::Instant) -> Option<Key> {
        if self.cancel || self.let_go.is_some() {
            return None;
        }
        self.let_go = Some(current_time);
        // Start fade out animation
        self.widget.start_fade(100);
        Some(self.key)
    }

    pub fn cancel(&mut self) {
        self.cancel = true;
        // Immediately fade to background for cancel
        self.widget.instant_fade();
    }

    pub fn has_been_let_go(&self) -> bool {
        self.let_go.is_some()
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) {
        if self.finished {
            return;
        }

        // Translate the target to draw at the correct position
        let mut translated = target.clone().translate(self.rect.top_left);

        // Let the widget handle all drawing
        let _ = self.widget.draw(&mut translated, current_time);

        // Check if fade is complete
        if self.widget.is_faded_out() {
            self.finished = true;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

impl core::fmt::Debug for KeyTouch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KeyTouch")
            .field("key", &self.key)
            .field("rect", &self.rect)
            .field("let_go", &self.let_go)
            .field("finished", &self.finished)
            .field("cancel", &self.cancel)
            .finish()
    }
}
