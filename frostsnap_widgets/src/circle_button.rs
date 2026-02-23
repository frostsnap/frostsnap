use crate::sdf;
use crate::super_draw_target::SuperDrawTarget;
use crate::vec_framebuffer::VecFramebuffer;
use crate::{checkmark::Checkmark, palette::PALETTE, prelude::*};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::Image,
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
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

/// A circular button that shows a hand icon when idle/pressed and transitions to a checkmark.
/// Idle and Pressed states are pre-rendered into Rgb565 framebuffers so that each draw is a
/// single fill_contiguous call, avoiding flicker from layered circle + icon drawing.
pub struct CircleButton {
    state: CircleButtonState,
    checkmark: Center<Checkmark<Rgb565>>,
    last_drawn_state: Option<CircleButtonState>,
    // Pre-rendered framebuffers for flicker-free drawing
    idle_fb: VecFramebuffer<Rgb565>,
    pressed_fb: VecFramebuffer<Rgb565>,
    // Checkmark state needs a pre-rendered circle background + animated checkmark on top
    checkmark_bg_fb: VecFramebuffer<Rgb565>,
}

impl Default for CircleButton {
    fn default() -> Self {
        Self::new()
    }
}

/// Circle radius used by SDF rendering (matches the old CIRCLE_DIAMETER - 4 geometry).
/// The old circle was `Circle::with_center(center, CIRCLE_DIAMETER - 4)` which means
/// diameter = 96, radius = 48. The stroke was 2px centered on the boundary.
const SDF_CIRCLE_RADIUS: f32 = 48.0;
const SDF_STROKE_WIDTH: f32 = 2.0;

/// Render a filled+stroked circle with an icon centered on it into a framebuffer
fn render_circle_with_icon(
    fill_color: Rgb565,
    stroke_color: Rgb565,
    icon_color: Rgb565,
    bg_color: Rgb565,
) -> VecFramebuffer<Rgb565> {
    let mut fb = VecFramebuffer::<Rgb565>::new(CIRCLE_DIAMETER as usize, CIRCLE_DIAMETER as usize);
    fb.clear(bg_color);

    let cx = CIRCLE_RADIUS as f32;
    let cy = CIRCLE_RADIUS as f32;

    sdf::render_circle_aa(
        &mut fb,
        cx,
        cy,
        SDF_CIRCLE_RADIUS,
        Some(SDF_STROKE_WIDTH),
        fill_color,
        stroke_color,
        bg_color,
    );

    let center = Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);
    let icon = OpenSelectHandGesture::new(icon_color);
    Image::with_center(&icon, center).draw(&mut fb).unwrap();

    fb
}

/// Render just a filled circle (no icon) into a framebuffer
fn render_circle_only(
    fill_color: Rgb565,
    stroke_color: Rgb565,
    bg_color: Rgb565,
) -> VecFramebuffer<Rgb565> {
    let mut fb = VecFramebuffer::<Rgb565>::new(CIRCLE_DIAMETER as usize, CIRCLE_DIAMETER as usize);
    fb.clear(bg_color);

    let cx = CIRCLE_RADIUS as f32;
    let cy = CIRCLE_RADIUS as f32;

    sdf::render_circle_aa(
        &mut fb,
        cx,
        cy,
        SDF_CIRCLE_RADIUS,
        Some(SDF_STROKE_WIDTH),
        fill_color,
        stroke_color,
        bg_color,
    );

    fb
}

impl CircleButton {
    pub fn new() -> Self {
        let checkmark = Center::new(Checkmark::new(50, PALETTE.on_tertiary_container));

        let idle_fb = render_circle_with_icon(
            PALETTE.surface_variant,
            PALETTE.outline,
            PALETTE.on_surface_variant,
            PALETTE.background,
        );

        let pressed_fb = render_circle_with_icon(
            PALETTE.tertiary_container,
            PALETTE.confirm_progress,
            PALETTE.on_tertiary_container,
            PALETTE.background,
        );

        let checkmark_bg_fb = render_circle_only(
            PALETTE.tertiary_container,
            PALETTE.tertiary_container,
            PALETTE.background,
        );

        Self {
            state: CircleButtonState::Idle,
            checkmark,
            last_drawn_state: None,
            idle_fb,
            pressed_fb,
            checkmark_bg_fb,
        }
    }

    /// Set custom colors for the pressed state
    pub fn set_pressed_colors(&mut self, pressed_fill: Rgb565, pressed_stroke: Rgb565) {
        // For danger actions (red), use white checkmark; otherwise use default
        let icon_color = if pressed_fill == PALETTE.error {
            self.checkmark.child.set_color(PALETTE.on_error);
            PALETTE.on_error
        } else {
            self.checkmark
                .child
                .set_color(PALETTE.on_tertiary_container);
            PALETTE.on_tertiary_container
        };

        // Re-render pressed framebuffer with new colors
        self.pressed_fb = render_circle_with_icon(
            pressed_fill,
            pressed_stroke,
            icon_color,
            PALETTE.background,
        );

        // Re-render checkmark background with new colors
        self.checkmark_bg_fb =
            render_circle_only(pressed_fill, pressed_fill, PALETTE.background);

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
        // Only redraw the circle if state changed
        let should_redraw = self.last_drawn_state != Some(self.state);

        if should_redraw {
            let fb = match self.state {
                CircleButtonState::Idle => &self.idle_fb,
                CircleButtonState::Pressed => &self.pressed_fb,
                CircleButtonState::ShowingCheckmark => &self.checkmark_bg_fb,
            };

            let area = Rectangle::new(Point::zero(), Size::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER));
            target.fill_contiguous(&area, fb.contiguous_pixels())?;

            self.last_drawn_state = Some(self.state);
        }

        // Draw checkmark animation when in ShowingCheckmark state
        if self.state == CircleButtonState::ShowingCheckmark {
            self.checkmark.draw(target, current_time)?;
        }

        Ok(())
    }
}
