use crate::{Widget, FONT_SMALL, bobbing_carat::BobbingCarat, Column, text::Text};
use embedded_graphics::{
    pixelcolor::{PixelColor, RgbColor},
    prelude::*,
};
use u8g2_fonts::U8g2TextStyle;

pub struct SwipeUpChevron<C: PixelColor> {
    column: Column<(BobbingCarat<C>, Text<U8g2TextStyle<C>>)>,
    cached_size: Option<crate::Sizing>,
}

impl<C: PixelColor + RgbColor> SwipeUpChevron<C> 
where
    C: Copy + Default,
{
    pub fn new(color: C, background_color: C) -> Self {
        // Create bobbing carat
        let bobbing_carat = BobbingCarat::new(color, background_color);
        
        // Create text
        let text = Text::new(
            "Swipe up",
            U8g2TextStyle::new(FONT_SMALL, color),
        );
        
        // Create column with both widgets
        let column = Column::new((bobbing_carat, text));
        
        Self { 
            column,
            cached_size: None,
        }
    }
}

impl<C: PixelColor + RgbColor + Default> crate::DynWidget for SwipeUpChevron<C> 
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

impl<C: PixelColor + RgbColor + Default> Widget for SwipeUpChevron<C> 
where
    C: Copy,
{
    type Color = C;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }

}
