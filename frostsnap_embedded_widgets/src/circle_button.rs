use crate::{Widget, Instant, checkmark::Checkmark, Center, ColorMap};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::Image,
    pixelcolor::{BinaryColor, Rgb565, Rgb888},
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
    checkmark: ColorMap<Center<Checkmark>, Rgb565>,
    last_drawn_state: Option<CircleButtonState>,
    checkmark_started: bool,
}

impl CircleButton {
    pub fn new() -> Self {
        // Use a checkmark that fits nicely within the circle
        let checkmark = Center::new(Checkmark::new(50)).color_map(|color| match color {
            BinaryColor::On => Rgb565::WHITE,
            BinaryColor::Off => Rgb565::RED, // Doesn't matter, won't be visible
        });
        
        Self {
            state: CircleButtonState::Idle,
            checkmark,
            last_drawn_state: None,
            checkmark_started: false,
        }
    }
    
    /// Set the button state
    pub fn set_state(&mut self, state: CircleButtonState) {
        if self.state != state {
            self.state = state;
            if state == CircleButtonState::ShowingCheckmark && !self.checkmark_started {
                self.checkmark.child.child.start_animation();
                self.checkmark_started = true;
            }
        }
    }
    
    /// Get the current state
    pub fn state(&self) -> CircleButtonState {
        self.state
    }
    
    /// Check if the checkmark animation is complete
    pub fn is_checkmark_complete(&self) -> bool {
        self.checkmark.child.child.is_complete()
    }
    
    /// Reset the button to idle state
    pub fn reset(&mut self) {
        self.state = CircleButtonState::Idle;
        self.checkmark.child.child.reset();
        self.last_drawn_state = None;
        self.checkmark_started = false;
    }
    
    /// Check if a point is within the circle
    pub fn contains_point(&self, point: Point) -> bool {
        let center = Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);
        let distance_squared = (point.x - center.x).pow(2) + (point.y - center.y).pow(2);
        distance_squared <= (CIRCLE_RADIUS as i32).pow(2)
    }
}

impl Widget for CircleButton {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        let center = Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);
        
        // Only redraw the circle if state changed
        let should_redraw = self.last_drawn_state != Some(self.state);
        
        if should_redraw {
            match self.state {
                CircleButtonState::Idle => {
                    // Regular colors when not holding
                    let fill_color = Rgb565::new(6, 16, 10);
                    let border_color = Rgb565::new(2, 46, 16);
                    
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(fill_color)
                        .stroke_color(border_color)
                        .stroke_width(2)
                        .build();
                    
                    Circle::with_center(center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;
                    
                    let icon = OpenSelectHandGesture::new(Rgb565::WHITE);
                    Image::with_center(&icon, center).draw(target)?;
                }
                CircleButtonState::Pressed => {
                    // Green colors when holding
                    let green_fill: Rgb565 = Rgb888::new(22, 163, 74).into(); // green-600
                    let border_color = Rgb565::new(2, 46, 16);
                    
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(green_fill)
                        .stroke_color(border_color)
                        .stroke_width(2)
                        .build();
                    
                    Circle::with_center(center, CIRCLE_DIAMETER - 4)
                        .into_styled(circle_style)
                        .draw(target)?;
                    
                    let icon = OpenSelectHandGesture::new(Rgb565::WHITE);
                    Image::with_center(&icon, center).draw(target)?;
                }
                CircleButtonState::ShowingCheckmark => {
                    // Draw solid green circle (both fill and border are green)
                    let green_fill: Rgb565 = Rgb888::new(22, 163, 74).into(); // green-600
                    let circle_style = PrimitiveStyleBuilder::new()
                        .fill_color(green_fill)
                        .stroke_color(green_fill)
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
        if self.state == CircleButtonState::ShowingCheckmark && self.checkmark_started {
            self.checkmark.draw(target, current_time)?;
        }
        
        Ok(())
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
    
    fn size_hint(&self) -> Option<Size> {
        Some(Size::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER))
    }
    
    fn force_full_redraw(&mut self) {
        self.last_drawn_state = None;
    }
}