use crate::{
    checkmark::Checkmark, color_map::ColorMap, fader::Fader,
    hold_to_confirm_border::HoldToConfirmBorder, palette::PALETTE, sized_box::SizedBox, Widget,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::Image,
    pixelcolor::{BinaryColor, Rgb565, Rgb888},
    prelude::*,
    primitives::{Circle, PrimitiveStyleBuilder},
};
use embedded_iconoir::{icons::size48px::gestures::OpenSelectHandGesture, prelude::IconoirNewIcon};

// Circle dimensions - matching gradient_circle.rs
const CIRCLE_RADIUS: u32 = 50;
const CIRCLE_DIAMETER: u32 = CIRCLE_RADIUS * 2;

/// A widget that combines HoldToConfirmBorder with a hand gesture icon and transitions to a checkmark
pub struct HoldToConfirm {
    hold_to_confirm_border: Fader<ColorMap<HoldToConfirmBorder<SizedBox<BinaryColor>>, Rgb565>>,
    checkmark: ColorMap<Checkmark, Rgb565>,
    state: State,
    size: Size,
    icon_center: Point,
    last_drawn_state: Option<State>,
    holding: bool,
    last_drawn_holding: bool,
    progress: f32,
    last_update: Option<crate::Instant>,
    hold_duration_ms: f32,
    completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    WaitingForHold,
    ShowingCheckmark,
}

impl HoldToConfirm {
    pub fn new(size: Size, hold_duration_ms: f32) -> Self {
        let sized_box = SizedBox::<BinaryColor>::new(size);
        let hold_to_confirm_border_binary = HoldToConfirmBorder::new(sized_box);
        let hold_to_confirm_border_rgb =
            hold_to_confirm_border_binary.color_map(|color| match color {
                BinaryColor::On => Rgb565::new(2, 46, 16), // Dark green border
                BinaryColor::Off => PALETTE.background,
            });
        let hold_to_confirm_border = Fader::new(hold_to_confirm_border_rgb, PALETTE.background);

        let checkmark = Checkmark::new(Size::new(96, 96)) // Larger checkmark size
            .color_map(|color| match color {
                BinaryColor::On => Rgb565::WHITE,
                BinaryColor::Off => Rgb565::RED, // Doesn't matter, won't be visible
            });

        // Position icon towards the bottom - leave some margin from the bottom edge
        let icon_center = Point::new(size.width as i32 / 2, size.height as i32 - 80);

        Self {
            hold_to_confirm_border,
            checkmark,
            state: State::WaitingForHold,
            size,
            icon_center,
            last_drawn_state: None,
            holding: false,
            last_drawn_holding: false,
            progress: 0.0,
            last_update: None,
            hold_duration_ms,
            completed: false,
        }
    }

    pub fn enable(&mut self) {
        // Nothing to do - we handle everything internally
    }

    pub fn reset(&mut self) {
        if let Some(inner) = self.hold_to_confirm_border.inner_mut() {
            inner.inner_mut().set_progress(0.0);
        }
        self.checkmark.inner_mut().reset();
        self.state = State::WaitingForHold;
        self.last_drawn_state = None;
        self.holding = false;
        self.last_drawn_holding = false;
        self.progress = 0.0;
        self.last_update = None;
        self.completed = false;
    }

    pub fn is_completed(&self) -> bool {
        self.checkmark.inner().is_complete()
    }

    fn update_progress(&mut self, current_time: crate::Instant) {
        // Only process if we're actively holding or decaying
        if !self.holding && self.progress == 0.0 {
            return;
        }

        if let Some(last_time) = self.last_update {
            let elapsed_ms = current_time.saturating_duration_since(last_time) as f32;

            // Skip if no time has passed
            if elapsed_ms == 0.0 {
                return;
            }

            if self.holding && !self.completed {
                // Build up progress: complete in hold_duration_ms
                let increment = elapsed_ms / self.hold_duration_ms;
                self.progress = (self.progress + increment).min(1.0);
                if let Some(inner) = self.hold_to_confirm_border.inner_mut() {
                    inner.inner_mut().set_progress(self.progress);
                }

                if self.progress >= 1.0 {
                    self.completed = true;
                    self.progress = 1.0;
                    self.state = State::ShowingCheckmark;
                    self.checkmark.inner_mut().start_animation();
                }
            } else if !self.holding && self.progress > 0.0 && !self.completed {
                // Reduce progress: decay in 1000ms
                let decrement = elapsed_ms / 1000.0;
                self.progress = (self.progress - decrement).max(0.0);
                if let Some(inner) = self.hold_to_confirm_border.inner_mut() {
                    inner.inner_mut().set_progress(self.progress);
                }

                // Clear last_update when we reach 0 to stop updates
                if self.progress == 0.0 {
                    self.last_update = None;
                    return;
                }
            }

            // Update the timer for next frame
            self.last_update = Some(current_time);
        }
    }
}

impl Widget for HoldToConfirm {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Update progress based on holding state
        if self.holding || self.progress > 0.0 {
            self.update_progress(current_time);
        }

        // Always draw hold to confirm border animation
        self.hold_to_confirm_border.draw(target, current_time)?;

        // Only redraw the center content if state changed or holding state changed
        let should_redraw =
            self.last_drawn_state != Some(self.state) || self.holding != self.last_drawn_holding;

        if should_redraw {
            match self.state {
                State::WaitingForHold => {
                    // Draw circle with appropriate colors based on holding state
                    let (fill_color, border_color) = if self.holding {
                        // Green colors when holding
                        let green_fill: Rgb565 = Rgb888::new(22, 163, 74).into(); // green-600
                        (green_fill, Rgb565::new(2, 46, 16))
                    } else {
                        // Regular colors when not holding
                        (Rgb565::new(6, 16, 10), Rgb565::new(2, 46, 16))
                    };

                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(fill_color)
                        .stroke_color(border_color)
                        .stroke_width(2)
                        .build();

                    Circle::with_center(self.icon_center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;

                    // Draw the open select hand gesture icon in the center (white for contrast)
                    let icon = OpenSelectHandGesture::new(Rgb565::WHITE);
                    Image::with_center(&icon, self.icon_center).draw(target)?;
                }
                State::ShowingCheckmark => {
                    // Draw solid green circle (both fill and border are green)
                    // Using the same green as when holding
                    let green_fill: Rgb565 = Rgb888::new(22, 163, 74).into(); // green-600
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(green_fill)
                        .stroke_color(green_fill)
                        .stroke_width(2)
                        .build();

                    Circle::with_center(self.icon_center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;

                    self.hold_to_confirm_border
                        .start_fade(8_00, 1_0, PALETTE.background);
                }
            }

            self.last_drawn_state = Some(self.state);
            self.last_drawn_holding = self.holding;
        }

        // Always draw checkmark animation when in ShowingCheckmark state
        if self.state == State::ShowingCheckmark {
            // The checkmark is drawn from (0,0), so we need to translate it to center it
            let translation = Point::new(
                self.icon_center.x - 48, // Half of 96
                self.icon_center.y - 48, // Half of 96
            );

            let mut translated = target.translated(translation);
            self.checkmark.draw(&mut translated, current_time)?;
        }

        Ok(())
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if self.state == State::WaitingForHold {
            // Always handle release events, regardless of position
            if is_release {
                if self.holding {
                    // Released touch
                    self.holding = false;
                    if self.completed {
                        // No special action - parent should handle completion
                        // Don't reset automatically - let parent decide
                    } else {
                        // Not completed, will decay gradually
                        // Keep last_update so decay continues
                    }
                }
                return None;
            }

            // For press events, check if within circle
            let distance_squared =
                (point.x - self.icon_center.x).pow(2) + (point.y - self.icon_center.y).pow(2);
            let within_circle = distance_squared <= (CIRCLE_RADIUS as i32).pow(2);

            if within_circle && !self.holding {
                // Just started holding
                self.holding = true;
                self.last_update = Some(current_time);
                // Don't reset progress - let it continue from where it was
            }
        }

        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32) {}

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}
