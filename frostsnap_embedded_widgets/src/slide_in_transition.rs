use crate::{DynWidget, Widget, Instant, translate::Translate, fader::Fader, animation_speed::AnimationSpeed};
use core::mem;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

/// A transition widget that slides in a new widget while fading in
/// The previous widget is efficiently cleared by Translate's pixel tracking
/// Currently only supports Rgb565 due to Fader limitations
pub struct SlideInTransition<T: Widget<Color = Rgb565>> {
    current: Option<Fader<Translate<T>>>,
    old: Option<Fader<Translate<T>>>,  // Keep old widget for one frame to fade out
    transition_duration_ms: u64,
    slide_from_position: Point,
    bg_color: Rgb565,
}

impl<T: Widget<Color = Rgb565>> SlideInTransition<T> {
    /// Create a new slide-in transition
    /// - initial: The initial widget to display
    /// - transition_duration_ms: How long the transition takes
    /// - slide_from_position: Where the widget slides in FROM (e.g., Point::new(0, 100) to slide up from bottom)
    /// - bg_color: Background color to use when clearing previous widget
    pub fn new(
        initial: T,
        transition_duration_ms: u64,
        slide_from_position: Point,
        bg_color: Rgb565,
    ) -> Self {
        let mut self_ = Self {
            current: None,
            old: None,
            transition_duration_ms,
            slide_from_position,
            bg_color,
        };
        self_.switch_to(initial);
        self_
    }
    
    /// Set the slide-from position for the next transition
    pub fn set_slide_from(&mut self, position: Point) {
        self.slide_from_position = position;
    }
    
    /// Switch to a new widget with slide-in transition
    pub fn switch_to(&mut self, widget: T) {
        // Create translate widget and start the slide animation from the offset
        let mut new_translate = Translate::new(widget, self.bg_color);
        new_translate.set_animation_speed(AnimationSpeed::EaseOut);
        new_translate.animate_from(self.slide_from_position, self.transition_duration_ms);
        
        // Create new fader with fade starting
        let mut new_fader = Fader::new_faded_out(new_translate);
        new_fader.set_animation_speed(AnimationSpeed::EaseOut);

        // Use mem::replace to swap in the new widget and get the old one
        if let Some(old) = self.current.as_mut() {
            let mut old_fader = mem::replace(old, new_fader);
            // we don't want to write over self.old unless the current one has actually been drawn
            if old_fader.is_visible() {
                old_fader.instant_fade(self.bg_color);
                self.old = Some(old_fader);
            }
        } else {
            self.current = Some(new_fader);
        }
    }
}

impl<T: Widget<Color = Rgb565>> DynWidget for SlideInTransition<T> {
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.current.as_mut().and_then(|w| w.handle_touch(point, current_time, is_release))
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if let Some(ref mut current) = self.current {
            current.handle_vertical_drag(prev_y, new_y, is_release);
        }
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.current.as_ref().and_then(|c| c.size_hint())
    }
    
    fn force_full_redraw(&mut self) {
        if let Some(ref mut current) = self.current {
            current.force_full_redraw();
        }
    }
}

impl<T: Widget<Color = Rgb565>> Widget for SlideInTransition<T> {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Draw old widget once to let it fade out (clear pixels)
        if let Some(ref mut old) = self.old {
            old.draw(target, current_time)?;
            if old.is_faded_out() {
                self.old = None;
            } else {
                // this should never happen but just in case
                return Ok(());
            }
        }

        if let Some(ref mut current) = self.current  {
            if current.is_faded_out() {
                current.start_fade_in(self.transition_duration_ms, 10, self.bg_color);
            }

            current.draw(target, current_time)?;
        }

        Ok(())
    }
}
