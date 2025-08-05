use crate::Widget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::ImageDrawable,
    pixelcolor::PixelColor,
    prelude::*,
};

/// A widget that wraps any ImageDrawable as a widget
#[derive(Clone)]
pub struct Image<I> {
    image: I,
    needs_redraw: bool,
}

impl<I> Image<I> {
    pub fn new(image: I) -> Self {
        Self {
            image,
            needs_redraw: true,
        }
    }
}

impl<I> crate::DynWidget for Image<I>
where
    I: ImageDrawable,
    I::Color: PixelColor,
{
    fn handle_touch(&mut self, _point: Point, _current_time: crate::Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}
    
    fn size_hint(&self) -> Option<Size> {
        self.image.bounding_box().size.into()
    }
    
    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl<I> Widget for Image<I>
where
    I: ImageDrawable,
    I::Color: PixelColor,
{
    type Color = I::Color;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if !self.needs_redraw {
            return Ok(());
        }
        
        // Draw image at origin (0, 0)
        embedded_graphics::image::Image::new(&self.image, Point::zero()).draw(target)?;
        
        self.needs_redraw = false;
        Ok(())
    }
}
