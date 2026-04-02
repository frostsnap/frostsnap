use crate::{
    circle_button::{CircleButton, CircleButtonState},
    frame_cache::FrameCache,
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

pub struct HoldToConfirmColors {
    pub border: Rgb565,
    pub button_fill: Rgb565,
    pub button_stroke: Rgb565,
    pub checkmark: Rgb565,
}

impl Default for HoldToConfirmColors {
    fn default() -> Self {
        Self {
            border: PALETTE.confirm_progress,
            button_fill: PALETTE.tertiary_container,
            button_stroke: PALETTE.confirm_progress,
            checkmark: PALETTE.on_tertiary_container,
        }
    }
}

#[derive(Clone)]
pub struct HoldToConfirm<W>
where
    W: Widget<Color = Rgb565>,
{
    content: Box<
        HoldToConfirmBorder<
            Container<Center<Column<(W, Fader<FrameCache<CircleButton>>, SizedBox<Rgb565>)>>>,
            Rgb565,
        >,
    >,
    last_update: Option<crate::Instant>,
    hold_duration_ms: u32,
    completed: bool,
    completed_at: Option<crate::Instant>,
    finished: bool,
    dwell_ms: u64,
}

impl<W> HoldToConfirm<W>
where
    W: Widget<Color = Rgb565>,
{
    pub fn new(hold_duration_ms: u32, widget: W) -> Self {
        const BORDER_WIDTH: u32 = 5;

        let button = CircleButton::new();
        let cached_button = FrameCache::new(button);
        let faded_button = Fader::new(cached_button);

        // Create a 10px spacer beneath the button
        let bottom_spacer = SizedBox::<Rgb565>::new(Size::new(1, 10));

        // Create column with the widget (flex), faded button, and spacer
        let column = Column::builder()
            .push(widget)
            .flex(1)
            .push(faded_button)
            .push(bottom_spacer)
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween);

        // Center the column, then put it in an expanded container to fill available space
        let centered = Center::new(column);
        let content = Container::new(centered).with_expanded();

        // Create the border with the actual content inside
        let border = Box::new(HoldToConfirmBorder::new(
            content,
            BORDER_WIDTH,
            PALETTE.confirm_progress,
        ));

        Self {
            content: border,
            last_update: None,
            hold_duration_ms,
            completed: false,
            completed_at: None,
            finished: false,
            dwell_ms: 2000,
        }
    }

    /// Builder method to start with the button faded out
    pub fn with_faded_out_button(mut self) -> Self {
        self.button_fader_mut().set_faded_out();
        self
    }

    pub fn with_colors(mut self, colors: HoldToConfirmColors) -> Self {
        self.content.set_border_color(colors.border);
        self.button_mut().set_pressed_colors(
            colors.button_fill,
            colors.button_stroke,
            colors.checkmark,
        );
        self
    }

    /// Fade in the button
    pub fn fade_in_button(&mut self) {
        if self.button_fader_mut().is_faded_out() {
            self.button_fader_mut().start_fade_in(300);
        }
    }

    pub fn button_mut(&mut self) -> &mut CircleButton {
        self.content.child.child.child.children.1.child.child_mut()
    }

    pub fn button(&self) -> &CircleButton {
        self.content.child.child.child.children.1.child.child()
    }

    fn button_fader_mut(&mut self) -> &mut Fader<FrameCache<CircleButton>> {
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

    pub fn is_confirmed(&self) -> bool {
        self.button().state() == CircleButtonState::ShowingCheckmark
    }

    pub fn is_finished(&self) -> bool {
        self.finished
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
                    // 🎬 don't start fade/checkmark yet — let the border
                    // draw one more frame at progress=1.0 before the button
                    // state change triggers a large SPI blit
                    self.completed = true;
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

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

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

        if self.completed && !self.content.is_fading() {
            self.content.start_fade_out(500);
            self.button_mut()
                .set_state(CircleButtonState::ShowingCheckmark);
        }

        if self.content.is_faded_out() && !self.button().checkmark().drawing_started() {
            self.button_mut().checkmark_mut().start_drawing()
        }

        if self.completed_at.is_none() && self.button().checkmark().is_complete() {
            self.completed_at = Some(current_time);
        }

        if let Some(at) = self.completed_at {
            if current_time.saturating_duration_since(at) >= self.dwell_ms {
                self.finished = true;
            }
        }

        Ok(())
    }
}
