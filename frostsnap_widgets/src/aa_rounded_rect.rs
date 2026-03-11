use crate::widget_color::ColorInterpolate;
use crate::Frac;
use embedded_graphics::{draw_target::DrawTarget, prelude::*, primitives::Rectangle};

pub struct AARoundedRectangle<C: ColorInterpolate> {
    rect: Rectangle,
    corner_radius: u32,
    fill_color: Option<C>,
    border_color: Option<C>,
    border_width: u32,
    background_color: C,
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

    pub fn pixels(&self) -> impl Iterator<Item = embedded_graphics::Pixel<C>> + '_ {
        let bw = if self.border_color.is_some() {
            self.border_width
        } else {
            0
        };
        let bg = self.background_color;
        let border_color = self.border_color.unwrap_or(bg);
        let inner = self.fill_color.unwrap_or(bg);
        let offset = self.rect.top_left;

        AARoundedRectIter::new(
            self.rect.size.width,
            self.rect.size.height,
            self.corner_radius,
            bw,
            border_color,
            inner,
            bg,
        )
        .with_fill(self.fill_color)
        .map(move |embedded_graphics::Pixel(p, c)| embedded_graphics::Pixel(p + offset, c))
    }
}

/// Scale factor for fixed-point SDF calculations
const SCALE: i64 = 256;

#[inline]
fn coverage_from_distance(distance_scaled: i64) -> Frac {
    let half = SCALE / 2;
    let clamped = (half - distance_scaled).clamp(0, SCALE);
    Frac::from_ratio(clamped as u32, SCALE as u32)
}

#[inline]
fn isqrt_distance(dx: i64, dy: i64, radius_scaled: i64) -> i64 {
    (dx * dx + dy * dy).unsigned_abs().isqrt() as i64 - radius_scaled
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
        let (outer_cov, border_cov) = self.coverages(row, col);
        let fill_cov = Frac::new((outer_cov.as_rat() - border_cov.as_rat()).max(crate::Rat::ZERO));
        let mut color = outer_color;
        color = color.interpolate(inner_color, fill_cov);
        if border_cov > Frac::ZERO {
            color = color.interpolate(border_color, border_cov);
        }
        color
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

    /// Restrict iteration to a perimeter fraction range `[start, end)`.
    /// If `start > end` in logical space, the range wraps around the perimeter.
    pub fn with_frac_range(self, start: Frac, end: Frac) -> Self {
        let raw_start = self.actual_to_logical(self.frac_to_raw(start));
        let mut raw_end = self.actual_to_logical(self.frac_to_raw(end));
        if raw_end <= raw_start && start != end {
            // ↻ wrapping range
            raw_end += self.total_raw_pixels();
        }
        self.with_raw_range(raw_start, raw_end)
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

    fn flat_to_rowcol(&self, seg: u8, flat: u32) -> (u32, u32) {
        let (rs, re, cs, ce) = self.segment_bounds(seg);
        let rows = re - rs;
        let cols = ce - cs;
        if rows == 0 || cols == 0 {
            return (rs, cs);
        }

        match seg {
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
            _ => {
                let row_idx = flat / cols;
                let col_idx = flat % cols;
                (rs + row_idx, cs + col_idx)
            }
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
    fn corner_pixels_have_partial_coverage() {
        let bg = Rgb565::BLACK;
        let fill = Rgb565::WHITE;
        let rect = Rectangle::new(Point::zero(), Size::new(100, 100));
        let shape = AARoundedRectangle::new(rect, bg)
            .with_corner_radius(40)
            .with_fill(fill);

        let pixels: alloc::vec::Vec<_> = shape.pixels().collect();

        let mut partial_count = 0;
        for pixel in &pixels {
            let c = pixel.1;
            if c != bg && c != fill {
                partial_count += 1;
            }
        }

        assert!(
            partial_count > 0,
            "Expected some partially-covered pixels for AA, got none"
        );
    }

    #[test]
    fn coverage_from_distance_values() {
        let cov = coverage_from_distance(0);
        assert!(
            cov > Frac::ZERO && cov < Frac::ONE,
            "boundary coverage should be partial: {:?}",
            cov
        );

        let cov = coverage_from_distance(-SCALE);
        assert_eq!(cov, Frac::ONE);

        let cov = coverage_from_distance(SCALE);
        assert_eq!(cov, Frac::ZERO);
    }

    fn test_iter(w: u32, h: u32, cr: u32, bw: u32) -> AARoundedRectIter<Rgb565> {
        AARoundedRectIter::new(w, h, cr, bw, Rgb565::WHITE, Rgb565::BLACK, Rgb565::BLACK)
    }

    #[test]
    fn border_iter_yields_pixels() {
        let iter = test_iter(128, 296, 42, 5);
        let pixels: alloc::vec::Vec<_> = iter.collect();
        assert!(
            pixels.len() > 1000,
            "expected many border pixels, got {}",
            pixels.len()
        );

        for &embedded_graphics::Pixel(point, _) in &pixels {
            assert!(point.x >= 0 && point.x < 128);
            assert!(point.y >= 0 && point.y < 296);
        }
    }

    #[test]
    fn border_iter_no_duplicates() {
        let iter = test_iter(128, 296, 42, 5);
        let pixels: alloc::vec::Vec<_> = iter.collect();

        let mut seen = alloc::collections::BTreeSet::new();
        for &embedded_graphics::Pixel(point, _) in &pixels {
            assert!(
                seen.insert((point.x, point.y)),
                "duplicate pixel at {:?}",
                point
            );
        }
    }

    #[test]
    fn border_iter_corners_have_partial_coverage() {
        let bg = Rgb565::BLACK;
        let border_color = Rgb565::WHITE;
        let iter = AARoundedRectIter::new(128, 296, 42, 5, border_color, bg, bg);
        let mut partial = 0;
        for embedded_graphics::Pixel(_, color) in iter {
            if color != bg && color != border_color {
                partial += 1;
            }
        }
        assert!(
            partial > 0,
            "expected some partial-coverage pixels in corners"
        );
    }

    #[test]
    fn border_iter_reverse_same_pixels() {
        let iter = test_iter(128, 296, 42, 5);
        let forward: alloc::vec::Vec<_> = iter.collect();

        let iter = test_iter(128, 296, 42, 5);
        let mut reverse: alloc::vec::Vec<_> = iter.rev().collect();
        reverse.reverse();

        assert_eq!(
            forward.len(),
            reverse.len(),
            "forward and reverse should yield same count"
        );
        for (f, r) in forward.iter().zip(reverse.iter()) {
            assert_eq!(f, r);
        }
    }

    #[test]
    fn border_iter_double_ended_meets_in_middle() {
        let mut iter = test_iter(128, 296, 42, 5);
        let total = iter.clone().count();

        let mut from_front = alloc::vec::Vec::new();
        let mut from_back = alloc::vec::Vec::new();

        for _ in 0..total / 2 {
            from_front.push(iter.next().unwrap());
        }
        while let Some(p) = iter.next_back() {
            from_back.push(p);
        }
        let remaining: alloc::vec::Vec<_> = iter.collect();
        assert!(remaining.is_empty());

        assert_eq!(
            from_front.len() + from_back.len(),
            total,
            "front + back should equal total"
        );
    }

    #[test]
    fn iter_with_fill_matches_rasterizer() {
        let w = 100u32;
        let h = 100;
        let cr = 30u32;
        let bw = 4u32;

        let bg = Rgb565::BLACK;
        let border = Rgb565::WHITE;
        let fill = Rgb565::new(0, 31, 0);
        let rect = Rectangle::new(Point::zero(), Size::new(w, h));
        let shape = AARoundedRectangle::new(rect, bg)
            .with_corner_radius(cr)
            .with_border(border, bw)
            .with_fill(fill);

        let raster_pixels: alloc::collections::BTreeMap<(i32, i32), Rgb565> = shape
            .pixels()
            .map(|embedded_graphics::Pixel(p, c)| ((p.x, p.y), c))
            .collect();

        let iter_pixels: alloc::collections::BTreeMap<(i32, i32), Rgb565> =
            AARoundedRectIter::new(w, h, cr, bw, border, fill, bg)
                .with_fill(Some(fill))
                .map(|embedded_graphics::Pixel(p, c)| ((p.x, p.y), c))
                .collect();

        assert_eq!(
            raster_pixels.len(),
            iter_pixels.len(),
            "pixel count mismatch: rasterizer={} iter={}",
            raster_pixels.len(),
            iter_pixels.len(),
        );

        for (&pos, &raster_color) in &raster_pixels {
            let iter_color = iter_pixels
                .get(&pos)
                .unwrap_or_else(|| panic!("rasterizer pixel {:?} missing from iter output", pos));
            assert_eq!(
                raster_color, *iter_color,
                "color mismatch at {:?}: rasterizer={:?} iter={:?}",
                pos, raster_color, iter_color
            );
        }
    }

    #[test]
    fn iter_with_fill_no_duplicates() {
        let w = 100u32;
        let h = 100;
        let cr = 30u32;
        let bw = 4u32;

        let bg = Rgb565::BLACK;
        let border = Rgb565::WHITE;
        let fill = Rgb565::new(0, 31, 0);

        let iter = AARoundedRectIter::new(w, h, cr, bw, border, fill, bg).with_fill(Some(fill));
        let pixels: alloc::vec::Vec<_> = iter.collect();

        let mut seen = alloc::collections::BTreeSet::new();
        for &embedded_graphics::Pixel(point, _) in &pixels {
            assert!(
                seen.insert((point.x, point.y)),
                "duplicate pixel at {:?}",
                point
            );
        }
    }

    #[test]
    fn iter_fill_only_no_border() {
        let w = 80u32;
        let h = 80;
        let cr = 20u32;

        let bg = Rgb565::BLACK;
        let fill = Rgb565::WHITE;
        let rect = Rectangle::new(Point::zero(), Size::new(w, h));
        let shape = AARoundedRectangle::new(rect, bg)
            .with_corner_radius(cr)
            .with_fill(fill);

        let raster_pixels: alloc::collections::BTreeMap<(i32, i32), Rgb565> = shape
            .pixels()
            .map(|embedded_graphics::Pixel(p, c)| ((p.x, p.y), c))
            .collect();

        let iter_pixels: alloc::collections::BTreeMap<(i32, i32), Rgb565> =
            AARoundedRectIter::new(w, h, cr, 0, bg, fill, bg)
                .with_fill(Some(fill))
                .map(|embedded_graphics::Pixel(p, c)| ((p.x, p.y), c))
                .collect();

        assert_eq!(
            raster_pixels.len(),
            iter_pixels.len(),
            "fill-only pixel count mismatch: rasterizer={} iter={}",
            raster_pixels.len(),
            iter_pixels.len()
        );

        for (&pos, &raster_color) in &raster_pixels {
            let iter_color = iter_pixels.get(&pos).unwrap_or_else(|| {
                panic!(
                    "rasterizer pixel {:?} missing from fill-only iter output",
                    pos
                )
            });
            assert_eq!(
                raster_color, *iter_color,
                "fill-only color mismatch at {:?}",
                pos
            );
        }
    }

    #[test]
    fn frac_to_raw_matches_raw_helpers() {
        let iter = test_iter(100, 200, 20, 5);
        let top_raw = iter.top_center_raw();
        let bottom_raw = iter.bottom_center_raw();
        let top_frac = iter.top_center();
        let bottom_frac = iter.bottom_center();
        let top_via_frac = iter.frac_to_raw(top_frac);
        let bottom_via_frac = iter.frac_to_raw(bottom_frac);

        assert_eq!(
            top_raw, top_via_frac,
            "top_center_raw={} but frac_to_raw(top_center())={} (frac={:?})",
            top_raw, top_via_frac, top_frac
        );
        assert_eq!(
            bottom_raw, bottom_via_frac,
            "bottom_center_raw={} but frac_to_raw(bottom_center())={} (frac={:?})",
            bottom_raw, bottom_via_frac, bottom_frac
        );
    }

    #[test]
    fn border_iter_with_raw_range() {
        let iter = test_iter(128, 296, 42, 5);
        let all: alloc::vec::Vec<_> = iter.collect();

        let total_raw = test_iter(128, 296, 42, 5).total_raw_pixels();
        let half = total_raw / 2;
        let first_half: alloc::vec::Vec<_> =
            test_iter(128, 296, 42, 5).with_raw_range(0, half).collect();
        let second_half: alloc::vec::Vec<_> = test_iter(128, 296, 42, 5)
            .with_raw_range(half, total_raw)
            .collect();

        let mut combined = first_half;
        combined.extend(second_half);
        assert_eq!(combined.len(), all.len());
        assert_eq!(combined, all);
    }
}
