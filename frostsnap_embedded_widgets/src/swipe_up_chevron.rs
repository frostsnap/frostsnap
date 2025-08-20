use crate::super_draw_target::SuperDrawTarget;
use crate::{bobbing_carat::BobbingCarat, text::Text, Column, Widget, FONT_SMALL};
use embedded_graphics::prelude::*;
use u8g2_fonts::U8g2TextStyle;

pub struct SwipeUpChevron<C: crate::WidgetColor> {
    column: Column<(BobbingCarat<C>, Text<U8g2TextStyle<C>>)>,
    cached_size: Option<crate::Sizing>,
}

impl<C: crate::WidgetColor> SwipeUpChevron<C>
where
    C: Copy,
{
    pub fn new(color: C, background_color: C) -> Self {
        // Create bobbing carat
        let bobbing_carat = BobbingCarat::new(color, background_color);

        // Create text
        let text = Text::new("Swipe up", U8g2TextStyle::new(FONT_SMALL, color));

        // Create column with both widgets
        let column = Column::new((bobbing_carat, text));

        Self {
            column,
            cached_size: None,
        }
    }
}

impl<C: crate::WidgetColor> crate::DynWidget for SwipeUpChevron<C>
where
    C: Copy,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.column.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.column.sizing()
    }

    fn force_full_redraw(&mut self) {
        self.column.force_full_redraw();
    }
}

impl<C: crate::WidgetColor> Widget for SwipeUpChevron<C>
where
    C: Copy,
{
    type Color = C;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.column.draw(target, current_time)
    }
}
