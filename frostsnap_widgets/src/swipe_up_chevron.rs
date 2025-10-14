use crate::DefaultTextStyle;
use crate::{bobbing_carat::BobbingCarat, text::Text, Column, FONT_SMALL};
use embedded_graphics::pixelcolor::Rgb565;
use frostsnap_macros::Widget;

// Since Gray4TextStyle only works with Rgb565, we now specialize SwipeUpChevron for Rgb565
#[derive(Widget)]
pub struct SwipeUpChevron {
    column: Column<(BobbingCarat<Rgb565>, Text)>,
}

impl SwipeUpChevron {
    pub fn new(color: Rgb565, background_color: Rgb565) -> Self {
        let bobbing_carat = BobbingCarat::new(color, background_color);
        let text = Text::new("Swipe up", DefaultTextStyle::new(FONT_SMALL, color));
        let column = Column::new((bobbing_carat, text));
        Self { column }
    }
}
