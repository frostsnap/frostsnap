use crate::{Widget, Instant, Fader};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

/// State of the fade switcher
#[derive(Debug, Clone, Copy, PartialEq)]
enum FadeState {
    /// Showing left widget
    ShowingLeft,
    /// Fading out left widget
    FadingOutLeft,
    /// Fading in right widget
    FadingInRight,
    /// Showing right widget
    ShowingRight,
    /// Fading out right widget
    FadingOutRight,
    /// Fading in left widget
    FadingInLeft,
}

/// A widget that smoothly fades between two child widgets
pub struct FadeSwitcher<L, R> 
where
    L: Widget<Color = Rgb565>,
    R: Widget<Color = Rgb565>,
{
    left_fader: Fader<L>,
    right_fader: Fader<R>,
    state: FadeState,
    fade_duration_ms: u32,
    bg_color: Rgb565,
}

impl<L, R> FadeSwitcher<L, R>
where
    L: Widget<Color = Rgb565>,
    R: Widget<Color = Rgb565>,
{
    /// Create a new FadeSwitcher showing the left widget by default
    pub fn new(left: L, right: R, fade_duration_ms: u32, bg_color: Rgb565) -> Self {
        let left_fader = Fader::new(left);
        let right_fader = Fader::new_faded_out(right);
        
        Self {
            left_fader,
            right_fader,
            state: FadeState::ShowingLeft,
            fade_duration_ms,
            bg_color,
        }
    }
    
    /// Switch to show the right widget with fade transition
    pub fn switch_to_right(&mut self) {
        match self.state {
            FadeState::ShowingLeft | FadeState::FadingInLeft => {
                // Start fading out left
                self.left_fader.start_fade(self.fade_duration_ms as u64, 16, self.bg_color);
                self.state = FadeState::FadingOutLeft;
            }
            _ => {} // Already showing or transitioning to right
        }
    }
    
    /// Switch to show the left widget with fade transition
    pub fn switch_to_left(&mut self) {
        match self.state {
            FadeState::ShowingRight | FadeState::FadingInRight => {
                // Start fading out right
                self.right_fader.start_fade(self.fade_duration_ms as u64, 16, self.bg_color);
                self.state = FadeState::FadingOutRight;
            }
            _ => {} // Already showing or transitioning to left
        }
    }
    
    /// Check if currently showing left widget (not transitioning)
    pub fn is_showing_left(&self) -> bool {
        self.state == FadeState::ShowingLeft
    }
    
    /// Check if currently showing right widget (not transitioning)
    pub fn is_showing_right(&self) -> bool {
        self.state == FadeState::ShowingRight
    }
    
    /// Check if currently transitioning between widgets
    pub fn is_transitioning(&self) -> bool {
        !matches!(self.state, FadeState::ShowingLeft | FadeState::ShowingRight)
    }
    
    /// Update the fade state based on current fader states
    fn update_state(&mut self) {
        match self.state {
            FadeState::FadingOutLeft => {
                if self.left_fader.is_fade_complete() {
                    // Left fade out complete, start fading in right
                    self.right_fader.start_fade_in(self.fade_duration_ms as u64, 16, self.bg_color);
                    self.state = FadeState::FadingInRight;
                }
            }
            FadeState::FadingInRight => {
                if self.right_fader.is_fade_complete() {
                    // Right fade in complete
                    self.state = FadeState::ShowingRight;
                }
            }
            FadeState::FadingOutRight => {
                if self.right_fader.is_fade_complete() {
                    // Right fade out complete, start fading in left
                    self.left_fader.start_fade_in(self.fade_duration_ms as u64, 16, self.bg_color);
                    self.state = FadeState::FadingInLeft;
                }
            }
            FadeState::FadingInLeft => {
                if self.left_fader.is_fade_complete() {
                    // Left fade in complete
                    self.state = FadeState::ShowingLeft;
                }
            }
            _ => {} // Stable states, nothing to update
        }
    }
}

impl<L, R> Widget for FadeSwitcher<L, R>
where
    L: Widget<Color = Rgb565>,
    R: Widget<Color = Rgb565>,
{
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Update state based on fade progress
        self.update_state();
        
        // Draw the appropriate widget(s) based on state
        match self.state {
            FadeState::ShowingLeft => {
                self.left_fader.draw(target, current_time)
            }
            FadeState::ShowingRight => {
                self.right_fader.draw(target, current_time)
            }
            FadeState::FadingOutLeft | FadeState::FadingInLeft => {
                // Only draw left during its transitions
                self.left_fader.draw(target, current_time)
            }
            FadeState::FadingOutRight | FadeState::FadingInRight => {
                // Only draw right during its transitions
                self.right_fader.draw(target, current_time)
            }
        }
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Only handle touch for the active widget
        match self.state {
            FadeState::ShowingLeft | FadeState::FadingOutLeft | FadeState::FadingInLeft => {
                self.left_fader.handle_touch(point, current_time, is_release)
            }
            FadeState::ShowingRight | FadeState::FadingOutRight | FadeState::FadingInRight => {
                self.right_fader.handle_touch(point, current_time, is_release)
            }
        }
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Only handle drag for the active widget
        match self.state {
            FadeState::ShowingLeft | FadeState::FadingOutLeft | FadeState::FadingInLeft => {
                self.left_fader.handle_vertical_drag(prev_y, new_y, is_release)
            }
            FadeState::ShowingRight | FadeState::FadingOutRight | FadeState::FadingInRight => {
                self.right_fader.handle_vertical_drag(prev_y, new_y, is_release)
            }
        }
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Return the size hint from the currently active widget
        match self.state {
            FadeState::ShowingLeft | FadeState::FadingOutLeft | FadeState::FadingInLeft => {
                self.left_fader.size_hint()
            }
            FadeState::ShowingRight | FadeState::FadingOutRight | FadeState::FadingInRight => {
                self.right_fader.size_hint()
            }
        }
    }
    
    fn force_full_redraw(&mut self) {
        self.left_fader.force_full_redraw();
        self.right_fader.force_full_redraw();
    }
}