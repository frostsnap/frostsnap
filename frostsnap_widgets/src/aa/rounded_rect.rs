use super::{coverage_from_distance, isqrt_distance, SCALE};
use crate::widget_color::ColorInterpolate;
use crate::Frac;
use embedded_graphics::{
    draw_target::DrawTarget,
    prelude::*,
    primitives::{Rectangle, StrokeAlignment},
};

pub struct AARoundedRectangle<C: ColorInterpolate> {
    rect: Rectangle,
    corner_radius: u32,
    fill_color: Option<C>,
    border_color: Option<C>,
    border_width: u32,
    background_color: C,
    stroke_alignment: StrokeAlignment,
}

impl<C: ColorInterpolate> AARoundedRectangle<C> {
    pub fn new(rect: Rectangle, background_color: C) -> Self {
        Self {
            rect,
            corner_radius: 0,
            fill_color: None,
            border_color: None,
            border_width: 0,
            background_color,
            stroke_alignment: StrokeAlignment::Inside,
        }
    }

    pub fn with_corner_radius(mut self, radius: u32) -> Self {
        self.corner_radius = radius;
        self
    }

    pub fn with_fill(mut self, color: C) -> Self {
        self.fill_color = Some(color);
        self
    }

    pub fn with_border(mut self, color: C, width: u32) -> Self {
        self.border_color = Some(color);
        self.border_width = width;
        self
    }

    pub fn with_stroke_alignment(mut self, alignment: StrokeAlignment) -> Self {
        self.stroke_alignment = alignment;
        self
    }

    pub fn pixels(&self) -> impl Iterator<Item = embedded_graphics::Pixel<C>> + '_ {
        let bw = if self.border_color.is_some() {
            self.border_width
        } else {
            0
        };
        let bg = self.background_color;
        let border_color = self.border_color.unwrap_or(bg);
        let inner = self.fill_color.unwrap_or(bg);

        // Adjust geometry based on stroke alignment
        let (iter_w, iter_h, iter_cr, pixel_offset) = match self.stroke_alignment {
            StrokeAlignment::Inside => (
                self.rect.size.width,
                self.rect.size.height,
                self.corner_radius,
                Point::zero(),
            ),
            StrokeAlignment::Outside => (
                self.rect.size.width + 2 * bw,
                self.rect.size.height + 2 * bw,
                self.corner_radius + bw,
                Point::new(-(bw as i32), -(bw as i32)),
            ),
            StrokeAlignment::Center => (
                self.rect.size.width + bw,
                self.rect.size.height + bw,
                self.corner_radius + bw / 2,
                Point::new(-(bw as i32) / 2, -(bw as i32) / 2),
            ),
        };

        let offset = self.rect.top_left + pixel_offset;

        AARoundedRectIter::new(iter_w, iter_h, iter_cr, bw, border_color, inner, bg)
            .with_fill(self.fill_color)
            .map(move |embedded_graphics::Pixel(p, c)| embedded_graphics::Pixel(p + offset, c))
    }
}

/// Precomputed geometry for a rounded rectangle's SDF calculations.
#[derive(Clone, Copy)]
struct RoundedRectGeom {
    w: u32,
    h: u32,
    cr: u32,
    bw: u32,
    cr_scaled: i64,
    inner_cr: u32,
    inner_cr_scaled: i64,
}

impl RoundedRectGeom {
    fn new(w: u32, h: u32, cr: u32, bw: u32) -> Self {
        let cr = cr.min(w / 2).min(h / 2);
        let inner_cr = cr.saturating_sub(bw);
        Self {
            w,
            h,
            cr,
            bw,
            cr_scaled: cr as i64 * SCALE,
            inner_cr,
            inner_cr_scaled: inner_cr as i64 * SCALE,
        }
    }

    /// Compute outer and border coverage for a pixel.
    /// Returns `(outer_coverage, border_coverage)`.
    /// For non-corner pixels, both are `Frac::ONE`.
    fn coverages(&self, row: u32, col: u32) -> (Frac, Frac) {
        if self.in_corner(row, col) {
            let (cx_offset, cy_offset) = self.corner_offsets(row, col);
            let outer =
                coverage_from_distance(isqrt_distance(cx_offset, cy_offset, self.cr_scaled));
            if outer == Frac::ZERO {
                return (Frac::ZERO, Frac::ZERO);
            }
            let inner = self.inner_corner_coverage(row, col);
            let border = Frac::new((outer.as_rat() - inner.as_rat()).max(crate::Rat::ZERO));
            (outer, border)
        } else {
            (Frac::ONE, Frac::ONE)
        }
    }

    /// Blend a border pixel color from its coverage values.
    fn border_pixel_color<C: ColorInterpolate>(
        &self,
        row: u32,
        col: u32,
        border_color: C,
        inner_color: C,
        outer_color: C,
    ) -> C {
        if !self.in_corner(row, col) {
            return border_color;
        }
        let (shape_cov, border_cov) = self.coverages(row, col);
        // Blend fill and border within the shape, then composite over background.
        // This avoids dark-pixel artifacts at the border/fill boundary.
        let border_ratio = if shape_cov > Frac::ZERO {
            Frac::new(border_cov / shape_cov)
        } else {
            Frac::ZERO
        };
        let shape_color = inner_color.interpolate(border_color, border_ratio);
        outer_color.interpolate(shape_color, shape_cov)
    }

    /// Blend a fill pixel color in a corner region.
    fn fill_pixel_color<C: ColorInterpolate>(
        &self,
        row: u32,
        col: u32,
        fill_color: C,
        outer_color: C,
    ) -> C {
        let (outer_cov, _) = self.coverages(row, col);
        outer_color.interpolate(fill_color, outer_cov)
    }

    #[inline]
    fn in_corner(&self, row: u32, col: u32) -> bool {
        (row < self.cr || row >= self.h - self.cr) && (col < self.cr || col >= self.w - self.cr)
    }

    #[inline]
    fn corner_offsets(&self, row: u32, col: u32) -> (i64, i64) {
        let px = col as i64 * SCALE + SCALE / 2;
        let py = row as i64 * SCALE + SCALE / 2;

        let cy_offset = if row < self.cr {
            py - self.cr_scaled
        } else {
            py - (self.h as i64 * SCALE - self.cr_scaled)
        };
        let cx_offset = if col < self.cr {
            px - self.cr_scaled
        } else {
            px - (self.w as i64 * SCALE - self.cr_scaled)
        };

        (cx_offset, cy_offset)
    }

    #[inline]
    fn inner_corner_coverage(&self, row: u32, col: u32) -> Frac {
        let inside_border =
            row >= self.bw && row < self.h - self.bw && col >= self.bw && col < self.w - self.bw;

        if self.inner_cr == 0 {
            return if inside_border { Frac::ONE } else { Frac::ZERO };
        }

        let in_inner_corner_y =
            row < self.bw + self.inner_cr || row >= self.h - self.bw - self.inner_cr;
        let in_inner_corner_x =
            col < self.bw + self.inner_cr || col >= self.w - self.bw - self.inner_cr;

        if !(in_inner_corner_y && in_inner_corner_x) {
            return if inside_border { Frac::ONE } else { Frac::ZERO };
        }

        let px = col as i64 * SCALE + SCALE / 2;
        let py = row as i64 * SCALE + SCALE / 2;

        let inner_cy = if row < self.bw + self.inner_cr {
            py - (self.bw as i64 + self.inner_cr as i64) * SCALE
        } else {
            py - ((self.h - self.bw) as i64 - self.inner_cr as i64) * SCALE
        };

        let inner_cx = if col < self.bw + self.inner_cr {
            px - (self.bw as i64 + self.inner_cr as i64) * SCALE
        } else {
            px - ((self.w - self.bw) as i64 - self.inner_cr as i64) * SCALE
        };

        coverage_from_distance(isqrt_distance(inner_cx, inner_cy, self.inner_cr_scaled))
    }

    fn corner_bounds(&self, corner_idx: u8) -> (u32, u32, u32, u32) {
        match corner_idx {
            0 => (0, self.cr, 0, self.cr),                             // TL
            1 => (0, self.cr, self.w - self.cr, self.w),               // TR
            2 => (self.h - self.cr, self.h, self.w - self.cr, self.w), // BR
            3 => (self.h - self.cr, self.h, 0, self.cr),               // BL
            _ => unreachable!(),
        }
    }
}

/// Tracks the fill iteration phase.
#[derive(Clone)]
enum FillPhase {
    Border,
    CornerFill { corner_idx: u8, row: u32, col: u32 },
    InteriorFill { row: u32, col: u32 },
    Done,
}

/// Iterates pixels of an AA rounded rectangle.
///
/// Without fill: yields only border pixels in clockwise perimeter order.
/// With fill: yields border pixels first, then fill pixels.
///
/// Implements `DoubleEndedIterator` for the border phase (fill is forward-only).
///
/// Border segments in clockwise order starting from the top-left corner:
/// 0: TL arc, 1: top edge (L→R), 2: TR arc, 3: right edge (T→B),
/// 4: BR arc, 5: bottom edge (R→L), 6: BL arc, 7: left edge (B→T)
#[derive(Clone)]
pub struct AARoundedRectIter<C: ColorInterpolate> {
    geom: RoundedRectGeom,
    seg_sizes: [u32; 8],
    front: i64,
    back: i64,
    origin_offset: u32,
    border_color: C,
    inner_color: C,
    outer_color: C,
    fill_color: Option<C>,
    fill_phase: FillPhase,
}

impl<C: ColorInterpolate> AARoundedRectIter<C> {
    pub fn new(
        w: u32,
        h: u32,
        cr: u32,
        bw: u32,
        border_color: C,
        inner_color: C,
        outer_color: C,
    ) -> Self {
        let geom = RoundedRectGeom::new(w, h, cr, bw);
        let bw = geom.bw;
        let cr = geom.cr;

        let edge_h = geom.w.saturating_sub(2 * cr);
        let edge_v = geom.h.saturating_sub(2 * cr);

        let seg_sizes = [
            cr * cr,     // 0: TL arc
            edge_h * bw, // 1: top edge
            cr * cr,     // 2: TR arc
            edge_v * bw, // 3: right edge
            cr * cr,     // 4: BR arc
            edge_h * bw, // 5: bottom edge
            cr * cr,     // 6: BL arc
            edge_v * bw, // 7: left edge
        ];

        let total: u32 = seg_sizes.iter().sum();

        Self {
            geom,
            seg_sizes,
            front: 0,
            back: total as i64 - 1,
            origin_offset: 0,
            border_color,
            inner_color,
            outer_color,
            fill_color: None,
            fill_phase: FillPhase::Border,
        }
    }

    pub fn with_fill(mut self, fill_color: Option<C>) -> Self {
        self.fill_color = fill_color;
        self
    }

    pub fn set_border_color(&mut self, color: C) {
        self.border_color = color;
    }

    /// Shift the iterator's origin so that logical index 0 starts at the given
    /// perimeter fraction. Raw ranges and DoubleEndedIterator then operate
    /// relative to this origin.
    pub fn with_origin(mut self, origin: Frac) -> Self {
        self.origin_offset = self.frac_to_raw(origin);
        self
    }

    /// Restrict iteration to pixels in the perimeter fraction range, including
    /// the pixel that `end` lands on.
    /// If `start > end` in logical space, the range wraps around the perimeter.
    pub fn with_frac_range(self, start: Frac, end: Frac) -> Self {
        let raw_start = self.actual_to_logical(self.frac_to_raw(start));
        let mut raw_end = self.actual_to_logical(self.frac_to_raw(end));
        if raw_end <= raw_start && start != end {
            // ↻ wrapping range
            raw_end += self.total_raw_pixels();
        }
        self.with_raw_range(raw_start, raw_end + 1)
    }

    fn with_raw_range(mut self, start: u32, end: u32) -> Self {
        self.front = start as i64;
        self.back = end as i64 - 1;
        self
    }

    fn logical_to_actual(&self, logical: u32) -> u32 {
        let total = self.total_raw_pixels();
        if total == 0 {
            return 0;
        }
        (logical + self.origin_offset) % total
    }

    fn actual_to_logical(&self, actual: u32) -> u32 {
        let total = self.total_raw_pixels();
        if total == 0 {
            return 0;
        }
        (actual + total - self.origin_offset) % total
    }

    fn total_raw_pixels(&self) -> u32 {
        self.seg_sizes.iter().sum()
    }

    fn segment_start(&self, seg: u8) -> u32 {
        self.seg_sizes[..seg as usize].iter().sum()
    }

    #[allow(dead_code)]
    fn top_center_raw(&self) -> u32 {
        self.segment_start(1) + self.seg_sizes[1] / 2
    }

    #[allow(dead_code)]
    fn bottom_center_raw(&self) -> u32 {
        self.segment_start(5) + self.seg_sizes[5] / 2
    }

    /// Perimeter fraction for the midpoint of the top edge.
    pub fn top_center(&self) -> Frac {
        let perims = self.seg_perimeters_scaled();
        let total: u64 = perims.iter().sum();
        let at_top_center: u64 = perims[0] + perims[1] / 2;
        Frac::from_ratio(at_top_center as u32, total as u32)
    }

    /// Perimeter fraction for the midpoint of the bottom edge.
    pub fn bottom_center(&self) -> Frac {
        let perims = self.seg_perimeters_scaled();
        let total: u64 = perims.iter().sum();
        let at_bottom_center: u64 = perims[..5].iter().sum::<u64>() + perims[5] / 2;
        Frac::from_ratio(at_bottom_center as u32, total as u32)
    }

    /// Convert a perimeter Frac (0..1) to a raw flat index.
    pub fn frac_to_raw(&self, frac: Frac) -> u32 {
        let perims = self.seg_perimeters_scaled();
        let total: u64 = perims.iter().sum();
        if total == 0 {
            return 0;
        }

        let target = (frac * total as u32).floor() as u64;

        let mut cumulative = 0u64;
        for i in 0..8u8 {
            let seg_perim = perims[i as usize];
            if target < cumulative + seg_perim {
                let within = target - cumulative;
                let seg_raw = self.seg_sizes[i as usize] as u64;
                let raw_within = if seg_perim > 0 {
                    (within * seg_raw) / seg_perim
                } else {
                    0
                };
                return self.segment_start(i) + raw_within as u32;
            }
            cumulative += seg_perim;
        }
        self.total_raw_pixels()
    }

    /// Perimeter lengths for each segment, scaled by 226 (= 2 * 113) to keep integer math.
    /// Arc segments use π ≈ 355/113, so quarter-arc * 226 = 355 * cr.
    fn seg_perimeters_scaled(&self) -> [u64; 8] {
        const PI_NUM: u64 = 355;
        const DENOM: u64 = 226;
        let cr = self.geom.cr as u64;
        let edge_h = self.geom.w.saturating_sub(2 * self.geom.cr) as u64;
        let edge_v = self.geom.h.saturating_sub(2 * self.geom.cr) as u64;
        [
            PI_NUM * cr,    // TL arc
            DENOM * edge_h, // top edge
            PI_NUM * cr,    // TR arc
            DENOM * edge_v, // right edge
            PI_NUM * cr,    // BR arc
            DENOM * edge_h, // bottom edge
            PI_NUM * cr,    // BL arc
            DENOM * edge_v, // left edge
        ]
    }

    fn global_to_seg(&self, global: u32) -> (u8, u32) {
        let mut offset = 0u32;
        for i in 0..8u8 {
            let size = self.seg_sizes[i as usize];
            if global < offset + size {
                return (i, global - offset);
            }
            offset += size;
        }
        unreachable!()
    }

    fn segment_bounds(&self, seg: u8) -> (u32, u32, u32, u32) {
        let g = &self.geom;
        match seg {
            0 => (0, g.cr, 0, g.cr),                  // TL arc
            1 => (0, g.bw, g.cr, g.w - g.cr),         // top edge
            2 => (0, g.cr, g.w - g.cr, g.w),          // TR arc
            3 => (g.cr, g.h - g.cr, g.w - g.bw, g.w), // right edge
            4 => (g.h - g.cr, g.h, g.w - g.cr, g.w),  // BR arc
            5 => (g.h - g.bw, g.h, g.cr, g.w - g.cr), // bottom edge
            6 => (g.h - g.cr, g.h, 0, g.cr),          // BL arc
            7 => (g.cr, g.h - g.cr, 0, g.bw),         // left edge
            _ => unreachable!(),
        }
    }

    /// Map a flat index to (row, col) in anti-diagonal order within an n×n grid.
    /// Anti-diagonals are lines where `row + col = d`, swept from d=0 to d=2*(n-1).
    fn antidiag_to_rowcol(n: u32, flat: u32) -> (u32, u32) {
        let mut remaining = flat;
        let max_d = 2 * (n - 1);
        for d in 0..=max_d {
            let diag_len = (d + 1).min(n).min(max_d + 1 - d);
            if remaining < diag_len {
                let row = if d < n {
                    remaining
                } else {
                    d - n + 1 + remaining
                };
                let col = d - row;
                return (row, col);
            }
            remaining -= diag_len;
        }
        (n - 1, n - 1)
    }

    fn flat_to_rowcol(&self, seg: u8, flat: u32) -> (u32, u32) {
        let (rs, re, cs, ce) = self.segment_bounds(seg);
        let rows = re - rs;
        let cols = ce - cs;
        if rows == 0 || cols == 0 {
            return (rs, cs);
        }

        match seg {
            // Straight edges
            1 => {
                let col_idx = flat / rows;
                let row_idx = flat % rows;
                (rs + row_idx, cs + col_idx)
            }
            3 => {
                let row_idx = flat / cols;
                let col_idx = flat % cols;
                (rs + row_idx, cs + col_idx)
            }
            5 => {
                let col_idx = flat / rows;
                let row_idx = flat % rows;
                (rs + row_idx, ce - 1 - col_idx)
            }
            7 => {
                let row_idx = flat / cols;
                let col_idx = flat % cols;
                (re - 1 - row_idx, cs + col_idx)
            }
            // Corner arcs: anti-diagonal sweep for smooth angular progress
            0 => {
                let (r, c) = Self::antidiag_to_rowcol(cols, flat);
                (rs + r, ce - 1 - c)
            }
            2 => {
                let (r, c) = Self::antidiag_to_rowcol(cols, flat);
                (rs + r, cs + c)
            }
            4 => {
                let (r, c) = Self::antidiag_to_rowcol(cols, flat);
                (rs + r, ce - 1 - c)
            }
            6 => {
                let (r, c) = Self::antidiag_to_rowcol(cols, flat);
                (re - 1 - r, cs + c)
            }
            _ => unreachable!(),
        }
    }

    fn next_border(&mut self) -> Option<embedded_graphics::Pixel<C>> {
        loop {
            if self.front > self.back {
                return None;
            }

            let logical = self.front as u32;
            self.front += 1;
            let global = self.logical_to_actual(logical);

            let (seg, local) = self.global_to_seg(global);
            let (row, col) = self.flat_to_rowcol(seg, local);
            let cov = self.geom.coverages(row, col).1;

            if cov > Frac::ZERO {
                let color = self.geom.border_pixel_color(
                    row,
                    col,
                    self.border_color,
                    self.inner_color,
                    self.outer_color,
                );
                return Some(embedded_graphics::Pixel(
                    Point::new(col as i32, row as i32),
                    color,
                ));
            }
        }
    }

    fn next_fill(&mut self) -> Option<embedded_graphics::Pixel<C>> {
        let fill_color = self.fill_color?;
        let g = self.geom;

        loop {
            match self.fill_phase {
                FillPhase::Border => {
                    self.fill_phase = FillPhase::CornerFill {
                        corner_idx: 0,
                        row: 0,
                        col: 0,
                    };
                }
                FillPhase::CornerFill {
                    ref mut corner_idx,
                    ref mut row,
                    ref mut col,
                } => {
                    if g.cr == 0 {
                        self.fill_phase = FillPhase::InteriorFill {
                            row: g.bw,
                            col: g.bw,
                        };
                        continue;
                    }
                    loop {
                        if *corner_idx >= 4 {
                            self.fill_phase = FillPhase::InteriorFill {
                                row: g.bw,
                                col: g.bw,
                            };
                            break;
                        }
                        let (rs, re, cs, ce) = g.corner_bounds(*corner_idx);
                        let abs_row = rs + *row;
                        let abs_col = cs + *col;

                        *col += 1;
                        if *col >= ce - cs {
                            *col = 0;
                            *row += 1;
                        }
                        if *row >= re - rs {
                            *row = 0;
                            *col = 0;
                            *corner_idx += 1;
                        }

                        let (outer_cov, border_cov) = g.coverages(abs_row, abs_col);
                        if outer_cov > Frac::ZERO && border_cov == Frac::ZERO {
                            let color =
                                g.fill_pixel_color(abs_row, abs_col, fill_color, self.outer_color);
                            return Some(embedded_graphics::Pixel(
                                Point::new(abs_col as i32, abs_row as i32),
                                color,
                            ));
                        }
                    }
                }
                FillPhase::InteriorFill {
                    ref mut row,
                    ref mut col,
                } => loop {
                    if *row >= g.h - g.bw {
                        self.fill_phase = FillPhase::Done;
                        return None;
                    }

                    let r = *row;
                    let c = *col;

                    *col += 1;
                    if *col >= g.w - g.bw {
                        *col = g.bw;
                        *row += 1;
                    }

                    if !g.in_corner(r, c) {
                        return Some(embedded_graphics::Pixel(
                            Point::new(c as i32, r as i32),
                            fill_color,
                        ));
                    }
                },
                FillPhase::Done => return None,
            }
        }
    }
}

impl<C: ColorInterpolate> Iterator for AARoundedRectIter<C> {
    type Item = embedded_graphics::Pixel<C>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_border().or_else(|| self.next_fill())
    }
}

impl<C: ColorInterpolate> DoubleEndedIterator for AARoundedRectIter<C> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            if self.front > self.back {
                return None;
            }

            let logical = self.back as u32;
            self.back -= 1;
            let global = self.logical_to_actual(logical);

            let (seg, local) = self.global_to_seg(global);
            let (row, col) = self.flat_to_rowcol(seg, local);
            let cov = self.geom.coverages(row, col).1;

            if cov > Frac::ZERO {
                let color = self.geom.border_pixel_color(
                    row,
                    col,
                    self.border_color,
                    self.inner_color,
                    self.outer_color,
                );
                return Some(embedded_graphics::Pixel(
                    Point::new(col as i32, row as i32),
                    color,
                ));
            }
        }
    }
}

impl<C: ColorInterpolate> Drawable for AARoundedRectangle<C> {
    type Color = C;
    type Output = ();

    fn draw<D: DrawTarget<Color = C>>(&self, target: &mut D) -> Result<(), D::Error> {
        target.draw_iter(self.pixels())
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
        let border_color = Rgb565::WHITE;
        let fill = Rgb565::new(0, 31, 0);
        let (w, h, cr, bw) = (128, 296, 42, 5);

        let iter =
            AARoundedRectIter::new(w, h, cr, bw, border_color, fill, bg).with_fill(Some(fill));
        let pixels: alloc::vec::Vec<_> = iter.collect();

        assert!(pixels.len() > 1000);

        let mut seen = alloc::collections::BTreeSet::new();
        let mut partial = 0;
        for &embedded_graphics::Pixel(point, color) in &pixels {
            assert!(point.x >= 0 && point.x < w as i32);
            assert!(point.y >= 0 && point.y < h as i32);
            assert!(seen.insert((point.x, point.y)), "duplicate at {:?}", point);
            if color != bg && color != border_color && color != fill {
                partial += 1;
            }
        }
        assert!(partial > 0, "expected AA partial-coverage pixels");

        let forward: alloc::vec::Vec<_> =
            AARoundedRectIter::new(w, h, cr, bw, border_color, fill, bg).collect();
        let mut reverse: alloc::vec::Vec<_> =
            AARoundedRectIter::new(w, h, cr, bw, border_color, fill, bg)
                .rev()
                .collect();
        reverse.reverse();
        assert_eq!(forward, reverse);
    }

    #[test]
    fn frac_range_endpoint_is_inclusive() {
        let (w, h, cr, bw) = (240, 280, 42, 5);
        let mk =
            || AARoundedRectIter::new(w, h, cr, bw, Rgb565::WHITE, Rgb565::BLACK, Rgb565::BLACK);

        let top = mk().top_center();
        let bottom = mk().bottom_center();

        let all: alloc::collections::BTreeSet<_> = mk().map(|Pixel(p, _)| (p.x, p.y)).collect();

        // 🪞 simulate hold_to_confirm: draw top→bottom, mirror horizontally
        let w_i32 = w as i32;
        let mirrored: alloc::collections::BTreeSet<_> = mk()
            .with_frac_range(top, bottom)
            .flat_map(|Pixel(p, _)| [(p.x, p.y), (w_i32 - 1 - p.x, p.y)])
            .collect();

        let missing: alloc::vec::Vec<_> = all.difference(&mirrored).collect();
        assert!(
            missing.is_empty(),
            "mirrored half missed {} pixels: {:?}",
            missing.len(),
            &missing[..missing.len().min(10)]
        );
    }

    #[test]
    fn range_split_concatenates() {
        let (w, h, cr, bw) = (128, 296, 42, 5);
        let mk =
            || AARoundedRectIter::new(w, h, cr, bw, Rgb565::WHITE, Rgb565::BLACK, Rgb565::BLACK);

        let all: alloc::vec::Vec<_> = mk().collect();
        let total = mk().total_raw_pixels();
        let half = total / 2;

        let first: alloc::vec::Vec<_> = mk().with_raw_range(0, half).collect();
        let second: alloc::vec::Vec<_> = mk().with_raw_range(half, total).collect();

        let mut combined = first;
        combined.extend(second);
        assert_eq!(combined, all);
    }
}
