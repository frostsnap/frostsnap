use super::Widget;
use crate::{
    compressed_point::CompressedPointWithCoverage, sdf, super_draw_target::SuperDrawTarget, Frac,
    Instant, Rat,
};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{PixelColor, Rgb565},
    prelude::*,
};

#[derive(PartialEq)]
pub struct Checkmark<C> {
    width: u32,
    color: C,
    bg_color: C,
    check_pixels: Vec<CompressedPointWithCoverage>,
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

impl Checkmark<Rgb565> {
    pub fn new(width: u32, color: Rgb565, bg_color: Rgb565) -> Self {
        let mut checkmark = Self {
            width,
            color,
            bg_color,
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

    pub fn set_color(&mut self, color: Rgb565) {
        self.color = color;
    }

    pub fn set_bg_color(&mut self, bg_color: Rgb565) {
        self.bg_color = bg_color;
    }

    pub fn start_drawing(&mut self) {
        self.enabled = true;
        self.progress = Frac::ZERO;
        self.last_drawn_check_progress = None;
        self.animation_state = AnimationState::Drawing;
        self.animation_start_time = None;
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
        #[allow(unused_imports)]
        use micromath::F32Ext as _;

        let width = self.width;
        let margin = 5i32;
        let available_width = width - (2 * margin as u32);

        let inv_sqrt_2 = Rat::from_ratio(7071, 10000);

        let first_segment_length = (Rat::from_ratio(4420, 10000) * available_width).round();
        let second_segment_length = (Rat::from_ratio(22, 10) * first_segment_length).round();

        let first_x_offset = (inv_sqrt_2 * first_segment_length).round() as i32;
        let first_y_offset = (inv_sqrt_2 * first_segment_length).round() as i32;

        let second_x_offset = (inv_sqrt_2 * second_segment_length).round() as i32;
        let second_y_offset = (inv_sqrt_2 * second_segment_length).round() as i32;

        let middle = Point::new(first_x_offset + margin, second_y_offset + margin);
        let left_start = Point::new(middle.x - first_x_offset, middle.y - first_y_offset);
        let right_end = Point::new(middle.x + second_x_offset, middle.y - second_y_offset);

        let stroke_radius = 4.0_f32; // half of stroke_width=8

        // Compute bounding box
        let min_x = 0i32;
        let min_y = 0i32;
        let max_x = (right_end.x + stroke_radius as i32 + 2).min(width as i32);
        let max_y = (middle.y + stroke_radius as i32 + 2) as i32;

        let ax = left_start.x as f32;
        let ay = left_start.y as f32;
        let bx = middle.x as f32;
        let by = middle.y as f32;
        let cx = right_end.x as f32;
        let cy = right_end.y as f32;

        // Record pixels with SDF-based coverage for both segments (capsule shapes).
        // For each pixel, compute distance to both line segments and take the minimum.
        let mut first_leg: Vec<CompressedPointWithCoverage> = Vec::new();
        let mut second_leg: Vec<CompressedPointWithCoverage> = Vec::new();

        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;

                let d1 = sdf_capsule(px, py, ax, ay, bx, by, stroke_radius);
                let d2 = sdf_capsule(px, py, bx, by, cx, cy, stroke_radius);

                // Take the closer segment
                let d = d1.min(d2);
                let cov = sdf::sdf_coverage(d);
                if cov <= 0.0 {
                    continue;
                }
                let level = sdf::coverage_to_gray4(cov);
                if level == 0 {
                    continue;
                }

                let cp = CompressedPointWithCoverage::new(Point::new(x, y), level);

                if d1 <= d2 {
                    first_leg.push(cp);
                } else {
                    second_leg.push(cp);
                }
            }
        }

        // Sort each leg by projection onto the stroke direction for path-following reveal.
        // First leg direction: left_start → middle
        let d1x = bx - ax;
        let d1y = by - ay;
        first_leg.sort_unstable_by(|a, b| {
            let pa = a.x as f32 * d1x + a.y as f32 * d1y;
            let pb = b.x as f32 * d1x + b.y as f32 * d1y;
            pa.partial_cmp(&pb).unwrap_or(core::cmp::Ordering::Equal)
        });
        // Second leg direction: middle → right_end
        let d2x = cx - bx;
        let d2y = cy - by;
        second_leg.sort_unstable_by(|a, b| {
            let pa = a.x as f32 * d2x + a.y as f32 * d2y;
            let pb = b.x as f32 * d2x + b.y as f32 * d2y;
            pa.partial_cmp(&pb).unwrap_or(core::cmp::Ordering::Equal)
        });

        self.check_pixels = first_leg;
        self.check_pixels.extend(second_leg);

        // Compute bounds
        if !self.check_pixels.is_empty() {
            let max_x = self.check_pixels.iter().map(|cp| cp.x).max().unwrap_or(0);
            let max_y = self
                .check_pixels
                .iter()
                .map(|cp| cp.y as u32)
                .max()
                .unwrap_or(0);
            self.check_width = (max_x + 1) as u32;
            self.check_height = max_y + 1;
        } else {
            self.check_width = self.width;
            self.check_height = (self.width * 2) / 3;
        }
    }

    fn update_animation(&mut self, current_time: crate::Instant) {
        if !self.enabled {
            return;
        }

        const CHECK_DURATION_MS: u64 = 400;

        if self.animation_start_time.is_none() {
            self.animation_start_time = Some(current_time);
        }

        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing => {
                let start = self.animation_start_time.unwrap();
                let elapsed = current_time.saturating_duration_since(start) as u32;
                let linear = Frac::from_ratio(elapsed, CHECK_DURATION_MS as u32);

                // Ease-out: progress = 1 - (1 - t)^2
                let inv = Frac::ONE - linear;
                self.progress = Frac::ONE - inv * inv;

                if linear >= Frac::ONE {
                    self.progress = Frac::ONE;
                    self.animation_state = AnimationState::Complete;
                }
            }
            AnimationState::Complete => {}
        }
    }
}

/// SDF for a capsule (line segment with rounded ends).
/// Returns signed distance: negative inside, positive outside.
#[inline]
fn sdf_capsule(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, radius: f32) -> f32 {
    #[allow(unused_imports)]
    use micromath::F32Ext as _;

    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let dot_ab = bax * bax + bay * bay;
    // Project point onto line, clamped to [0, 1]
    let mut t = (pax * bax + pay * bay) / dot_ab;
    if t < 0.0 {
        t = 0.0;
    } else if t > 1.0 {
        t = 1.0;
    }
    let dx = pax - bax * t;
    let dy = pay - bay * t;
    (dx * dx + dy * dy).sqrt() - radius
}

impl<C: PixelColor> crate::DynWidget for Checkmark<C> {
    fn set_constraints(&mut self, _max_size: Size) {}

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

impl Widget for Checkmark<Rgb565> {
    type Color = Rgb565;

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
                        let lut = sdf::build_aa_lut(self.color, self.bg_color);
                        target.draw_iter(
                            self.check_pixels[last_pixels..current_pixels]
                                .iter()
                                .map(|cp| Pixel(cp.to_point(), lut[cp.coverage as usize])),
                        )?;
                    }

                    self.last_drawn_check_progress = Some(check_progress);
                }
            }
        }
        Ok(())
    }
}
