use crate::{
    circle_button::{CircleButton, CircleButtonState},
    hold_to_confirm_border::HoldToConfirmBorder,
    palette::PALETTE,
    prelude::*,
    rat::Frac,
    Fader,
};
use alloc::boxed::Box;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

/// A widget that combines HoldToConfirmBorder with a hand gesture icon and transitions to a checkmark
pub struct HoldToConfirm<W>
where
    W: Widget<Color = Rgb565>,
{
    content: Box<HoldToConfirmBorder<Container<Center<Column<(W, Fader<CircleButton>)>>>, Rgb565>>,
    last_update: Option<crate::Instant>,
    hold_duration_ms: u32,
    completed: bool,
}

impl<W> HoldToConfirm<W>
where
    W: Widget<Color = Rgb565>,
{
    pub fn new(hold_duration_ms: u32, widget: W) -> Self {
        const BORDER_WIDTH: u32 = 5;

        // Create the circle button wrapped in a fader (starts visible by default)
        let button = CircleButton::new();
        let faded_button = Fader::new(button);

        // Create column with the widget (flex) and faded button
        let column = Column::builder()
            .push(widget)
            .flex(1)
            .push(faded_button)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween);

        // Center the column, then put it in an expanded container to fill available space
        let centered = Center::new(column);
        let content = Container::new(centered).with_expanded();

        // Create the border with the actual content inside
        let border = Box::new(HoldToConfirmBorder::new(
            content,
            BORDER_WIDTH,
            PALETTE.confirm_progress,
            PALETTE.background,
        ));

        Self {
            content: border,
            last_update: None,
            hold_duration_ms,
            completed: false,
        }
    }

    /// Builder method to start with the button faded out
    pub fn with_faded_out_button(mut self) -> Self {
        self.button_fader_mut().set_faded_out();
        self
    }

    /// Fade in the button
    pub fn fade_in_button(&mut self) {
        if self.button_fader_mut().is_faded_out() {
            self.button_fader_mut().start_fade_in(
                300, // 800ms fade duration
                20,  // 50ms redraw interval
            );
        }
    }

    pub fn button_mut(&mut self) -> &mut CircleButton {
        &mut self.content.child.child.child.children.1.child
    }

    pub fn button(&self) -> &CircleButton {
        &self.content.child.child.child.children.1.child
    }

    fn button_fader_mut(&mut self) -> &mut Fader<CircleButton> {
        &mut self.content.child.child.child.children.1
    }

    /// Get mutable access to the inner widget
    pub fn widget_mut(&mut self) -> &mut W {
        &mut self.content.child.child.child.children.0
    }

    /// Get access to the inner widget
    pub fn widget(&self) -> &W {
        &self.content.child.child.child.children.0
    }

    pub fn is_completed(&self) -> bool {
        self.button().state() == CircleButtonState::ShowingCheckmark
    }

    fn is_holding(&self) -> bool {
        self.button().state() == CircleButtonState::Pressed
    }

    fn update_progress(&mut self, current_time: crate::Instant) {
        let holding = self.is_holding();
        let current_progress = self.content.get_progress();

        // Early exit if not holding and no progress
        if !holding && current_progress == Frac::ZERO {
            self.last_update = None; // Clear last_update when fully released
            return;
        }

        if let Some(last_time) = self.last_update {
            let elapsed_ms = current_time.saturating_duration_since(last_time) as u32;

            if elapsed_ms == 0 {
                return;
            }

            if holding && !self.completed {
                let increment = Frac::from_ratio(elapsed_ms, self.hold_duration_ms);
                let new_progress = current_progress + increment;
                self.content.set_progress(new_progress);

                if new_progress >= Frac::ONE {
                    self.completed = true;

                    // Start fading out the border only
                    self.content.start_fade_out(500);
                    self.button_mut()
                        .set_state(CircleButtonState::ShowingCheckmark);
                }
            } else if !holding && current_progress > Frac::ZERO && !self.completed {
                let decrement = Frac::from_ratio(elapsed_ms, 1000);
                let new_progress = current_progress - decrement;
                self.content.set_progress(new_progress);

                // If we've fully released, clear last_update
                if new_progress == Frac::ZERO {
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

impl<W> crate::DynWidget for HoldToConfirm<W>
where
    W: Widget<Color = Rgb565>,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.content.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.content.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Handle touch on the border (which will pass it to content)
        self.content.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Only pass drag events if not holding the button
        if !self.is_holding() {
            self.content.handle_vertical_drag(prev_y, new_y, is_release);
        }
    }

    fn force_full_redraw(&mut self) {
        self.content.force_full_redraw();
    }
}

impl<W> Widget for HoldToConfirm<W>
where
    W: Widget<Color = Rgb565>,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if self.is_holding() || self.content.get_progress() > Frac::ZERO {
            self.update_progress(current_time);
        }

        // Draw the border (which includes the content)
        self.content.draw(target, current_time)?;

        if self.content.is_faded_out() && !self.button().checkmark().drawing_started() {
            self.button_mut().checkmark_mut().start_drawing()
        }

        Ok(())
    }
}
