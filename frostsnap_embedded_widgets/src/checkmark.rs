use super::{pixel_recorder::PixelRecorder, Widget};
use crate::compressed_point::CompressedPoint;
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
    check_pixels: Vec<CompressedPoint>,
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
        let mut recorder = PixelRecorder::new();

        // Define checkmark points for perfect right angle
        // Second segment is 2.2x the length of the first segment
        let width = self.width as f32;

        // Calculate segment lengths to use full width
        // The checkmark total width = first_x + second_x
        // We want: (first_x + second_x) + margins = width
        // With stroke width 8, we need margin of 4 on each side
        let margin = 5; // Radius of cap circle + 1
        let available_width = width - (2.0 * margin as f32);
        
        // For 45-degree angles, x and y components are length / sqrt(2)
        let sqrt_2 = 1.414213562373095_f32;
        
        // With ratio of 2.2:1 for second:first segment
        // first_x + second_x = available_width
        // (first_length / sqrt_2) + (2.2 * first_length / sqrt_2) = available_width
        // first_length * (1 + 2.2) / sqrt_2 = available_width
        // first_length = available_width * sqrt_2 / 3.2
        let first_segment_length = available_width * sqrt_2 / 3.2;
        let second_segment_length = first_segment_length * 2.2;

        // First segment: 45 degrees down-right
        let first_x_offset = (first_segment_length / sqrt_2) as i32;
        let first_y_offset = (first_segment_length / sqrt_2) as i32;

        // Second segment: 45 degrees up-right (perpendicular to first)
        let second_x_offset = (second_segment_length / sqrt_2) as i32;
        let second_y_offset = (second_segment_length / sqrt_2) as i32;

        // Position the middle point with margin
        let middle = Point::new(first_x_offset + margin, second_y_offset + margin);

        // Calculate the other points based on the middle point
        let left_start = Point::new(middle.x - first_x_offset, middle.y - first_y_offset);
        let right_end = Point::new(middle.x + second_x_offset, middle.y - second_y_offset);

        // Draw with thicker lines for better visibility
        let stroke_width = 8;
        let style = PrimitiveStyle::with_stroke(BinaryColor::On, stroke_width);

        Line::new(left_start, middle)
            .into_styled(style)
            .draw(&mut recorder)
            .ok();

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

        self.check_pixels = recorder.pixels;
        self.check_pixels.sort_by_key(|cp| cp.x);
        
        // Find the actual bounds of the recorded pixels
        if !self.check_pixels.is_empty() {
            let max_x = self.check_pixels.iter().map(|cp| cp.x).max().unwrap_or(0);
            let max_y = self.check_pixels.iter().map(|cp| cp.y as u32).max().unwrap_or(0);
            self.check_width = (max_x + 1) as u32;  // +1 because coordinates are 0-based
            self.check_height = max_y + 1;
        } else {
            // Fallback to calculated values
            self.check_width = self.width;
            self.check_height = (middle.y + margin) as u32;
        }
    }

    fn update_animation(&mut self, current_time: crate::Instant) {
        if !self.enabled {
            return;
        }

        // Animation duration in milliseconds
        const CHECK_DURATION_MS: u64 = 800;

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
        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing | AnimationState::Complete => {
                let check_progress = self.progress.min(1.0); // Clamp to 1.0
                let current_pixels = (self.check_pixels.len() as f32 * check_progress) as usize;
                let last_pixels = (self.check_pixels.len() as f32
                    * self.last_drawn_check_progress.min(1.0))
                    as usize;

                if current_pixels > last_pixels && current_pixels <= self.check_pixels.len() {
                    target.draw_iter(
                        self.check_pixels[last_pixels..current_pixels]
                            .iter()
                            .map(|cp| Pixel(cp.to_point(), BinaryColor::On)),
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

    fn handle_vertical_drag(&mut self, _start_y: Option<u32>, _current_y: u32, _is_release: bool) {}

    fn size_hint(&self) -> Option<Size> {
        // Return the actual size of the checkmark
        Some(Size::new(self.check_width, self.check_height))
    }
}
