use crate::{Widget, Instant, Frac, bitmap::Bitmap, animation_speed::AnimationSpeed};
use embedded_graphics::{
    draw_target::DrawTarget, geometry::{Dimensions, Point, Size}, pixelcolor::BinaryColor, primitives::Rectangle, Pixel, prelude::*,
};

/// Translation direction for the translate widget
#[derive(Clone, PartialEq, Debug)]
enum TranslationDirection {
    /// Animating between rest and offset
    Animating {
        /// The offset point (either from or to depending on from_offset)
        offset: Point,
        /// Duration of the animation
        duration: u64,
        /// When the animation started
        start_time: Option<Instant>,
        /// If true, animating from offset to rest. If false, from rest to offset.
        from_offset: bool,
    },
    /// No animation - idle at a specific offset
    Idle {
        offset: Point,
    },
}

/// A widget that animates its child by translating it across the screen
#[derive(Clone, PartialEq)]
pub struct Translate<W: Widget> {
    pub child: W,
    /// Current offset from original position
    current_offset: Point,
    /// Current translation direction
    translation_direction: TranslationDirection,
    /// Whether to repeat the animation (reversing direction each time)
    repeat: bool,
    /// Animation speed curve
    animation_speed: AnimationSpeed,
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
            translation_direction: TranslationDirection::Idle { offset: Point::zero() },
            repeat: false,
            animation_speed: AnimationSpeed::Linear,
            background_color,
        }
    }
    
    /// Set the animation speed curve
    pub fn set_animation_speed(&mut self, speed: AnimationSpeed) {
        self.animation_speed = speed;
    }
    
    /// Animate from an offset to the rest position (entrance animation)
    pub fn animate_from(&mut self, from: Point, duration: u64) {
        self.translation_direction = TranslationDirection::Animating { 
            offset: from, 
            duration, 
            start_time: None,
            from_offset: true,
        };
    }
    
    /// Animate from rest position to an offset (exit animation)
    pub fn animate_to(&mut self, to: Point, duration: u64) {
        self.translation_direction = TranslationDirection::Animating { 
            offset: to, 
            duration, 
            start_time: None,
            from_offset: false,
        };
    }
    
    /// Legacy method for backwards compatibility
    pub fn translate(&mut self, movement: Point, duration: u64) {
        // Treat this as animating from current position by movement amount
        self.animate_to(movement, duration);
    }
    
    /// Enable or disable repeat mode (animation reverses direction each cycle)
    pub fn set_repeat(&mut self, repeat: bool) {
        self.repeat = repeat;
    }
    
    /// Check if animation is complete
    pub fn is_idle(&self) -> bool {
        matches!(self.translation_direction, TranslationDirection::Idle { .. })
    }
    
    /// Calculate the current offset based on translation direction
    fn calculate_offset(&mut self, current_time: Instant) -> Point {
        match self.translation_direction.clone() {
            TranslationDirection::Animating { offset, duration, start_time, from_offset } => {
                // Initialize start time if needed
                let start = start_time.unwrap_or(current_time);
                if start_time.is_none() {
                    self.translation_direction = TranslationDirection::Animating {
                        offset,
                        duration,
                        start_time: Some(current_time),
                        from_offset,
                    };
                }
                
                let elapsed_ms = current_time.saturating_duration_since(start) as u32;
                
                if elapsed_ms >= duration as u32 {
                    // Animation complete
                    let final_position = if from_offset {
                        Point::zero() // Ended at rest
                    } else {
                        offset // Ended at offset
                    };
                    
                    if self.repeat {
                        // Flip direction
                        self.translation_direction = TranslationDirection::Animating {
                            offset,
                            duration,
                            start_time: Some(current_time),
                            from_offset: !from_offset, // Flip the direction
                        };
                        final_position
                    } else {
                        // Stop at final position
                        self.translation_direction = TranslationDirection::Idle { offset: final_position };
                        final_position
                    }
                } else {
                    // Animation in progress
                    let linear_progress = Frac::from_ratio(elapsed_ms, duration as u32);
                    let progress = self.animation_speed.apply(linear_progress);
                    
                    if from_offset {
                        // Animating from offset to rest
                        offset * (Frac::ONE - progress)
                    } else {
                        // Animating from rest to offset
                        offset * progress
                    }
                }
            }
            TranslationDirection::Idle { offset } => offset,
        }
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
        // Calculate the current offset (will initialize start time if needed)
        let offset = self.calculate_offset(current_time);

        // Handle offset change and bitmap tracking
        if offset != self.current_offset {
            self.child.force_full_redraw();
            
            // Clear current bitmap for reuse
            self.current_bitmap.clear();
            
            // Calculate offset difference
            let diff_offset = offset - self.current_offset;
            
            // Draw the child using the TranslatorDrawTarget with custom bounding box
            let mut translated = target.translated(offset);
            let mut translator_target = TranslatorDrawTarget {
                inner: &mut translated,
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
            self.child.draw(&mut target.translated(offset), current_time)?;
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
        
        // Start animation from offset
        translate.animate_from(Point::new(10, 0), 1000);
        
        // After calling animate_from, no longer idle
        assert!(!translate.is_idle());
    }
    
    #[test]
    fn test_animate_from_and_to() {
        let widget = SizedBox::<Rgb565>::new(Size::new(10, 10));
        let mut translate = Translate::new(widget, Rgb565::BLACK);
        
        // Test animate_from
        translate.animate_from(Point::new(0, 100), 1000);
        assert!(!translate.is_idle());
        
        // Test animate_to
        translate.animate_to(Point::new(0, -100), 1000);
        assert!(!translate.is_idle());
    }
}
