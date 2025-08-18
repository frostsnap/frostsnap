use crate::super_draw_target::SuperDrawTarget;
use crate::{checkmark::Checkmark, palette::PALETTE, Center, Instant, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::Image,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyleBuilder},
};
use embedded_iconoir::{icons::size48px::gestures::OpenSelectHandGesture, prelude::IconoirNewIcon};

// Circle dimensions
const CIRCLE_RADIUS: u32 = 50;
const CIRCLE_DIAMETER: u32 = CIRCLE_RADIUS * 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircleButtonState {
    Idle,
    Pressed,
    ShowingCheckmark,
}

/// A circular button that shows a hand icon when idle/pressed and transitions to a checkmark
pub struct CircleButton {
    state: CircleButtonState,
    checkmark: Center<Checkmark<Rgb565>>,
    last_drawn_state: Option<CircleButtonState>,
    idle_color: Rgb565,
    pressed_color: Rgb565,
    pressed_outline_color: Rgb565,
}

impl CircleButton {
    pub fn new() -> Self {
        // Use a checkmark that fits nicely within the circle
        let checkmark = Center::new(Checkmark::new(50, PALETTE.on_tertiary_container));

        Self {
            state: CircleButtonState::Idle,
            checkmark,
            last_drawn_state: None,
            idle_color: PALETTE.surface_variant,
            pressed_color: PALETTE.tertiary_container,
            pressed_outline_color: PALETTE.confirm_progress,
        }
    }

    /// Set custom colors for pressed state
    pub fn set_pressed_colors(&mut self, pressed_fill: Rgb565, pressed_outline: Rgb565) {
        self.pressed_color = pressed_fill;
        self.pressed_outline_color = pressed_outline;
        // Force redraw to apply new colors
        self.last_drawn_state = None;
    }

    /// Set the button state
    pub fn set_state(&mut self, state: CircleButtonState) {
        self.state = state;
    }

    /// Get the current state
    pub fn state(&self) -> CircleButtonState {
        self.state
    }

    pub fn checkmark(&self) -> &Checkmark<Rgb565> {
        &self.checkmark.child
    }

    pub fn checkmark_mut(&mut self) -> &mut Checkmark<Rgb565> {
        &mut self.checkmark.child
    }

    /// Reset the button to idle state
    pub fn reset(&mut self) {
        self.state = CircleButtonState::Idle;
        self.checkmark.child.reset();
        self.last_drawn_state = None;
    }

    /// Check if a point is within the circle
    pub fn contains_point(&self, point: Point) -> bool {
        let center = Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);
        let distance_squared = (point.x - center.x).pow(2) + (point.y - center.y).pow(2);
        distance_squared <= (CIRCLE_RADIUS as i32).pow(2)
    }
}

impl crate::DynWidget for CircleButton {
    fn set_constraints(&mut self, _max_size: Size) {
        // CircleButton has a fixed size, but we need to set constraints on the checkmark
        // Give the checkmark the full circle area to work with
        self.checkmark
            .set_constraints(Size::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER));
    }

    fn sizing(&self) -> crate::Sizing {
        Size::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER).into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if self.state == CircleButtonState::ShowingCheckmark {
            // Don't handle touches when showing checkmark
            return None;
        }

        if is_release {
            // Release - go back to idle
            if self.state == CircleButtonState::Pressed {
                self.state = CircleButtonState::Idle;
            }
        } else if self.contains_point(point) {
            // Press within button - set to pressed
            self.state = CircleButtonState::Pressed;
        }

        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

    fn force_full_redraw(&mut self) {
        self.last_drawn_state = None;
    }
}

impl Widget for CircleButton {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let center = Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);

        // Only redraw the circle if state changed
        let should_redraw = self.last_drawn_state != Some(self.state);

        if should_redraw {
            match self.state {
                CircleButtonState::Idle => {
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(self.idle_color)
                        .stroke_color(PALETTE.text_secondary)  // Grey outline when idle
                        .stroke_width(2)
                        .build();

                    Circle::with_center(center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;

                    let icon = OpenSelectHandGesture::new(PALETTE.on_surface_variant);
                    Image::with_center(&icon, center).draw(target)?;
                }
                CircleButtonState::Pressed => {
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(self.pressed_color)
                        .stroke_color(self.pressed_outline_color)
                        .stroke_width(2)
                        .build();

                    Circle::with_center(center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;

                    let icon = OpenSelectHandGesture::new(PALETTE.on_tertiary_container);
                    Image::with_center(&icon, center).draw(target)?;
                }
                CircleButtonState::ShowingCheckmark => {
                    // Draw solid circle (both fill and border use pressed color)
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(self.pressed_color)
                        .stroke_color(self.pressed_color)
                        .stroke_width(2)
                        .build();

                    Circle::with_center(center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;
                }
            }

            self.last_drawn_state = Some(self.state);
        }

        // Draw checkmark animation when in ShowingCheckmark state
        if self.state == CircleButtonState::ShowingCheckmark {
            self.checkmark.draw(target, current_time)?;
        }

        Ok(())
    }
}
