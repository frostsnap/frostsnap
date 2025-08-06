use crate::{Widget, Instant, Frac, bitmap::Bitmap};
use embedded_graphics::{
    draw_target::{DrawTarget, DrawTargetExt},
    geometry::{Point, Size, Dimensions},
    primitives::Rectangle,
    pixelcolor::BinaryColor,
    Pixel,
};


/// A widget that animates its child by translating it across the screen
#[derive(Clone, PartialEq)]
pub struct Translate<W: Widget> {
    child: W,
    /// Current offset from original position
    current_offset: Point,
    /// Total movement vector for current animation
    movement: Point,
    /// Duration of the animation in ms
    duration: u64,
    /// Start time of current animation (None if idle)
    start_time: Option<Instant>,
    /// Whether to repeat the animation (reversing direction each time)
    repeat: bool,
    /// Background color for erasing
    background_color: W::Color,
    /// Bitmap tracking previous frame's pixels
    previous_bitmap: Bitmap,
    /// Bitmap tracking current frame's pixels
    current_bitmap: Bitmap,
}

impl<W: Widget> Translate<W> 
where
    W::Color: Copy,
{
    pub fn new(child: W, background_color: W::Color) -> Self {
        let size = child.size_hint().expect("translated widgets must have size");
        Self {
            previous_bitmap: Bitmap::new(size, BinaryColor::Off),
            current_bitmap: Bitmap::new(size, BinaryColor::Off),
            child,
            current_offset: Point::zero(),
            movement: Point::zero(),
            duration: 0,
            start_time: None,
            repeat: false,
            background_color,
        }
    }
    
    /// Start a translation animation
    pub fn translate(&mut self, movement: Point, duration: u64) {
        // Store where we're starting from - animation will go from current_offset to current_offset + movement
        self.movement = movement;
        self.duration = duration;
        self.start_time = None; // Will be set on next draw
    }
    
    /// Enable or disable repeat mode (animation reverses direction each cycle)
    pub fn set_repeat(&mut self, repeat: bool) {
        self.repeat = repeat;
    }
    
    /// Reverse the current movement direction
    pub fn translate_reverse(&mut self) {
        self.translate(-self.movement, self.duration);
    }
    
    /// Check if animation is complete
    pub fn is_idle(&self) -> bool {
        self.start_time.is_none() && !self.repeat
    }
    
    /// Get the current movement vector
    pub fn current_movement(&self) -> Point {
        self.movement
    }
}

impl<W: Widget> crate::DynWidget for Translate<W>
where
    W::Color: Copy,

{
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Adjust touch point for current offset
        let adjusted_point = point - self.current_offset;
        self.child.handle_touch(adjusted_point, current_time, is_release)
    }

    fn size_hint(&self) -> Option<Size> {
        self.child.size_hint()
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
    }
}

impl<W: Widget> Widget for Translate<W> 
where
    W::Color: Copy,
{
    type Color = W::Color;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Initialize start time if needed
        if self.start_time.is_none() && self.movement != Point::zero() {
            self.start_time = Some(current_time);
        }
        
        // Calculate the current offset inline
        let offset = if let Some(start) = self.start_time {
            let elapsed_ms = current_time.duration_since(start).unwrap_or(0) as u32;
            
            if self.repeat {
                // For repeat mode, determine which cycle we're in
                let cycle = elapsed_ms / self.duration as u32;
                let cycle_ms = elapsed_ms % self.duration as u32;
                let frac = Frac::from_ratio(cycle_ms, self.duration as u32);
                
                // If odd cycle, reverse the animation
                if cycle % 2 == 1 {
                    self.movement * (Frac::ONE - frac)
                } else {
                    self.movement * frac
                }
            } else {
                // Single animation
                let frac = Frac::from_ratio(elapsed_ms, self.duration as u32);
                
                // Check if animation is complete
                if frac == Frac::ONE {
                    self.start_time = None;
                }
                
                self.movement * frac
            }
        } else {
            Point::zero()
        };
        
        // Handle offset change and bitmap tracking
        if offset != self.current_offset {
            self.child.force_full_redraw();
            
            // Clear current bitmap for reuse
            self.current_bitmap.clear();
            
            // Calculate offset difference
            let diff_offset = offset - self.current_offset;
            
            // Draw the child using the TranslatorDrawTarget
            let mut translated_target = target.translated(offset);
            let mut translator_target = TranslatorDrawTarget {
                inner: &mut translated_target,
                current_bitmap: &mut self.current_bitmap,
                previous_bitmap: &mut self.previous_bitmap,
                diff_offset,
            };
            self.child.draw(&mut translator_target, current_time)?;
            
            // Clear any remaining pixels from the previous bitmap
            let clear_pixels = self.previous_bitmap.on_pixels()
                                          .map(|point| {
                                              // Translate bitmap coordinates to screen coordinates
                                              let screen_point = point + self.current_offset;
                                              Pixel(screen_point, self.background_color)
                                          });
            target.draw_iter(clear_pixels)?;

            // Swap bitmaps
            core::mem::swap(&mut self.previous_bitmap, &mut self.current_bitmap);
            self.current_offset = offset;
        } else {
            // No movement - just draw normally
            let mut translated_target = target.translated(offset);
            self.child.draw(&mut translated_target, current_time)?;
        }
        
        Ok(())
    }
    
}

/// A DrawTarget wrapper that tracks pixels for the translate animation
struct TranslatorDrawTarget<'a, D> {
    inner: &'a mut D,
    current_bitmap: &'a mut Bitmap,
    previous_bitmap: &'a mut Bitmap,
    diff_offset: Point,
}

impl<'a, D> DrawTarget for TranslatorDrawTarget<'a, D>
where
    D: DrawTarget,
{
    type Color = D::Color;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let current_bitmap = &mut self.current_bitmap;
        let previous_bitmap = &mut self.previous_bitmap;
        let diff_offset = self.diff_offset;
        
        self.inner.draw_iter(pixels.into_iter().inspect(|Pixel(point, _color)| {
            // Mark this pixel as drawn in the current bitmap
            current_bitmap.set_pixel(point.x as u32, point.y as u32, BinaryColor::On);
            
            // Clear this pixel from the previous bitmap (offset by diff_offset)
            let prev_point = *point + diff_offset;
            if prev_point.x >= 0 && prev_point.y >= 0 {
                previous_bitmap.set_pixel(
                    prev_point.x as u32,
                    prev_point.y as u32,
                    BinaryColor::Off
                );
            }
        }))
    }
}

impl<'a, D> Dimensions for TranslatorDrawTarget<'a, D>
where
    D: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::sized_box::SizedBox;
    use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
    
    #[test]
    fn test_is_idle() {
        let widget = SizedBox::<Rgb565>::new(Size::new(10, 10));
        let mut translate = Translate::new(widget, Rgb565::BLACK);
        
        // Should be idle initially
        assert!(translate.is_idle());
        
        // Start animation
        translate.translate(Point::new(10, 0), 1000);
        
        // After calling translate, still idle until draw is called
        assert!(translate.is_idle());
    }
    
    #[test]
    fn test_translate_reverse() {
        let widget = SizedBox::<Rgb565>::new(Size::new(10, 10));
        let mut translate = Translate::new(widget, Rgb565::BLACK);
        
        translate.translate(Point::new(10, 5), 1000);
        let original_movement = translate.current_movement();
        
        translate.translate_reverse();
        let reversed_movement = translate.current_movement();
        
        assert_eq!(reversed_movement, -original_movement);
        assert_eq!(translate.duration, 1000);
    }
}
