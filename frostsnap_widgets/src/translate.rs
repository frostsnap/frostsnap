use crate::{
    animation_speed::AnimationSpeed, vec_framebuffer::VecFramebuffer, Frac, Instant, Widget,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::BinaryColor,
    primitives::Rectangle,
    Pixel,
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
    Idle { offset: Point },
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
    previous_bitmap: VecFramebuffer<BinaryColor>,
    /// Bitmap tracking current frame's pixels
    current_bitmap: VecFramebuffer<BinaryColor>,
    /// Cached constraints
    constraints: Option<Size>,
    /// Offset of the dirty rect within the child's full area
    dirty_rect_offset: Point,
    /// The child's dirty rect (cached from set_constraints)
    child_dirty_rect: Rectangle,
}

impl<W: Widget> Translate<W>
where
    W::Color: Copy,
{
    pub fn new(child: W, background_color: W::Color) -> Self {
        // We'll initialize bitmaps when we get constraints
        Self {
            previous_bitmap: VecFramebuffer::new(0, 0),
            current_bitmap: VecFramebuffer::new(0, 0),
            child,
            current_offset: Point::zero(),
            translation_direction: TranslationDirection::Idle {
                offset: Point::zero(),
            },
            repeat: false,
            animation_speed: AnimationSpeed::Linear,
            background_color,
            constraints: None,
            dirty_rect_offset: Point::zero(),
            child_dirty_rect: Rectangle::zero(),
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
        matches!(
            self.translation_direction,
            TranslationDirection::Idle { .. }
        )
    }

    /// Calculate the current offset based on translation direction
    fn calculate_offset(&mut self, current_time: Instant) -> Point {
        match self.translation_direction.clone() {
            TranslationDirection::Animating {
                offset,
                duration,
                start_time,
                from_offset,
            } => {
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
                        self.translation_direction = TranslationDirection::Idle {
                            offset: final_position,
                        };
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
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
        self.child.set_constraints(max_size);

        // Use the child's dirty_rect if available, otherwise fall back to full sizing
        let child_sizing = self.child.sizing();
        let dirty_rect = child_sizing.dirty_rect();
        let (buffer_size, offset) = (dirty_rect.size, dirty_rect.top_left);

        self.dirty_rect_offset = offset;
        self.child_dirty_rect = dirty_rect;
        self.previous_bitmap =
            VecFramebuffer::new(buffer_size.width as usize, buffer_size.height as usize);
        self.current_bitmap =
            VecFramebuffer::new(buffer_size.width as usize, buffer_size.height as usize);
    }

    fn sizing(&self) -> crate::Sizing {
        self.child.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Adjust touch point for current offset
        let adjusted_point = point - self.current_offset;
        self.child
            .handle_touch(adjusted_point, current_time, is_release)
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
        target: &mut crate::SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.constraints.unwrap();

        // Calculate the current offset (will initialize start time if needed)
        let offset = self.calculate_offset(current_time);

        // Handle offset change and bitmap tracking
        if offset != self.current_offset {
            self.child.force_full_redraw();

            // Clear current bitmap for reuse
            self.current_bitmap.clear(BinaryColor::Off);

            // Calculate offset difference
            let diff_offset = offset - self.current_offset;

            // Create a translated SuperDrawTarget for the animation offset only
            let translated_target = target.clone().translate(offset);

            // Wrap it in TranslatorDrawTarget for pixel tracking
            // The TranslatorDrawTarget will handle converting screen coords to bitmap coords
            let translator = TranslatorDrawTarget {
                inner: translated_target,
                current_bitmap: &mut self.current_bitmap,
                previous_bitmap: &mut self.previous_bitmap,
                diff_offset,
                dirty_rect_offset: self.dirty_rect_offset,
                dirty_rect: self.child_dirty_rect,
            };

            // Wrap the TranslatorDrawTarget in another SuperDrawTarget
            let mut outer_target = crate::SuperDrawTarget::new(translator, self.background_color);

            // Draw the child
            self.child.draw(&mut outer_target, current_time)?;

            // Clear any remaining pixels from the previous bitmap
            let dirty_rect_offset = self.dirty_rect_offset;
            let clear_pixels = self.previous_bitmap.on_pixels().map(|point| {
                // Translate bitmap coordinates to screen coordinates
                // First add the dirty_rect offset, then the current animation offset
                let screen_point = point + dirty_rect_offset + self.current_offset;
                Pixel(screen_point, self.background_color)
            });
            target.draw_iter(clear_pixels)?;

            // Swap bitmaps
            core::mem::swap(&mut self.previous_bitmap, &mut self.current_bitmap);
            self.current_offset = offset;
        } else {
            // No movement - just draw normally with animation offset
            let mut translated_target = target.clone().translate(offset);
            self.child.draw(&mut translated_target, current_time)?;
        }

        Ok(())
    }
}

/// A DrawTarget wrapper that tracks pixels for the translate animation
struct TranslatorDrawTarget<'a, D, C>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    inner: crate::SuperDrawTarget<D, C>,
    current_bitmap: &'a mut VecFramebuffer<BinaryColor>,
    previous_bitmap: &'a mut VecFramebuffer<BinaryColor>,
    diff_offset: Point,
    dirty_rect_offset: Point,
    dirty_rect: Rectangle,
}

impl<'a, D, C> DrawTarget for TranslatorDrawTarget<'a, D, C>
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
        let current_bitmap = &mut self.current_bitmap;
        let previous_bitmap = &mut self.previous_bitmap;
        let diff_offset = self.diff_offset;
        let dirty_rect_offset = self.dirty_rect_offset;
        let dirty_rect = self.dirty_rect;

        self.inner.draw_iter(
            pixels
                .into_iter()
                .filter(move |Pixel(point, _)| {
                    // Only draw pixels that are within the dirty_rect
                    dirty_rect.contains(*point)
                })
                .inspect(|Pixel(point, _color)| {
                    // Convert screen coordinates to bitmap coordinates
                    let bitmap_point = *point - dirty_rect_offset;

                    // Mark this pixel as drawn in the current bitmap
                    VecFramebuffer::<BinaryColor>::set_pixel(
                        current_bitmap,
                        bitmap_point,
                        BinaryColor::On,
                    );

                    // Clear this pixel from the previous bitmap (offset by diff_offset)
                    let prev_bitmap_point = bitmap_point + diff_offset;
                    VecFramebuffer::<BinaryColor>::set_pixel(
                        previous_bitmap,
                        prev_bitmap_point,
                        BinaryColor::Off,
                    );
                }),
        )
    }
}

impl<'a, D, C> Dimensions for TranslatorDrawTarget<'a, D, C>
where
    D: DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    fn bounding_box(&self) -> Rectangle {
        self.inner.bounding_box()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SizedBox;
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
