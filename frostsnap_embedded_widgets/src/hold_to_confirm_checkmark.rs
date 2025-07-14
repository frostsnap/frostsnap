use crate::{
    checkmark::Checkmark, color_map::ColorMap, hold_to_confirm::HoldToConfirm, palette::PALETTE,
    sized_box::SizedBox, Widget,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    image::Image,
    pixelcolor::{BinaryColor, Rgb565},
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
};
use embedded_iconoir::{icons::size48px::gestures::OpenSelectHandGesture, prelude::IconoirNewIcon};

// Circle dimensions
const CIRCLE_RADIUS: u32 = 40;
const CIRCLE_DIAMETER: u32 = CIRCLE_RADIUS * 2;

/// A widget that combines HoldToConfirm with a hand gesture icon and transitions to a checkmark
pub struct HoldToConfirmCheckmark {
    hold_to_confirm: ColorMap<HoldToConfirm<SizedBox<BinaryColor>>, Rgb565>,
    checkmark: ColorMap<Checkmark, Rgb565>,
    state: State,
    size: Size,
    icon_center: Point,
    last_drawn_state: Option<State>,
    dark_color: Rgb565,
    light_color: Rgb565,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    WaitingForHold,
    ShowingCheckmark,
}

impl HoldToConfirmCheckmark {
    pub fn new(size: Size, hold_duration_ms: f32) -> Self {
        let sized_box = SizedBox::<BinaryColor>::new(size);
        let hold_to_confirm_binary = HoldToConfirm::new(sized_box, hold_duration_ms);
        let hold_to_confirm = hold_to_confirm_binary.color_map(|color| match color {
            BinaryColor::On => PALETTE.primary,
            BinaryColor::Off => PALETTE.surface_variant,
        });

        let checkmark_binary = Checkmark::new(Size::new(50, 50)); // Standard checkmark size
        let checkmark = checkmark_binary.color_map(|color| match color {
            BinaryColor::On => PALETTE.primary,
            BinaryColor::Off => PALETTE.background,
        });

        // Position icon towards the bottom - leave some margin from the bottom edge
        let icon_center = Point::new(size.width as i32 / 2, size.height as i32 - 80);

        // Store gradient colors
        let dark_color = Rgb565::new(4, 10, 7); // slate-800
        let light_color = Rgb565::new(6, 16, 10); // slate-700

        Self {
            hold_to_confirm,
            checkmark,
            state: State::WaitingForHold,
            size,
            icon_center,
            last_drawn_state: None,
            dark_color,
            light_color,
        }
    }

    pub fn enable(&mut self) {
        self.hold_to_confirm.inner_mut().enable();
    }

    pub fn reset(&mut self) {
        self.hold_to_confirm.inner_mut().reset();
        self.checkmark.inner_mut().reset();
        self.state = State::WaitingForHold;
        self.last_drawn_state = None;
    }

    pub fn is_completed(&self) -> bool {
        self.checkmark.inner().is_complete()
    }
}

impl Widget for HoldToConfirmCheckmark {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Check if hold to confirm just completed
        if self.state == State::WaitingForHold && self.hold_to_confirm.inner().is_completed() {
            self.state = State::ShowingCheckmark;
            self.checkmark.inner_mut().start_animation();
        }

        // Always draw hold to confirm border animation
        self.hold_to_confirm.draw(target, current_time)?;

        // Only redraw the center content if state changed
        if self.last_drawn_state != Some(self.state) {
            match self.state {
                State::WaitingForHold => {
                    // Draw filled circle with gradient
                    let circle_offset =
                        self.icon_center - Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);

                    Circle::new(circle_offset, CIRCLE_DIAMETER)
                        .into_styled(PrimitiveStyle::with_fill(self.dark_color))
                        .draw(target)?;

                    // Draw the border on top
                    Circle::new(circle_offset, CIRCLE_DIAMETER)
                        .into_styled(PrimitiveStyle::with_stroke(PALETTE.outline, 2))
                        .draw(target)?;

                    // Draw the open select hand gesture icon in the center (white for contrast)
                    let icon = OpenSelectHandGesture::new(Rgb565::WHITE);
                    Image::with_center(&icon, self.icon_center).draw(target)?;
                }
                State::ShowingCheckmark => {
                    // Clear the area first
                    let top_left =
                        self.icon_center - Point::new(CIRCLE_RADIUS as i32, CIRCLE_RADIUS as i32);
                    let area =
                        Rectangle::new(top_left, Size::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER));
                    target.fill_solid(&area, PALETTE.background)?;

                    // Draw the checkmark at the same position as the icon was
                    let checkmark_offset = Point::new(
                        self.icon_center.x - 25, // 50px checkmark, so offset by half
                        self.icon_center.y - 25,
                    );
                    let mut checkmark_target = target.translated(checkmark_offset);

                    // Draw checkmark (already wrapped in ColorMap)
                    self.checkmark.draw(&mut checkmark_target, current_time)?;
                }
            }

            self.last_drawn_state = Some(self.state);
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
            // Check if touch is within the icon circle
            let distance_squared =
                (point.x - self.icon_center.x).pow(2) + (point.y - self.icon_center.y).pow(2);
            let within_circle = distance_squared <= (CIRCLE_RADIUS as i32).pow(2);

            if within_circle {
                // Pass the touch to hold_to_confirm
                self.hold_to_confirm
                    .inner_mut()
                    .handle_touch(point, current_time, is_release)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}
