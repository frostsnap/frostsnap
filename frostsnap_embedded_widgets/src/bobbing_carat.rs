use crate::{Widget, Instant, translate::Translate, image::Image};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Size, Point},
    pixelcolor::{PixelColor, RgbColor},
};
use embedded_iconoir::{size24px::navigation::NavArrowUp, prelude::IconoirNewIcon};

/// A carat icon that bobs up and down
pub struct BobbingCarat<C: PixelColor> {
    translate: Translate<Image<embedded_iconoir::Icon<C, NavArrowUp>>>,
}

impl<C: PixelColor + RgbColor> BobbingCarat<C> 
where
    C: Copy + Default,
{
    pub fn new(color: C, background_color: C) -> Self {
        let icon = NavArrowUp::new(color);
        let image_widget = Image::new(icon);
        let mut translate = Translate::new(image_widget, background_color);
        
        // Set up bobbing animation - move up and down by 5 pixels over 1 second
        translate.set_repeat(true);
        translate.animate_to(Point::new(0, 5), 500); // 500ms down, 500ms back up
        
        Self { translate }
    }
}

impl<C: PixelColor> crate::DynWidget for BobbingCarat<C>
where
    C: Copy,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.translate.set_constraints(max_size);
    }
    
    fn sizing(&self) -> crate::Sizing {
        self.translate.sizing()
    }
    

    fn force_full_redraw(&mut self) {
        self.translate.force_full_redraw();
    }
}

impl<C: PixelColor> Widget for BobbingCarat<C>
where
    C: Copy,
{
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.translate.draw(target, current_time)
    }
    
}
