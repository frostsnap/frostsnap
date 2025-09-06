use crate::{palette::PALETTE, FadeSwitcher, Widget};
use embedded_graphics::pixelcolor::Rgb565;

/// A widget that switches between child widgets instantly without fading
/// Uses FadeSwitcher internally but with 0 duration for instant switching
#[derive(frostsnap_macros::Widget)]
pub struct Switcher<W: Widget<Color = Rgb565>> {
    fade_switcher: FadeSwitcher<W>,
}

impl<W: Widget<Color = Rgb565>> Switcher<W> {
    /// Create a new Switcher with an initial widget
    pub fn new(initial: W) -> Self {
        Self {
            fade_switcher: FadeSwitcher::new(initial, 0, 0, PALETTE.background),
        }
    }

    /// Switch to a new widget instantly
    pub fn switch_to(&mut self, widget: W) {
        // FadeSwitcher's switch_to only takes the widget
        self.fade_switcher.switch_to(widget);
    }

    /// Get a reference to the current widget
    pub fn current(&self) -> &W {
        self.fade_switcher.current()
    }

    /// Get a mutable reference to the current widget
    pub fn current_mut(&mut self) -> &mut W {
        self.fade_switcher.current_mut()
    }
}
