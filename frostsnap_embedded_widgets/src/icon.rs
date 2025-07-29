use crate::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::Image,
    pixelcolor::PixelColor,
    Drawable,
};
use embedded_iconoir::prelude::IconoirNewIcon;

/// A widget that displays an icon from embedded_iconoir
pub struct Icon<I, C> 
where
    I: IconoirNewIcon<C>,
    C: PixelColor,
{
    icon: I,
    needs_redraw: bool,
    _phantom: core::marker::PhantomData<C>,
}

impl<I, C> Icon<I, C>
where
    I: IconoirNewIcon<C>,
    C: PixelColor,
{
    pub fn new(icon: I) -> Self {
        Self {
            icon,
            needs_redraw: true,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<I, C> Widget for Icon<I, C>
where
    I: IconoirNewIcon<C> + embedded_graphics::image::ImageDrawable<Color = C>,
    C: PixelColor,
{
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if !self.needs_redraw {
            return Ok(());
        }
        
        // Draw icon at origin (0, 0)
        Image::new(&self.icon, Point::zero()).draw(target)?;
        
        self.needs_redraw = false;
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(self.icon.size())
    }
    
    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}