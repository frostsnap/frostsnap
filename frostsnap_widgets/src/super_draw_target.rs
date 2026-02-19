use crate::Frac;
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
    /// Optional clip rectangle in display coordinates. When set, pixels outside
    /// this rectangle are discarded. Unlike `crop`, this does not affect translation.
    clip_rect: Option<Rectangle>,
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
            clip_rect: None,
            opacity: Frac::ONE,
            background_color,
        }
    }

    pub fn from_shared(display: Rc<RefCell<D>>, background_color: C) -> Self {
        let crop_area = display.borrow().bounding_box();
        Self {
            display,
            crop_area,
            clip_rect: None,
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

    /// Restrict drawing to the given area (in local coordinates) without
    /// changing the coordinate translation. Pixels outside this area will be
    /// discarded.
    pub fn clip(mut self, area: Rectangle) -> Self {
        // Convert local coordinates to display coordinates
        let mut display_area = area;
        display_area.top_left += self.crop_area.top_left;
        self.clip_rect = Some(match self.clip_rect {
            Some(existing) => rect_intersection(existing, display_area),
            None => display_area,
        });
        self
    }

    pub fn opacity(mut self, opacity: Frac) -> Self {
        // Multiply opacities to correctly handle nested transparency.
        // If a parent widget has 0.5 opacity and a child has 0.5 opacity,
        // the child should appear at 0.25 opacity (0.5 * 0.5), not 0.5.
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

    pub fn with_background_color(mut self, new_background_color: C) -> Self {
        // Interpolate the new background toward the existing background based on opacity.
        // This ensures child widgets (e.g., Text with anti-aliased fonts) blend against
        // the correct effective background color after opacity is applied.
        // Without this, text would blend against the raw background color, but pixels
        // would then be faded toward a different background, causing visible artifacts.
        self.background_color = self
            .background_color
            .interpolate(new_background_color, self.opacity);
        self
    }

    /// Clear an area with the background color
    pub fn clear_area(&mut self, area: &Rectangle) -> Result<(), D::Error> {
        self.fill_solid(area, self.background_color)
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
            clip_rect: self.clip_rect,
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
        let clip = self.clip_rect;

        if self.opacity < Frac::ONE {
            // Cache with invalidation based on source color
            let mut cache: Option<(C, C)> = None; // (source_color, interpolated_color)

            let pixels = pixels.into_iter().filter_map(|Pixel(point, color)| {
                let translated_point = point + crop.top_left;

                if let Some(clip) = clip {
                    if !clip.contains(translated_point) {
                        return None;
                    }
                }

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

                Some(Pixel(translated_point, final_color))
            });
            display.draw_iter(pixels)
        } else {
            // Just translate points
            let pixels = pixels.into_iter().filter_map(|Pixel(point, color)| {
                let translated_point = point + crop.top_left;
                if let Some(clip) = clip {
                    if !clip.contains(translated_point) {
                        return None;
                    }
                }
                Some(Pixel(translated_point, color))
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

        if let Some(clip) = self.clip_rect {
            translated_area = rect_intersection(translated_area, clip);
            if translated_area.size.width == 0 || translated_area.size.height == 0 {
                return Ok(());
            }
        }

        display.fill_solid(&translated_area, final_color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let mut display = self.display.borrow_mut();

        let final_color = if self.opacity < Frac::ONE {
            self.background_color.interpolate(color, self.opacity)
        } else {
            color
        };

        let mut area = self.crop_area;
        if let Some(clip) = self.clip_rect {
            area = rect_intersection(area, clip);
            if area.size.width == 0 || area.size.height == 0 {
                return Ok(());
            }
        }

        // When clearing with a crop, we fill the crop area
        display.fill_solid(&area, final_color)
    }
}

/// Compute the intersection of two rectangles. Returns a zero-size rectangle if
/// they don't overlap.
fn rect_intersection(a: Rectangle, b: Rectangle) -> Rectangle {
    let left = a.top_left.x.max(b.top_left.x);
    let top = a.top_left.y.max(b.top_left.y);
    let right = (a.top_left.x + a.size.width as i32).min(b.top_left.x + b.size.width as i32);
    let bottom = (a.top_left.y + a.size.height as i32).min(b.top_left.y + b.size.height as i32);

    if right <= left || bottom <= top {
        Rectangle::new(Point::new(left, top), Size::zero())
    } else {
        Rectangle::new(
            Point::new(left, top),
            Size::new((right - left) as u32, (bottom - top) as u32),
        )
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
