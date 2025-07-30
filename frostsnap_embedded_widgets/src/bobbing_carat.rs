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
        translate.translate(Point::new(0, -5), 500); // 500ms up, 500ms down
        
        Self { translate }
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
    
    fn size_hint(&self) -> Option<Size> {
        // Get the base icon size from translate widget
        if let Some(base_size) = self.translate.size_hint() {
            // Account for animation height - icon moves up by 5 pixels
            Some(Size::new(base_size.width, base_size.height + 5))
        } else {
            None
        }
    }
    
    fn force_full_redraw(&mut self) {
        self.translate.force_full_redraw();
    }
}
