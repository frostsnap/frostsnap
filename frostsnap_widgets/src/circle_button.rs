use crate::super_draw_target::SuperDrawTarget;
use crate::{checkmark::Checkmark, palette::PALETTE, prelude::*, GrayToAlpha};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{Gray8, Rgb565},
};
use tinybmp::Bmp;

const CIRCLE_RADIUS: u32 = 50;
const CIRCLE_DIAMETER: u32 = CIRCLE_RADIUS * 2;

const TOUCH_ICON_DATA: &[u8] = include_bytes!("../assets/touch-icon-100x100.bmp");

type TouchIcon = Center<crate::Image<GrayToAlpha<Bmp<'static, Gray8>, Rgb565>>>;

fn make_icon(color: Rgb565) -> TouchIcon {
    let bmp = Bmp::<Gray8>::from_slice(TOUCH_ICON_DATA).expect("valid BMP");
    Center::new(crate::Image::new(GrayToAlpha::new(bmp, color)))
}

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
    last_drawn_state: Option<CircleButtonState>,

    idle: CircleContainer<TouchIcon>,
    pressed: CircleContainer<TouchIcon>,
    checkmark_circle: CircleContainer<Center<Checkmark<Rgb565>>>,
}

impl Default for CircleButton {
    fn default() -> Self {
        Self::new()
    }
}

impl CircleButton {
    pub fn new() -> Self {
        let idle = CircleContainer::new(
            make_icon(PALETTE.on_surface_variant),
            CIRCLE_RADIUS,
            PALETTE.surface_variant,
            PALETTE.outline,
        );

        let pressed = CircleContainer::new(
            make_icon(PALETTE.on_tertiary_container),
            CIRCLE_RADIUS,
            PALETTE.tertiary_container,
            PALETTE.confirm_progress,
        );

        let checkmark_circle = CircleContainer::new(
            Center::new(Checkmark::new(50, PALETTE.on_tertiary_container)),
            CIRCLE_RADIUS,
            PALETTE.tertiary_container,
            PALETTE.tertiary_container,
        );

        Self {
            state: CircleButtonState::Idle,
            last_drawn_state: None,
            idle,
            pressed,
            checkmark_circle,
        }
    }

    pub fn set_pressed_colors(
        &mut self,
        pressed_fill: Rgb565,
        pressed_stroke: Rgb565,
        checkmark_color: Rgb565,
    ) {
        self.pressed.set_colors(pressed_fill, pressed_stroke);
        self.checkmark_circle.set_colors(pressed_fill, pressed_fill);
        self.checkmark_circle.child.child.set_color(checkmark_color);
        self.force_full_redraw();
    }

    pub fn set_state(&mut self, state: CircleButtonState) {
        self.state = state;
    }

    pub fn state(&self) -> CircleButtonState {
        self.state
    }

    pub fn checkmark(&self) -> &Checkmark<Rgb565> {
        &self.checkmark_circle.child.child
    }

    pub fn checkmark_mut(&mut self) -> &mut Checkmark<Rgb565> {
        &mut self.checkmark_circle.child.child
    }

    pub fn reset(&mut self) {
        self.state = CircleButtonState::Idle;
        self.checkmark_circle.child.child.reset();
        self.last_drawn_state = None;
    }

    pub fn contains_point(&self, point: Point) -> bool {
        let center = Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);
        let distance_squared = (point.x - center.x).pow(2) + (point.y - center.y).pow(2);
        distance_squared <= (CIRCLE_RADIUS as i32).pow(2)
    }
}

impl crate::DynWidget for CircleButton {
    fn set_constraints(&mut self, max_size: Size) {
        self.idle.set_constraints(max_size);
        self.pressed.set_constraints(max_size);
        self.checkmark_circle.set_constraints(max_size);
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
            return None;
        }

        if is_release {
            if self.state == CircleButtonState::Pressed {
                self.state = CircleButtonState::Idle;
            }
        } else if self.contains_point(point) {
            self.state = CircleButtonState::Pressed;
        }

        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

    fn force_full_redraw(&mut self) {
        self.last_drawn_state = None;
        self.idle.force_full_redraw();
        self.pressed.force_full_redraw();
        self.checkmark_circle.force_full_redraw();
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
        let should_redraw = self.last_drawn_state != Some(self.state);

        if should_redraw {
            match self.state {
                CircleButtonState::Idle => {
                    self.idle.force_full_redraw();
                    self.idle.draw(target, current_time)?;
                }
                CircleButtonState::Pressed => {
                    self.pressed.force_full_redraw();
                    self.pressed.draw(target, current_time)?;
                }
                CircleButtonState::ShowingCheckmark => {
                    self.checkmark_circle.force_full_redraw();
                    self.checkmark_circle.draw(target, current_time)?;
                }
            }
            self.last_drawn_state = Some(self.state);
        }

        if self.state == CircleButtonState::ShowingCheckmark {
            self.checkmark_circle.draw(target, current_time)?;
        }

        Ok(())
    }
}
