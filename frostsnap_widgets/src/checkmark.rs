use super::{DynWidget, Widget};
use crate::{super_draw_target::SuperDrawTarget, Frac, Instant, Rat};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
};

/// A checkmark pixel with SDF coverage for anti-aliased rendering.
#[derive(Clone, Copy, PartialEq)]
struct CoveragePoint {
    x: u8,
    y: u16,
    /// Coverage level 1-15 (0 means invisible, not stored).
    coverage: u8,
}

#[derive(Clone, PartialEq)]
pub struct Checkmark<C> {
    width: u32,
    color: C,
    bg_color: Option<C>,
    check_pixels: Vec<CoveragePoint>,
    progress: Frac,
    last_drawn_check_progress: Option<Frac>,
    animation_state: AnimationState,
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
            bg_color: None,
            check_pixels: Vec::new(),
            progress: Frac::ZERO,
            last_drawn_check_progress: None,
            animation_state: AnimationState::Idle,
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

    /// Set the background color for AA blending. Call this before drawing
    /// when the checkmark is rendered on top of a known solid color.
    pub fn set_bg_color(&mut self, bg_color: C) {
        self.bg_color = Some(bg_color);
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

        let stroke_radius = 4.0_f32;

        let max_x = (right_end.x + stroke_radius as i32 + 2).min(width as i32);
        let max_y = middle.y + stroke_radius as i32 + 2;

        let ax = left_start.x as f32;
        let ay = left_start.y as f32;
        let bx = middle.x as f32;
        let by = middle.y as f32;
        let cx = right_end.x as f32;
        let cy = right_end.y as f32;

        let mut first_leg: Vec<CoveragePoint> = Vec::new();
        let mut second_leg: Vec<CoveragePoint> = Vec::new();

        for y in 0..max_y {
            for x in 0..max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;

                let d1 = sdf_capsule(px, py, ax, ay, bx, by, stroke_radius);
                let d2 = sdf_capsule(px, py, bx, by, cx, cy, stroke_radius);

                let d = d1.min(d2);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                let level = (cov * 15.0 + 0.5) as u8;
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
        first_leg.sort_unstable_by(|a, b| {
            let pa = a.x as f32 * d1x + a.y as f32 * d1y;
            let pb = b.x as f32 * d1x + b.y as f32 * d1y;
            pa.partial_cmp(&pb).unwrap_or(core::cmp::Ordering::Equal)
        });
        let d2x = cx - bx;
        let d2y = cy - by;
        second_leg.sort_unstable_by(|a, b| {
            let pa = a.x as f32 * d2x + a.y as f32 * d2y;
            let pb = b.x as f32 * d2x + b.y as f32 * d2y;
            pa.partial_cmp(&pb).unwrap_or(core::cmp::Ordering::Equal)
        });

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
    let t = ((pax * bax + pay * bay) / dot_ab).clamp(0.0, 1.0);
    let dx = pax - bax * t;
    let dy = pay - bay * t;
    (dx * dx + dy * dy).sqrt() - radius
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

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
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
                        let bg = self.bg_color.unwrap_or_else(|| target.background_color());
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
