use crate::{widget_color::ColorInterpolate, Frac};
use alloc::rc::Rc;
use core::cell::{RefCell, RefMut};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

pub struct SuperDrawTarget<D, C = <D as DrawTarget>::Color>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    display: Rc<RefCell<D>>,
    crop_area: Rectangle,
    opacity: Frac,
    background_color: C,
}

impl<D, C> SuperDrawTarget<D, C>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    pub fn new(display: D, background_color: C) -> Self {
        let crop_area = display.bounding_box();
        Self {
            display: Rc::new(RefCell::new(display)),
            crop_area,
            opacity: Frac::ONE,
            background_color,
        }
    }

    pub fn from_shared(display: Rc<RefCell<D>>, background_color: C) -> Self {
        let crop_area = display.borrow().bounding_box();
        Self {
            display,
            crop_area,
            opacity: Frac::ONE,
            background_color,
        }
    }

    pub fn crop(mut self, area: Rectangle) -> Self {
        // When applying a crop, we translate the area relative to existing crop
        let mut translated = area;
        translated.top_left += self.crop_area.top_left;
        self.crop_area = translated;
        self
    }

    pub fn opacity(mut self, opacity: Frac) -> Self {
        self.opacity = self.opacity * opacity;
        self
    }

    pub fn translate(mut self, offset: Point) -> Self {
        self.crop_area.top_left += offset;
        self
    }

    pub fn inner_mut(&mut self) -> Option<RefMut<'_, D>> {
        // Only return mutable reference if we're the only holder
        if Rc::strong_count(&self.display) == 1 {
            Some(self.display.borrow_mut())
        } else {
            None
        }
    }

    pub fn background_color(&self) -> C {
        self.background_color
    }
}

impl<D, C> Clone for SuperDrawTarget<D, C>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    fn clone(&self) -> Self {
        Self {
            display: Rc::clone(&self.display),
            crop_area: self.crop_area,
            opacity: self.opacity,
            background_color: self.background_color,
        }
    }
}

impl<D, C> DrawTarget for SuperDrawTarget<D, C>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    type Color = C;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let mut display = self.display.borrow_mut();
        let crop = self.crop_area;

        if self.opacity < Frac::ONE {
            // Cache with invalidation based on source color
            let mut cache: Option<(C, C)> = None; // (source_color, interpolated_color)

            let pixels = pixels.into_iter().map(|Pixel(point, color)| {
                let translated_point = point + crop.top_left;

                let final_color = match cache {
                    Some((cached_source, cached_result)) if cached_source == color => {
                        // Cache hit - same source color
                        cached_result
                    }
                    _ => {
                        // Cache miss or first calculation
                        let calculated = self.background_color.interpolate(color, self.opacity);
                        cache = Some((color, calculated));
                        calculated
                    }
                };

                Pixel(translated_point, final_color)
            });
            display.draw_iter(pixels)
        } else {
            // Just translate points
            let pixels = pixels.into_iter().map(|Pixel(point, color)| {
                let translated_point = point + crop.top_left;
                Pixel(translated_point, color)
            });
            display.draw_iter(pixels)
        }
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let mut display = self.display.borrow_mut();
        let mut translated_area = *area;
        translated_area.top_left += self.crop_area.top_left;

        if self.opacity < Frac::ONE {
            // Cache with invalidation based on source color
            let mut cache: Option<(C, C)> = None; // (source_color, interpolated_color)

            let colors = colors.into_iter().map(|color| {
                match cache {
                    Some((cached_source, cached_result)) if cached_source == color => {
                        // Cache hit - same source color
                        cached_result
                    }
                    _ => {
                        // Cache miss or first calculation
                        let calculated = self.background_color.interpolate(color, self.opacity);
                        cache = Some((color, calculated));
                        calculated
                    }
                }
            });
            display.fill_contiguous(&translated_area, colors)
        } else {
            display.fill_contiguous(&translated_area, colors)
        }
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let mut display = self.display.borrow_mut();

        let final_color = if self.opacity < Frac::ONE {
            self.background_color.interpolate(color, self.opacity)
        } else {
            color
        };

        let mut translated_area = *area;
        translated_area.top_left += self.crop_area.top_left;
        display.fill_solid(&translated_area, final_color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let mut display = self.display.borrow_mut();

        let final_color = if self.opacity < Frac::ONE {
            self.background_color.interpolate(color, self.opacity)
        } else {
            color
        };

        // When clearing with a crop, we fill the crop area
        display.fill_solid(&self.crop_area, final_color)
    }
}

impl<D, C> Dimensions for SuperDrawTarget<D, C>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    fn bounding_box(&self) -> Rectangle {
        // Return the crop area but with top_left at origin since that's what the widget sees
        Rectangle::new(Point::zero(), self.crop_area.size)
    }
}
