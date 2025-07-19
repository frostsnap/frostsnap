use super::{pixel_recorder::PixelRecorder, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle},
};

pub struct Checkmark {
    width: u32,
    check_pixels: Vec<Point>,
    progress: f32, // 0.0 to 1.0
    last_drawn_check_progress: f32,
    animation_state: AnimationState,
    enabled: bool,
    animation_start_time: Option<crate::Instant>,
    check_width: u32,
    check_height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AnimationState {
    Idle,
    Drawing,
    Complete,
}

impl Checkmark {
    pub fn new(width: u32) -> Self {
        let mut checkmark = Self {
            width,
            check_pixels: Vec::new(),
            progress: 0.0,
            last_drawn_check_progress: -1.0,
            animation_state: AnimationState::Idle,
            enabled: false,
            animation_start_time: None,
            check_width: 0,
            check_height: 0,
        };

        checkmark.record_pixels();
        checkmark
    }

    pub fn start_animation(&mut self) {
        self.enabled = true;
        self.progress = 0.0;
        self.last_drawn_check_progress = -1.0;
        self.animation_state = AnimationState::Drawing;
        self.animation_start_time = None; // Will be set on first draw
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.progress = 0.0;
        self.last_drawn_check_progress = -1.0;
        self.animation_state = AnimationState::Idle;
    }

    pub fn is_complete(&self) -> bool {
        self.animation_state == AnimationState::Complete
    }

    fn record_pixels(&mut self) {
        // Use PixelRecorder to capture custom checkmark shape
        let mut recorder = PixelRecorder::new();

        // Define checkmark points for perfect right angle
        // Second segment is 2.2x the length of the first segment
        let width = self.width as f32;

        // Calculate segment lengths based on width and ratio
        // Total checkmark width is roughly first_segment + second_segment projected on x-axis
        // Fjirst segment: 15% of width
        let first_segment_length = width * 0.15;
        let second_segment_length = first_segment_length * 2.2;

        // For 45-degree angles, x and y components are length / sqrt(2)
        let sqrt_2 = 1.414213562373095_f32;

        // First segment: 45 degrees down-right
        let first_x_offset = (first_segment_length / sqrt_2) as i32;
        let first_y_offset = (first_segment_length / sqrt_2) as i32;

        // Second segment: 45 degrees up-right (perpendicular to first)
        let second_x_offset = (second_segment_length / sqrt_2) as i32;
        let second_y_offset = (second_segment_length / sqrt_2) as i32;

        let middle = Point::new(first_x_offset, second_y_offset);

        // Calculate the other points based on the middle point
        let left_start = Point::new(middle.x - first_x_offset, middle.y - first_y_offset);
        let right_end = Point::new(middle.x + second_x_offset, middle.y - second_y_offset);

        // Draw with thicker lines for better visibility
        let stroke_width = 8;
        let style = PrimitiveStyle::with_stroke(BinaryColor::On, stroke_width);

        // Draw left stroke
        Line::new(left_start, middle)
            .into_styled(style)
            .draw(&mut recorder)
            .ok();

        // Draw right stroke
        Line::new(middle, right_end)
            .into_styled(style)
            .draw(&mut recorder)
            .ok();

        // Add rounded caps at the ends - matching stroke width for subtle rounding
        let cap_diameter = stroke_width; // Same as stroke width for natural rounding
        let cap_style = PrimitiveStyle::with_fill(BinaryColor::On);

        // Round cap at left start
        Circle::with_center(left_start, cap_diameter)
            .into_styled(cap_style)
            .draw(&mut recorder)
            .ok();

        // Round cap at right end
        Circle::with_center(right_end - Point::new(1, 0), cap_diameter)
            .into_styled(cap_style)
            .draw(&mut recorder)
            .ok();

        // Round cap at middle joint
        Circle::with_center(middle - Point::new(1, 1), cap_diameter)
            .into_styled(cap_style)
            .draw(&mut recorder)
            .ok();

        self.check_width = right_end.x as u32;
        self.check_height = middle.y as u32;
        // Sort pixels by x-coordinate (left to right animation)
        self.check_pixels = recorder.pixels;
        self.check_pixels.sort_by_key(|p| p.x);
    }

    fn update_animation(&mut self, current_time: crate::Instant) {
        if !self.enabled {
            return;
        }

        // Animation duration in milliseconds
        const CHECK_DURATION_MS: u64 = 800;

        // Initialize animation start time if not set
        if self.animation_start_time.is_none() {
            self.animation_start_time = Some(current_time);
        }

        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing => {
                let start = self.animation_start_time.unwrap();
                let elapsed = current_time.saturating_duration_since(start);
                self.progress = (elapsed as f32 / CHECK_DURATION_MS as f32).min(1.0);

                if self.progress >= 1.0 {
                    self.progress = 1.0;
                    self.animation_state = AnimationState::Complete;
                }
            }
            AnimationState::Complete => {}
        }
    }

    fn draw_animation<D: DrawTarget<Color = BinaryColor>>(
        &mut self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        let mut translated = target.translated(Point::new(
            (target.bounding_box().size.width) as i32 / 2 - self.check_width as i32 / 2,
            target.bounding_box().size.height as i32 / 2 - self.check_height as i32 / 2,
        ));
        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing | AnimationState::Complete => {
                // Draw checkmark progress
                let check_progress = self.progress.min(1.0); // Clamp to 1.0
                let current_pixels = (self.check_pixels.len() as f32 * check_progress) as usize;
                let last_pixels = (self.check_pixels.len() as f32
                    * self.last_drawn_check_progress.min(1.0))
                    as usize;

                if current_pixels > last_pixels && current_pixels <= self.check_pixels.len() {
                    translated.draw_iter(
                        self.check_pixels[last_pixels..current_pixels]
                            .iter()
                            .map(|&point| Pixel(point, BinaryColor::On)),
                    )?;
                }

                self.last_drawn_check_progress = check_progress;
            }
        }

        Ok(())
    }
}

impl Widget for Checkmark {
    type Color = BinaryColor;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.enabled {
            self.update_animation(current_time);
            self.draw_animation(target)?;
        }
        Ok(())
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _start_y: Option<u32>, _current_y: u32) {}

    fn size_hint(&self) -> Option<Size> {
        // For simplicity, return a square that can contain the checkmark
        // The actual checkmark will be positioned within this square
        Some(Size::new(self.width, self.width))
    }
}
