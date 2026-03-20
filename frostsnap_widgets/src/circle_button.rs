use crate::aa::circle::AACircle;
use crate::super_draw_target::SuperDrawTarget;
use crate::{checkmark::Checkmark, palette::PALETTE, prelude::*, ColorInterpolate, Frac};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Gray8, GrayColor, Rgb565},
    prelude::*,
};
use tinybmp::Bmp;

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
#[derive(Clone)]
pub struct CircleButton {
    state: CircleButtonState,
    checkmark: Center<Checkmark<Rgb565>>,
    last_drawn_state: Option<CircleButtonState>,
    idle_stroke_color: Rgb565,
    pressed_fill_color: Rgb565,
    pressed_stroke_color: Rgb565,
    checkmark_color: Rgb565,
}

impl Default for CircleButton {
    fn default() -> Self {
        Self::new()
    }
}

const TOUCH_ICON_DATA: &[u8] = include_bytes!("../assets/touch-icon-100x100.bmp");

/// Draw the touch icon centered in the circle, blending grayscale values between
/// icon_color (for dark pixels) and bg_color (for light pixels).
/// Only draws pixels that fall inside the circle to avoid bleeding outside the AA border.
fn draw_icon<D: DrawTarget<Color = Rgb565>>(
    target: &mut D,
    icon_color: Rgb565,
    bg_color: Rgb565,
) -> Result<(), D::Error> {
    let bmp = Bmp::<Gray8>::from_slice(TOUCH_ICON_DATA).expect("Failed to load touch icon BMP");
    let icon_size = bmp.size();

    let offset_x = (CIRCLE_DIAMETER as i32 - icon_size.width as i32) / 2;
    let offset_y = (CIRCLE_DIAMETER as i32 - icon_size.height as i32) / 2;

    let center_x = CIRCLE_RADIUS as i32;
    let center_y = CIRCLE_RADIUS as i32;
    // Clip to the inner edge of the circle stroke (radius 48, stroke 2 → inner radius 46)
    // so icon pixels don't overwrite the AA-blended border
    let inner_radius = ((CIRCLE_DIAMETER - 4) / 2 - 2) as i32;
    let clip_radius_sq = inner_radius * inner_radius;

    target.draw_iter(bmp.pixels().filter_map(|Pixel(point, gray)| {
        let intensity = gray.luma();
        if intensity == 255 {
            return None;
        }
        let dest = Point::new(point.x + offset_x, point.y + offset_y);
        // Skip pixels outside the circle
        let dx = dest.x - center_x;
        let dy = dest.y - center_y;
        if dx * dx + dy * dy > clip_radius_sq {
            return None;
        }
        // luma 0 = fully icon_color, luma 255 = fully bg_color (skipped above)
        let frac = Frac::from_ratio(intensity as u32, 255);
        let blended = icon_color.interpolate(bg_color, frac);
        Some(Pixel(dest, blended))
    }))
}

impl CircleButton {
    pub fn new() -> Self {
        let checkmark = Center::new(Checkmark::new(50, PALETTE.on_tertiary_container));

        Self {
            state: CircleButtonState::Idle,
            checkmark,
            last_drawn_state: None,
            idle_stroke_color: PALETTE.outline,
            pressed_fill_color: PALETTE.tertiary_container,
            pressed_stroke_color: PALETTE.confirm_progress,
            checkmark_color: PALETTE.on_tertiary_container,
        }
    }

    pub fn set_pressed_colors(
        &mut self,
        pressed_fill: Rgb565,
        pressed_stroke: Rgb565,
        checkmark_color: Rgb565,
    ) {
        self.pressed_fill_color = pressed_fill;
        self.pressed_stroke_color = pressed_stroke;
        self.checkmark_color = checkmark_color;
        self.checkmark.child.set_color(checkmark_color);
        self.force_full_redraw();
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
        self.checkmark.force_full_redraw();
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
            let bg = target.background_color();
            let radius = (CIRCLE_DIAMETER - 4) / 2;
            const STROKE_WIDTH: u32 = 2;

            match self.state {
                CircleButtonState::Idle => {
                    AACircle::new(
                        center,
                        radius,
                        STROKE_WIDTH,
                        PALETTE.surface_variant,
                        self.idle_stroke_color,
                        bg,
                    )
                    .draw(target)?;

                    draw_icon(target, PALETTE.on_surface_variant, PALETTE.surface_variant)?;
                }
                CircleButtonState::Pressed => {
                    AACircle::new(
                        center,
                        radius,
                        STROKE_WIDTH,
                        self.pressed_fill_color,
                        self.pressed_stroke_color,
                        bg,
                    )
                    .draw(target)?;

                    draw_icon(
                        target,
                        PALETTE.on_tertiary_container,
                        self.pressed_fill_color,
                    )?;
                }
                CircleButtonState::ShowingCheckmark => {
                    AACircle::new(
                        center,
                        radius,
                        STROKE_WIDTH,
                        self.pressed_fill_color,
                        self.pressed_fill_color,
                        bg,
                    )
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
