use super::{coverage_from_distance, isqrt_distance, SCALE};
use crate::widget_color::ColorInterpolate;
use crate::{Frac, Rat};
use embedded_graphics::{draw_target::DrawTarget, prelude::*};

pub struct AACircle<C: ColorInterpolate> {
    center: Point,
    radius: u32,
    stroke_width: u32,
    fill_color: C,
    stroke_color: C,
    bg_color: C,
}

impl<C: ColorInterpolate> AACircle<C> {
    pub fn new(
        center: Point,
        radius: u32,
        stroke_width: u32,
        fill_color: C,
        stroke_color: C,
        bg_color: C,
    ) -> Self {
        Self {
            center,
            radius,
            stroke_width,
            fill_color,
            stroke_color,
            bg_color,
        }
    }

    pub fn pixels(&self) -> AACircleIter<C> {
        let outer_r = self.radius;
        let inner_r = outer_r.saturating_sub(self.stroke_width);
        let outer_r_scaled = outer_r as i64 * SCALE;
        let inner_r_scaled = inner_r as i64 * SCALE;

        let cx = self.center.x;
        let cy = self.center.y;
        let r = outer_r as i32;
        // +1 for the AA fringe pixel
        let min_x = cx - r - 1;
        let max_x = cx + r + 1;
        let min_y = cy - r - 1;
        let max_y = cy + r + 1;

        let cx_scaled = cx as i64 * SCALE + SCALE / 2;
        let cy_scaled = cy as i64 * SCALE + SCALE / 2;

        AACircleIter {
            cx_scaled,
            cy_scaled,
            outer_r_scaled,
            inner_r_scaled,
            has_stroke: self.stroke_width > 0,
            x: min_x,
            y: min_y,
            min_x,
            max_x,
            max_y,
            fill_color: self.fill_color,
            stroke_color: self.stroke_color,
            bg_color: self.bg_color,
        }
    }
}

impl<C: ColorInterpolate> Drawable for AACircle<C> {
    type Color = C;
    type Output = ();

    fn draw<D: DrawTarget<Color = C>>(&self, target: &mut D) -> Result<(), D::Error> {
        target.draw_iter(self.pixels())
    }
}

pub struct AACircleIter<C: ColorInterpolate> {
    cx_scaled: i64,
    cy_scaled: i64,
    outer_r_scaled: i64,
    inner_r_scaled: i64,
    has_stroke: bool,
    x: i32,
    y: i32,
    min_x: i32,
    max_x: i32,
    max_y: i32,
    fill_color: C,
    stroke_color: C,
    bg_color: C,
}

impl<C: ColorInterpolate> Iterator for AACircleIter<C> {
    type Item = embedded_graphics::Pixel<C>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.y > self.max_y {
                return None;
            }

            let x = self.x;
            let y = self.y;

            self.x += 1;
            if self.x > self.max_x {
                self.x = self.min_x;
                self.y += 1;
            }

            let px_scaled = x as i64 * SCALE + SCALE / 2;
            let py_scaled = y as i64 * SCALE + SCALE / 2;
            let dx = px_scaled - self.cx_scaled;
            let dy = py_scaled - self.cy_scaled;

            let outer_dist = isqrt_distance(dx, dy, self.outer_r_scaled);
            let shape_cov = coverage_from_distance(outer_dist);

            if shape_cov == Frac::ZERO {
                continue;
            }

            let color = if self.has_stroke {
                let inner_dist = isqrt_distance(dx, dy, self.inner_r_scaled);
                let fill_cov = coverage_from_distance(inner_dist);
                let stroke_cov = Frac::new((shape_cov.as_rat() - fill_cov.as_rat()).max(Rat::ZERO));

                // Blend fill and stroke within the shape, then composite over bg.
                // This avoids the dark-pixel artifacts at the stroke/fill boundary
                // that two-step interpolation (bg→fill then result→stroke) causes.
                let stroke_ratio = if shape_cov.as_rat().0 > 0 {
                    Frac::new(Rat::from_ratio(stroke_cov.as_rat().0, shape_cov.as_rat().0))
                } else {
                    Frac::ZERO
                };
                let shape_color = self.fill_color.interpolate(self.stroke_color, stroke_ratio);
                self.bg_color.interpolate(shape_color, shape_cov)
            } else {
                self.bg_color.interpolate(self.fill_color, shape_cov)
            };

            return Some(embedded_graphics::Pixel(Point::new(x, y), color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::pixelcolor::RgbColor;

    #[test]
    fn sanity() {
        let bg = Rgb565::BLACK;
        let fill = Rgb565::new(0, 31, 0);
        let stroke = Rgb565::WHITE;
        let center = Point::new(50, 50);
        let radius = 48;

        let circle = AACircle::new(center, radius, 2, fill, stroke, bg);
        let pixels: alloc::vec::Vec<_> = circle.pixels().collect();

        assert!(pixels.len() > 100);

        let mut seen = alloc::collections::BTreeSet::new();
        let mut partial = 0;
        for &embedded_graphics::Pixel(point, color) in &pixels {
            let dx = (point.x - center.x).abs();
            let dy = (point.y - center.y).abs();
            assert!(
                dx <= radius as i32 + 2 && dy <= radius as i32 + 2,
                "pixel {:?} too far from center",
                point
            );
            assert!(seen.insert((point.x, point.y)), "duplicate at {:?}", point);
            if color != bg && color != fill && color != stroke {
                partial += 1;
            }
        }
        assert!(partial > 0, "expected AA partial-coverage pixels");

        let no_stroke = AACircle::new(center, radius, 0, fill, fill, bg);
        let no_stroke_pixels: alloc::vec::Vec<_> = no_stroke.pixels().collect();
        assert!(no_stroke_pixels.len() > 100);
    }
}
