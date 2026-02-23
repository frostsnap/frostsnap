use embedded_graphics::prelude::*;

/// Compressed point representation using 3 bytes instead of 8
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressedPoint {
    pub x: u8,
    pub y: u16,
}

impl CompressedPoint {
    pub fn new(point: Point) -> Self {
        Self {
            x: point.x as u8,
            y: point.y as u16,
        }
    }

    pub fn to_point(self) -> Point {
        Point::new(self.x as i32, self.y as i32)
    }
}

impl From<CompressedPoint> for Point {
    fn from(cp: CompressedPoint) -> Self {
        cp.to_point()
    }
}

/// Compressed point with anti-aliasing coverage (Gray4 level 0–15).
/// 4 bytes per point vs 3 for CompressedPoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressedPointWithCoverage {
    pub x: u8,
    pub y: u16,
    pub coverage: u8, // 0–15 (Gray4 levels)
}

impl CompressedPointWithCoverage {
    pub fn new(point: Point, coverage: u8) -> Self {
        Self {
            x: point.x as u8,
            y: point.y as u16,
            coverage,
        }
    }

    pub fn to_point(self) -> Point {
        Point::new(self.x as i32, self.y as i32)
    }
}
