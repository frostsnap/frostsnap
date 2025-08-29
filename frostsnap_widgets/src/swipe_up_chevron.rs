use crate::{bobbing_carat::BobbingCarat, text::Text, Column, FONT_SMALL};
use frostsnap_macros::Widget;
use u8g2_fonts::U8g2TextStyle;

#[derive(Widget)]
pub struct SwipeUpChevron<C: crate::WidgetColor> {
    column: Column<(BobbingCarat<C>, Text<U8g2TextStyle<C>>)>,
}

impl<C: crate::WidgetColor> SwipeUpChevron<C>
where
    C: Copy,
{
    pub fn new(color: C, background_color: C) -> Self {
        let bobbing_carat = BobbingCarat::new(color, background_color);
        let text = Text::new("Swipe up", U8g2TextStyle::new(FONT_SMALL, color));
        let column = Column::new((bobbing_carat, text));
        Self { column }
    }
}
