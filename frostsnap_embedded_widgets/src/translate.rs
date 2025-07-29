use crate::{Widget, Instant, Rat};
use embedded_graphics::{
    draw_target::{DrawTarget, DrawTargetExt},
    geometry::{Point, Size, Dimensions},
    pixelcolor::PixelColor,
    primitives::Rectangle,
    Pixel,
};
use alloc::vec::Vec;

/// A compressed point that uses less memory
#[derive(Clone, Copy, PartialEq, Eq)]
struct CompressedPoint {
    x: i16,
    y: i16,
}

impl From<Point> for CompressedPoint {
    fn from(p: Point) -> Self {
        Self {
            x: p.x as i16,
            y: p.y as i16,
        }
    }
}

impl From<CompressedPoint> for Point {
    fn from(p: CompressedPoint) -> Self {
        Point::new(p.x as i32, p.y as i32)
    }
}

/// A widget that animates its child by translating it across the screen
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
    /// Previously drawn pixel positions
    previous_pixels: Vec<CompressedPoint>,
}

impl<W: Widget> Translate<W> 
where
    W::Color: Copy,
{
    pub fn new(child: W, background_color: W::Color) -> Self {
        Self {
            child,
            current_offset: Point::zero(),
            movement: Point::zero(),
            duration: 0,
            start_time: None,
            repeat: false,
            background_color,
            previous_pixels: Vec::new(),
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
                let rat = Rat::from_ratio(cycle_ms, self.duration as u32);
                
                // If odd cycle, reverse the animation
                if cycle % 2 == 1 {
                    self.movement * (Rat::ONE - rat)
                } else {
                    self.movement * rat
                }
            } else {
                // Single animation
                let rat = Rat::from_ratio(elapsed_ms, self.duration as u32).min(Rat::ONE);
                
                // Check if animation is complete
                if rat == Rat::ONE {
                    self.start_time = None;
                }
                
                self.movement * rat
            }
        } else {
            Point::zero()
        };
        
        // If offset changed, clear old pixels
        if offset != self.current_offset {
            self.child.force_full_redraw();
        } else if !self.previous_pixels.is_empty() {
            self.child.draw(&mut target.translated(offset), current_time)?;
            return Ok(());
        }

        let mut clear_target = target.translated(self.current_offset);
            let bg_pixels = self.previous_pixels.iter()
                .map(|&p| Pixel(p.into(), self.background_color));
            clear_target.draw_iter(bg_pixels)?;

        // Clear the recording buffer
        self.previous_pixels.clear();
        
        // Draw the child at the calculated offset, recording pixels
        let mut translated_target = target.translated(offset);
        let mut recording_target = RecordingTarget {
            inner: &mut translated_target,
            positions: &mut self.previous_pixels,
        };
        self.child.draw(&mut recording_target, current_time)?;
        
        // Update state for next frame
        self.current_offset = offset;
        

        Ok(())
    }
    
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

/// A DrawTarget that maps all colors to a single color
struct SingleColorTarget<'a, D, C> {
    inner: &'a mut D,
    color: C,
}

impl<'a, D, C> DrawTarget for SingleColorTarget<'a, D, C>
where
    D: DrawTarget<Color = C>,
    C: PixelColor,
{
    type Color = C;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.inner.draw_iter(
            pixels
                .into_iter()
                .map(|Pixel(point, _)| Pixel(point, self.color)),
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let count = colors.into_iter().count();
        let single_color = core::iter::repeat(self.color).take(count);
        self.inner.fill_contiguous(area, single_color)
    }

    fn fill_solid(&mut self, area: &Rectangle, _color: Self::Color) -> Result<(), Self::Error> {
        self.inner.fill_solid(area, self.color)
    }

    fn clear(&mut self, _color: Self::Color) -> Result<(), Self::Error> {
        self.inner.clear(self.color)
    }
}

impl<'a, D, C> Dimensions for SingleColorTarget<'a, D, C>
where
    D: DrawTarget,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}

/// A DrawTarget that records all pixel positions while drawing
struct RecordingTarget<'a, D> {
    inner: &'a mut D,
    positions: &'a mut Vec<CompressedPoint>,
}

impl<'a, D> DrawTarget for RecordingTarget<'a, D>
where
    D: DrawTarget,
{
    type Color = D::Color;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let pixels: Vec<_> = pixels.into_iter().collect();
        
        // Record positions
        for Pixel(point, _) in &pixels {
            self.positions.push((*point).into());
        }
        
        // Draw to inner target
        self.inner.draw_iter(pixels)
    }
}

impl<'a, D> Dimensions for RecordingTarget<'a, D>
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
