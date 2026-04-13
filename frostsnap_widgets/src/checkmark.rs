use super::{DynWidget, Widget};
use crate::{
    aa::{coverage_from_distance, SCALE},
    animation_speed::AnimationSpeed,
    super_draw_target::SuperDrawTarget,
    Frac, Instant, Rat,
};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
};

use crate::compressed_point::CoveragePoint;

#[derive(Clone, PartialEq)]
pub struct Checkmark<C> {
    width: u32,
    color: C,
    check_pixels: Vec<CoveragePoint>,
    progress: Frac,
    last_drawn_check_progress: Option<Frac>,
    animation_state: AnimationState,
    animation_speed: AnimationSpeed,
    animation_duration_ms: u32,
    enabled: bool,
    animation_start_time: Option<Instant>,
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
        Self {
            width,
            color,
            check_pixels: Vec::new(),
            progress: Frac::ZERO,
            last_drawn_check_progress: None,
            animation_state: AnimationState::Idle,
            animation_speed: AnimationSpeed::EaseOut,
            animation_duration_ms: 400,
            enabled: false,
            animation_start_time: None,
            check_width: 0,
            check_height: 0,
        }
    }

    pub fn set_color(&mut self, color: C) {
        self.color = color;
        self.force_full_redraw();
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

        let stroke_radius_scaled = 4 * SCALE;

        let max_x = (right_end.x + 6).min(width as i32);
        let max_y = middle.y + 6;

        // 🧮 scaled endpoints for integer SDF
        let ax = left_start.x as i64 * SCALE + SCALE / 2;
        let ay = left_start.y as i64 * SCALE + SCALE / 2;
        let bx = middle.x as i64 * SCALE + SCALE / 2;
        let by = middle.y as i64 * SCALE + SCALE / 2;
        let cx = right_end.x as i64 * SCALE + SCALE / 2;
        let cy = right_end.y as i64 * SCALE + SCALE / 2;

        let mut first_leg: Vec<CoveragePoint> = Vec::new();
        let mut second_leg: Vec<CoveragePoint> = Vec::new();

        for y in 0..max_y {
            for x in 0..max_x {
                let px = x as i64 * SCALE + SCALE / 2;
                let py = y as i64 * SCALE + SCALE / 2;

                let d1 = capsule_distance_scaled(px, py, ax, ay, bx, by, stroke_radius_scaled);
                let d2 = capsule_distance_scaled(px, py, bx, by, cx, cy, stroke_radius_scaled);

                let d = d1.min(d2);
                let cov = coverage_from_distance(d);
                if cov == Frac::ZERO {
                    continue;
                }
                let level = (cov * 15u32).round() as u8;
                if level == 0 {
                    continue;
                }

                let cp = CoveragePoint {
                    x: x as u8,
                    y: y as u16,
                    coverage: level,
                };

                if d1 <= d2 {
                    first_leg.push(cp);
                } else {
                    second_leg.push(cp);
                }
            }
        }

        // Sort each leg by projection onto the stroke direction for path-following reveal.
        let d1x = bx - ax;
        let d1y = by - ay;
        first_leg.sort_unstable_by_key(|p| p.x as i64 * d1x + p.y as i64 * d1y);
        let d2x = cx - bx;
        let d2y = cy - by;
        second_leg.sort_unstable_by_key(|p| p.x as i64 * d2x + p.y as i64 * d2y);

        self.check_pixels = first_leg;
        self.check_pixels.extend(second_leg);

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

    fn update_animation(&mut self, current_time: Instant) {
        if !self.enabled {
            return;
        }

        if self.animation_start_time.is_none() {
            self.animation_start_time = Some(current_time);
        }

        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing => {
                let start = self.animation_start_time.unwrap();
                let elapsed = current_time.saturating_duration_since(start) as u32;
                let linear = Frac::from_ratio(elapsed, self.animation_duration_ms);
                self.progress = self.animation_speed.apply(linear);

                if self.progress >= Frac::ONE {
                    self.animation_state = AnimationState::Complete;
                }
            }
            AnimationState::Complete => {}
        }
    }
}

/// Integer SDF for a capsule (line segment with rounded ends).
/// All inputs and output in SCALE units. Returns signed distance.
#[inline]
fn capsule_distance_scaled(
    px: i64,
    py: i64,
    ax: i64,
    ay: i64,
    bx: i64,
    by: i64,
    radius_scaled: i64,
) -> i64 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let dot_ab = bax * bax + bay * bay;
    if dot_ab == 0 {
        return (pax * pax + pay * pay).unsigned_abs().isqrt() as i64 - radius_scaled;
    }
    // 🎯 t = clamp(dot(pa, ba) / dot(ba, ba), 0, 1) in fixed point
    let dot_pa_ba = pax * bax + pay * bay;
    let t_numer = dot_pa_ba.clamp(0, dot_ab);
    // nearest point on segment: a + t * ba, but keep in numerator/denominator form
    // to avoid division. dx,dy = (pa - t*ba) * dot_ab
    let nearest_x = pax * dot_ab - bax * t_numer;
    let nearest_y = pay * dot_ab - bay * t_numer;
    // scale down by sqrt(dot_ab) to avoid overflow in squaring
    let len_ab = dot_ab.unsigned_abs().isqrt() as i64;
    let dx = nearest_x / len_ab;
    let dy = nearest_y / len_ab;
    (dx * dx + dy * dy).unsigned_abs().isqrt() as i64 / len_ab - radius_scaled
}

impl<C: PixelColor> DynWidget for Checkmark<C> {
    fn set_constraints(&mut self, _max_size: Size) {
        if self.check_pixels.is_empty() {
            self.record_pixels();
        }
    }

    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: self.check_width,
            height: self.check_height,
            ..Default::default()
        }
    }

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
                        let color = self.color;
                        let bg = target.background_color();
                        target.draw_iter(
                            self.check_pixels[last_pixels..current_pixels]
                                .iter()
                                .map(|cp| {
                                    let frac = Frac::from_ratio(cp.coverage as u32, 15);
                                    let blended = bg.interpolate(color, frac);
                                    Pixel(Point::new(cp.x as i32, cp.y as i32), blended)
                                }),
                        )?;
                    }

                    self.last_drawn_check_progress = Some(check_progress);
                }
            }
        }
        Ok(())
    }
}
