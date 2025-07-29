use core::ops::{Mul, Div, Sub};

/// A rational number represented as (numerator * 1000) / denominator
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rat(u32);

impl Rat {
    /// Create from a numerator and denominator
    pub fn from_ratio(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            return Self(0);
        }
        let value = ((numerator as u64 * 1000) / denominator as u64) as u32;
        Self(value)
    }
    
    /// Returns 1.0 - self (only valid if self <= 1.0)
    pub const fn one_minus(&self) -> Self {
        Self(1000u32.saturating_sub(self.0))
    }
    
    /// Minimum value (0)
    pub const ZERO: Self = Self(0);
    
    /// Value representing 1.0
    pub const ONE: Self = Self(1000);
}

impl Mul<u32> for Rat {
    type Output = u32;
    
    fn mul(self, rhs: u32) -> Self::Output {
        ((rhs as u64 * self.0 as u64) / 1000) as u32
    }
}

impl Mul<Rat> for u32 {
    type Output = u32;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        ((self as u64 * rhs.0 as u64) / 1000) as u32
    }
}

impl Mul<i32> for Rat {
    type Output = i32;
    
    fn mul(self, rhs: i32) -> Self::Output {
        ((rhs as i64 * self.0 as i64) / 1000) as i32
    }
}

impl Mul<Rat> for i32 {
    type Output = i32;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        ((self as i64 * rhs.0 as i64) / 1000) as i32
    }
}

impl Mul<Rat> for Rat {
    type Output = u32;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        ((self.0 as u64 * rhs.0 as u64) / 1000 / 1000) as u32
    }
}

impl Div<u32> for Rat {
    type Output = u32;
    
    fn div(self, rhs: u32) -> Self::Output {
        self.0 / rhs
    }
}

impl Default for Rat {
    fn default() -> Self {
        Self::ZERO
    }
}

impl Sub for Rat {
    type Output = Self;
    
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Mul<embedded_graphics::geometry::Point> for Rat {
    type Output = embedded_graphics::geometry::Point;
    
    fn mul(self, rhs: embedded_graphics::geometry::Point) -> Self::Output {
        embedded_graphics::geometry::Point::new(
            self * rhs.x,
            self * rhs.y,
        )
    }
}

impl Mul<Rat> for embedded_graphics::geometry::Point {
    type Output = embedded_graphics::geometry::Point;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        rhs * self
    }
}