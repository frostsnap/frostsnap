use crate::{
    circle_button::{CircleButton, CircleButtonState},
    column::{Column, MainAxisAlignment},
    fade_switcher::FadeSwitcher,
    fader::Fader,
    hold_to_confirm_border::HoldToConfirmBorder,
    padding::Padding,
    palette::PALETTE,
    Widget,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

/// A widget that combines HoldToConfirmBorder with a hand gesture icon and transitions to a checkmark
pub struct HoldToConfirm<P, S> 
where
    P: Widget<Color = Rgb565>,
    S: Widget<Color = Rgb565>,
{
    content: Padding<Column<(FadeSwitcher<P, S>, CircleButton), Rgb565>>,
    border: Fader<HoldToConfirmBorder<crate::SizedBox<Rgb565>, Rgb565>>,
    size: Size,
    last_update: Option<crate::Instant>,
    hold_duration_ms: f32,
    completed: bool,
}

impl<P, S> HoldToConfirm<P, S>
where
    P: Widget<Color = Rgb565>,
    S: Widget<Color = Rgb565>,
{
    pub fn new(size: Size, hold_duration_ms: f32, prompt_widget: P, success_widget: S) -> Self {
        const BORDER_WIDTH: u32 = 10;
        
        // Create the FadeSwitcher widget starting with prompt
        let fade_switcher = FadeSwitcher::new(
            prompt_widget,
            success_widget,
            300, // 300ms fade duration
            PALETTE.background
        );
        
        // Create the circle button
        let button = CircleButton::new();
        
        // Create column with the FadeSwitcher and button
        let column = Column::new((fade_switcher, button))
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween);
        
        // Add padding around the column
        let content = Padding::all(BORDER_WIDTH + 5, column);
        
        // Create the border separately with a SizedBox
        let sized_box = crate::SizedBox::new(size);
        let border_color = Rgb565::new(2, 46, 16); // Dark green border
        let border_holder = HoldToConfirmBorder::new(sized_box, BORDER_WIDTH, border_color, PALETTE.background);
        let border = Fader::new(border_holder);

        Self {
            content,
            border,
            size,
            last_update: None,
            hold_duration_ms,
            completed: false,
        }
    }

    pub fn enable(&mut self) {
    }

    pub fn reset(&mut self) {
        // Reset border progress
        self.border.child.set_progress(0.0);
        
        // Reset the FadeSwitcher to show prompt
        self.content.child.children.0.switch_to_left();
        
        // Reset the button (second element in tuple)
        self.content.child.children.1.reset();
        
        // Reset state
        self.last_update = None;
        self.completed = false;
    }

    pub fn is_completed(&self) -> bool {
        self.content.child.children.1.is_checkmark_complete()
    }
    
    fn is_holding(&self) -> bool {
        self.content.child.children.1.state() == CircleButtonState::Pressed
    }

    fn update_progress(&mut self, current_time: crate::Instant) {
        let holding = self.is_holding();
        let current_progress = self.border.child.get_progress();
        
        // Early exit if not holding and no progress
        if !holding && current_progress == 0.0 {
            self.last_update = None;  // Clear last_update when fully released
            return;
        }

        if let Some(last_time) = self.last_update {
            let elapsed_ms = current_time.saturating_duration_since(last_time) as f32;

            if elapsed_ms == 0.0 {
                return;
            }

            if holding && !self.completed {
                let increment = elapsed_ms / self.hold_duration_ms;
                let new_progress = (current_progress + increment).min(1.0);
                self.border.child.set_progress(new_progress);

                if new_progress >= 1.0 {
                    self.completed = true;
                    
                    // Switch FadeSwitcher to show success widget
                    self.content.child.children.0.switch_to_right();
                    
                    // Update circle button state
                    self.content.child.children.1.set_state(CircleButtonState::ShowingCheckmark);
                    
                    // Start fading out the border
                    self.border.start_fade(500, 100, PALETTE.background);
                }
            } else if !holding && current_progress > 0.0 && !self.completed {
                let decrement = elapsed_ms / 1000.0;
                let new_progress = (current_progress - decrement).max(0.0);
                self.border.child.set_progress(new_progress);
                
                // If we've fully released, clear last_update
                if new_progress == 0.0 {
                    self.last_update = None;
                    return;
                }
            }
            
            self.last_update = Some(current_time);
        } else if holding {
            // First frame of holding - just set the time, don't update progress yet
            self.last_update = Some(current_time);
        }
    }
}

impl<P, S> Widget for HoldToConfirm<P, S>
where
    P: Widget<Color = Rgb565>,
    S: Widget<Color = Rgb565>,
{
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.is_holding() || self.border.child.get_progress() > 0.0 {
            self.update_progress(current_time);
        }
        
        // Draw the border first
        self.border.draw(target, current_time)?;
        
        // Then draw the content on top
        self.content.draw(target, current_time)
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Handle touch on the content
        self.content.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }

    fn force_full_redraw(&mut self) {
        self.border.force_full_redraw();
        self.content.force_full_redraw();
    }
}
