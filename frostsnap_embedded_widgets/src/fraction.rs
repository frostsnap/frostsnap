use core::ops::Mul;

/// A fraction represented as a value from 0 to 1000 (representing 0.0 to 1.0)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fraction(u16);

impl Fraction {
    /// Create from a numerator and denominator
    pub fn from_ratio(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            return Self(0);
        }
        let value = ((numerator as u64 * 1000) / denominator as u64).min(1000) as u16;
        Self(value)
    }
    
    /// Returns 1.0 - self
    pub const fn one_minus(&self) -> Self {
        Self(1000 - self.0)
    }
    
    /// Minimum value (0)
    pub const ZERO: Self = Self(0);
    
    /// Maximum value (1000)
    pub const ONE: Self = Self(1000);
}

impl Mul<u32> for Fraction {
    type Output = u32;
    
    fn mul(self, rhs: u32) -> Self::Output {
        ((rhs as u64 * self.0 as u64) / 1000) as u32
    }
}

impl Mul<Fraction> for u32 {
    type Output = u32;
    
    fn mul(self, rhs: Fraction) -> Self::Output {
        ((self as u64 * rhs.0 as u64) / 1000) as u32
    }
}

impl Mul<i32> for Fraction {
    type Output = i32;
    
    fn mul(self, rhs: i32) -> Self::Output {
        ((rhs as i64 * self.0 as i64) / 1000) as i32
    }
}

impl Mul<Fraction> for i32 {
    type Output = i32;
    
    fn mul(self, rhs: Fraction) -> Self::Output {
        ((self as i64 * rhs.0 as i64) / 1000) as i32
    }
}

impl Default for Fraction {
    fn default() -> Self {
        Self::ZERO
    }
}