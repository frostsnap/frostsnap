use crate::{bobbing_carat::BobbingCarat, gray4_style::Gray4TextStyle, Column, SizedBox, Text};
use alloc::string::ToString;
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565};
use frostsnap_fonts::NOTO_SANS_14_LIGHT;
use frostsnap_macros::Widget;

const FONT_SWIPE_UP: &frostsnap_fonts::Gray4Font = &NOTO_SANS_14_LIGHT;

// Since Gray4TextStyle only works with Rgb565, we now specialize SwipeUpChevron for Rgb565
#[derive(Widget)]
pub struct SwipeUpChevron {
    column: Column<(BobbingCarat<Rgb565>, Text<Gray4TextStyle>, SizedBox<Rgb565>)>,
}

impl SwipeUpChevron {
    pub fn new(color: Rgb565, background_color: Rgb565) -> Self {
        let bobbing_carat = BobbingCarat::new(color, background_color);

        let text = Text::new(
            "Swipe up".to_string(),
            Gray4TextStyle::new(FONT_SWIPE_UP, color),
        );

        let bottom_spacer = SizedBox::<Rgb565>::new(Size::new(1, 8));

        let column = Column::new((bobbing_carat, text, bottom_spacer));

        Self { column }
    }
}
