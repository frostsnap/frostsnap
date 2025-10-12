use super::{pixel_recorder::PixelRecorder, Widget};
use crate::{
    compressed_point::CompressedPoint, super_draw_target::SuperDrawTarget, Frac, Instant, Rat,
};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, PixelColor},
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle},
};

#[derive(PartialEq)]
pub struct Checkmark<C> {
    width: u32,
    color: C,
    check_pixels: Vec<CompressedPoint>,
    progress: Frac,
    last_drawn_check_progress: Option<Frac>,
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

impl<C: PixelColor> Checkmark<C> {
    pub fn new(width: u32, color: C) -> Self {
        let mut checkmark = Self {
            width,
            color,
            check_pixels: Vec::new(),
            progress: Frac::ZERO,
            last_drawn_check_progress: None,
            animation_state: AnimationState::Idle,
            enabled: false,
            animation_start_time: None,
            check_width: 0,
            check_height: 0,
        };

        checkmark.record_pixels();
        checkmark
    }

    pub fn start_drawing(&mut self) {
        self.enabled = true;
        self.progress = Frac::ZERO;
        self.last_drawn_check_progress = None;
        self.animation_state = AnimationState::Drawing;
        self.animation_start_time = None; // Will be set on first draw
    }

    pub fn reset(&mut self) {
        self.enabled = false;
        self.progress = Frac::ZERO;
        self.last_drawn_check_progress = None;
        self.animation_state = AnimationState::Idle;
    }

    pub fn is_complete(&self) -> bool {
        self.animation_state == AnimationState::Complete
    }

    pub fn drawing_started(&self) -> bool {
        matches!(
            self.animation_state,
            AnimationState::Complete | AnimationState::Drawing
        )
    }

    fn record_pixels(&mut self) {
        let mut recorder = PixelRecorder::new();

        // Define checkmark points for perfect right angle
        // Second segment is 2.2x the length of the first segment
        let width = self.width;

        // Calculate segment lengths to use full width
        // The checkmark total width = first_x + second_x
        // We want: (first_x + second_x) + margins = width
        // With stroke width 8, we need margin of 4 on each side
        let margin = 5i32; // Radius of cap circle + 1
        let available_width = width - (2 * margin as u32);

        // For 45-degree angles, x and y components are length / sqrt(2)
        // sqrt(2) ≈ 1.414213562373095
        // To avoid division, we multiply by inverse: 1/sqrt(2) ≈ 0.7071067811865476
        // As a fraction: 7071/10000
        let inv_sqrt_2 = Rat::from_ratio(7071, 10000);

        // With ratio of 2.2:1 for second:first segment
        // Total horizontal span = first_x + second_x = available_width
        // first_x = first_length * inv_sqrt_2
        // second_x = second_length * inv_sqrt_2 = 2.2 * first_length * inv_sqrt_2
        // So: first_length * inv_sqrt_2 * (1 + 2.2) = available_width
        // first_length * inv_sqrt_2 * 3.2 = available_width
        // first_length = available_width / (inv_sqrt_2 * 3.2)
        // Since inv_sqrt_2 ≈ 0.7071, inv_sqrt_2 * 3.2 ≈ 2.263
        // So first_length ≈ available_width / 2.263 ≈ available_width * 0.442
        let first_segment_length = (Rat::from_ratio(4420, 10000) * available_width).round();
        let second_segment_length = (Rat::from_ratio(22, 10) * first_segment_length).round();

        // First segment: 45 degrees down-right
        let first_x_offset = (inv_sqrt_2 * first_segment_length).round() as i32;
        let first_y_offset = (inv_sqrt_2 * first_segment_length).round() as i32;

        // Second segment: 45 degrees up-right (perpendicular to first)
        let second_x_offset = (inv_sqrt_2 * second_segment_length).round() as i32;
        let second_y_offset = (inv_sqrt_2 * second_segment_length).round() as i32;

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
        self.check_pixels.sort_unstable_by_key(|cp| cp.x);

        // Find the actual bounds of the recorded pixels
        if !self.check_pixels.is_empty() {
            let max_x = self.check_pixels.iter().map(|cp| cp.x).max().unwrap_or(0);
            let max_y = self
                .check_pixels
                .iter()
                .map(|cp| cp.y as u32)
                .max()
                .unwrap_or(0);
            self.check_width = (max_x + 1) as u32; // +1 because coordinates are 0-based
            self.check_height = max_y + 1;
        } else {
            // Fallback to calculated values
            self.check_width = self.width;
            // Approximate height based on width
            self.check_height = (self.width * 2) / 3;
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
                let elapsed = current_time.saturating_duration_since(start) as u32;
                self.progress = Frac::from_ratio(elapsed, CHECK_DURATION_MS as u32);

                if self.progress >= Frac::ONE {
                    self.progress = Frac::ONE;
                    self.animation_state = AnimationState::Complete;
                }
            }
            AnimationState::Complete => {}
        }
    }
}

impl<C: PixelColor> crate::DynWidget for Checkmark<C> {
    fn set_constraints(&mut self, _max_size: Size) {
        // Checkmark has a fixed size based on check_width and check_height
    }

    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: self.check_width,
            height: self.check_height,
            ..Default::default()
        }
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

    fn force_full_redraw(&mut self) {
        self.last_drawn_check_progress = None;
    }
}

impl<C: crate::WidgetColor> Widget for Checkmark<C> {
    type Color = C;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if self.enabled {
            self.update_animation(current_time);

            // Draw animation inline
            match self.animation_state {
                AnimationState::Idle => {}
                AnimationState::Drawing | AnimationState::Complete => {
                    let check_progress = self.progress;
                    let current_pixels =
                        (check_progress * self.check_pixels.len() as u32).round() as usize;
                    let last_pixels = if let Some(last_progress) = self.last_drawn_check_progress {
                        (last_progress * self.check_pixels.len() as u32).round() as usize
                    } else {
                        0
                    };

                    if current_pixels > last_pixels && current_pixels <= self.check_pixels.len() {
                        target.draw_iter(
                            self.check_pixels[last_pixels..current_pixels]
                                .iter()
                                .map(|cp| Pixel(cp.to_point(), self.color)),
                        )?;
                    }

                    self.last_drawn_check_progress = Some(check_progress);
                }
            }
        }
        Ok(())
    }
}
