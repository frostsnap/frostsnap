use core::ops::{Add, Mul, Div, Sub};
use core::fmt;

/// The base denominator for rational number representation
const DENOMINATOR: u32 = 10_000;

/// A rational number represented as (numerator * DENOMINATOR) / denominator
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rat(pub(crate) u32);

impl Rat {
    /// Create from a numerator and denominator
    pub fn from_ratio(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            // everything over 0 should be large!
            return Self(u32::MAX);
        }
        let value = ((numerator as u64 * DENOMINATOR as u64) / denominator as u64) as u32;
        Self(value)
    }
    
    /// Returns 1.0 - self (only valid if self <= 1.0)
    pub const fn one_minus(&self) -> Self {
        Self(DENOMINATOR.saturating_sub(self.0))
    }
    
    /// Clamps the value to a maximum of 1.0
    pub fn clamp_to_one(&mut self) {
        if self.0 > DENOMINATOR {
            self.0 = DENOMINATOR;
        }
    }
    
    /// Minimum value (0)
    pub const ZERO: Self = Self(0);
    pub const MIN: Self = Self::ZERO;
    
    /// Value representing 1.0
    pub const ONE: Self = Self(DENOMINATOR);
    
    /// Maximum value
    pub const MAX: Self = Self(u32::MAX);
}

impl Mul<u32> for Rat {
    type Output = u32;
    
    fn mul(self, rhs: u32) -> Self::Output {
        ((rhs as u64 * self.0 as u64) / DENOMINATOR as u64) as u32
    }
}

impl Mul<Rat> for u32 {
    type Output = u32;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        ((self as u64 * rhs.0 as u64) / DENOMINATOR as u64) as u32
    }
}

impl Mul<i32> for Rat {
    type Output = i32;
    
    fn mul(self, rhs: i32) -> Self::Output {
        ((rhs as i64 * self.0 as i64) / DENOMINATOR as i64) as i32
    }
}

impl Mul<Rat> for i32 {
    type Output = i32;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        ((self as i64 * rhs.0 as i64) / DENOMINATOR as i64) as i32
    }
}

impl Mul<Rat> for Rat {
    type Output = Rat;
    
    fn mul(self, rhs: Rat) -> Self::Output {
        let value = ((self.0 as u64 * rhs.0 as u64) / DENOMINATOR as u64) as u32;
        Rat(value)
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

impl Add for Rat {
    type Output = Self;
    
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
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

impl fmt::Debug for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.0, DENOMINATOR)
    }
}

impl fmt::Display for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / DENOMINATOR;
        let frac = self.0 % DENOMINATOR;
        
        if frac == 0 {
            write!(f, "{}", whole)
        } else {
            // Format with up to 4 decimal places, trimming trailing zeros
            let frac_str = format!("{:04}", frac);
            let trimmed = frac_str.trim_end_matches('0');
            if trimmed.is_empty() {
                write!(f, "{}", whole)
            } else {
                write!(f, "{}.{}", whole, trimmed)
            }
        }
    }
}

/// A fraction between 0 and 1, automatically clamped
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frac(Rat);

impl Frac {
    /// Create a new Frac from a Rat, clamping to [0, 1]
    pub fn new(mut rat: Rat) -> Self {
        rat.clamp_to_one();
        Self(rat)
    }
    
    /// Create from a numerator and denominator, clamping to [0, 1]
    pub fn from_ratio(numerator: u32, denominator: u32) -> Self {
        let rat = Rat::from_ratio(numerator, denominator);
        Self::new(rat)
    }
    
    /// Get the inner Rat value
    pub fn as_rat(&self) -> Rat {
        self.0
    }
    
    /// Returns 1.0 - self
    pub fn one_minus(&self) -> Self {
        Self(self.0.one_minus())
    }

    /// Zero fraction
    pub const ZERO: Self = Self(Rat::ZERO);
    pub const MIN: Self = Self::ZERO;
    
    /// One fraction
    pub const ONE: Self = Self(Rat::ONE);
    pub const MAX: Self = Self::ONE;
}

impl Mul<u32> for Frac {
    type Output = u32;
    
    fn mul(self, rhs: u32) -> Self::Output {
        self.0 * rhs
    }
}

impl Mul<Frac> for u32 {
    type Output = u32;
    
    fn mul(self, rhs: Frac) -> Self::Output {
        self * rhs.0
    }
}

impl Mul<embedded_graphics::geometry::Point> for Frac {
    type Output = embedded_graphics::geometry::Point;
    
    fn mul(self, rhs: embedded_graphics::geometry::Point) -> Self::Output {
        self.0 * rhs
    }
}

impl Mul<Frac> for embedded_graphics::geometry::Point {
    type Output = embedded_graphics::geometry::Point;
    
    fn mul(self, rhs: Frac) -> Self::Output {
        self * rhs.0
    }
}

impl Add for Frac {
    type Output = Self;
    
    fn add(self, rhs: Self) -> Self::Output {
        // Add the underlying Rat values and clamp to 1
        Self::new(self.0 + rhs.0)
    }
}

impl Sub for Frac {
    type Output = Self;
    
    fn sub(self, rhs: Self) -> Self::Output {
        // Subtract and clamp at 0 (since Rat uses saturating_sub)
        Self(self.0 - rhs.0)
    }
}

impl fmt::Debug for Frac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frac({:?})", self.0)
    }
}

impl fmt::Display for Frac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Delegate to Rat's Display implementation
        fmt::Display::fmt(&self.0, f)
    }
}

