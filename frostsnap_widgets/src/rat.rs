use core::fmt;
use core::ops::{Add, Div, Mul, Sub};

/// The base denominator for rational number representation
const DENOMINATOR: u32 = 10_000;

/// A rational number represented as (numerator * DENOMINATOR) / denominator
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rat(pub(crate) u32);

impl Rat {
    pub const fn from_int(int: u32) -> Self {
        Self(int * DENOMINATOR)
    }

    /// Create from a numerator and denominator
    pub const fn from_ratio(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            // everything over 0 should be large!
            return Self(u32::MAX);
        }
        let value = ((numerator as u64 * DENOMINATOR as u64) / denominator as u64) as u32;
        Self(value)
    }

    /// Minimum value (0)
    pub const ZERO: Self = Self(0);
    pub const MIN: Self = Self::ZERO;

    /// Value representing 1.0
    pub const ONE: Self = Self(DENOMINATOR);

    /// Maximum value
    pub const MAX: Self = Self(u32::MAX);

    /// Round to the nearest whole number
    pub fn round(&self) -> u32 {
        let whole = self.0 / DENOMINATOR;
        let frac = self.0 % DENOMINATOR;
        if frac >= DENOMINATOR / 2 {
            whole + 1
        } else {
            whole
        }
    }

    /// Round down to the nearest whole number (floor)
    pub fn floor(&self) -> u32 {
        self.0 / DENOMINATOR
    }

    /// Round up to the nearest whole number (ceil)
    pub fn ceil(&self) -> u32 {
        let whole = self.0 / DENOMINATOR;
        let frac = self.0 % DENOMINATOR;
        if frac > 0 {
            whole + 1
        } else {
            whole
        }
    }
}

impl Mul<u32> for Rat {
    type Output = Rat;

    fn mul(self, rhs: u32) -> Self::Output {
        Rat(self.0 * rhs)
    }
}

impl Mul<Rat> for u32 {
    type Output = Rat;

    fn mul(self, rhs: Rat) -> Self::Output {
        Rat(self * rhs.0)
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

impl Sub<u32> for Rat {
    type Output = Rat;

    fn sub(self, rhs: u32) -> Self::Output {
        self - Rat::from_int(rhs)
    }
}

impl Sub<Rat> for u32 {
    type Output = Rat;

    fn sub(self, rhs: Rat) -> Self::Output {
        Rat::from_int(self) - rhs
    }
}

impl Mul<embedded_graphics::geometry::Point> for Rat {
    type Output = embedded_graphics::geometry::Point;

    fn mul(self, rhs: embedded_graphics::geometry::Point) -> Self::Output {
        embedded_graphics::geometry::Point::new(self * rhs.x, self * rhs.y)
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
    pub fn new(rat: Rat) -> Self {
        Self(rat.min(Rat::ONE))
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

    /// Zero fraction
    pub const ZERO: Self = Self(Rat::ZERO);
    pub const MIN: Self = Self::ZERO;

    /// One fraction
    pub const ONE: Self = Self(Rat::ONE);
    pub const MAX: Self = Self::ONE;
}

impl Mul<u32> for Frac {
    type Output = Rat;

    fn mul(self, rhs: u32) -> Self::Output {
        Rat(self.0 .0 * rhs)
    }
}

impl Mul<Frac> for u32 {
    type Output = Rat;

    fn mul(self, rhs: Frac) -> Self::Output {
        Rat(self * rhs.0 .0)
    }
}

impl Mul<Frac> for Frac {
    type Output = Frac;

    fn mul(self, rhs: Frac) -> Self::Output {
        // When multiplying two Fracs (both ≤ 1), result is guaranteed to be in [0,1]
        // We can directly use Rat * Rat which handles the fixed-point arithmetic
        Frac(self.0 * rhs.0)
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

/// The base denominator for FatRat rational number representation (1 trillion)
const FAT_DENOMINATOR: u64 = 1_000_000_000_000;

/// A rational number with higher precision, represented as (numerator * FAT_DENOMINATOR) / denominator
/// Uses u64 for larger range than Rat
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FatRat(pub(crate) u64);

impl FatRat {
    pub const fn from_int(int: u64) -> Self {
        Self(int * FAT_DENOMINATOR)
    }

    /// Create from a numerator and denominator
    pub const fn from_ratio(numerator: u64, denominator: u64) -> Self {
        if denominator == 0 {
            // everything over 0 should be large!
            return Self(u64::MAX);
        }
        // Use u128 to avoid overflow
        let value = ((numerator as u128 * FAT_DENOMINATOR as u128) / denominator as u128) as u64;
        Self(value)
    }

    /// Minimum value (0)
    pub const ZERO: Self = Self(0);
    pub const MIN: Self = Self::ZERO;

    /// Value representing 1.0
    pub const ONE: Self = Self(FAT_DENOMINATOR);

    /// Maximum value
    pub const MAX: Self = Self(u64::MAX);

    /// Round to the nearest whole number
    pub fn round(&self) -> u64 {
        let whole = self.0 / FAT_DENOMINATOR;
        let frac = self.0 % FAT_DENOMINATOR;
        if frac >= FAT_DENOMINATOR / 2 {
            whole + 1
        } else {
            whole
        }
    }

    /// Round down to the nearest whole number (floor)
    pub fn floor(&self) -> u64 {
        self.0 / FAT_DENOMINATOR
    }

    /// Round up to the nearest whole number (ceil)
    pub fn ceil(&self) -> u64 {
        let whole = self.0 / FAT_DENOMINATOR;
        let frac = self.0 % FAT_DENOMINATOR;
        if frac > 0 {
            whole + 1
        } else {
            whole
        }
    }

    /// Get the whole part (same as floor)
    pub const fn whole_part(&self) -> u64 {
        self.0 / FAT_DENOMINATOR
    }

    /// Get the fractional part (internal use)
    const fn fractional_part(&self) -> u64 {
        self.0 % FAT_DENOMINATOR
    }

    /// Returns an iterator over all decimal digits after the decimal point (up to 12 digits)
    pub fn decimal_digits(self) -> impl Iterator<Item = u8> {
        let mut remaining = self.fractional_part();
        // Start with 10^11 to get first decimal digit
        let mut window = 10_u64.pow(11);

        core::iter::from_fn(move || {
            if window == 0 {
                return None;
            }

            let digit = (remaining / window) as u8;
            remaining -= digit as u64 * window;
            window /= 10;

            Some(digit)
        })
    }

    /// Format as a decimal string with up to 12 decimal places
    /// Returns a tuple of (whole_part, decimal_part) as strings
    pub fn format_parts(
        &self,
        decimal_places: usize,
    ) -> (alloc::string::String, alloc::string::String) {
        assert!(
            decimal_places <= 12,
            "Cannot have more than 12 decimal places"
        );

        let whole = self.whole_part();
        let frac = self.fractional_part();

        // Scale down the fractional part if we want fewer decimal places
        let divisor = 10_u64.pow((12 - decimal_places) as u32);
        let scaled_frac = frac / divisor;

        // Format fractional part with leading zeros
        let decimal = alloc::format!("{:0width$}", scaled_frac, width = decimal_places);

        (alloc::format!("{}", whole), decimal)
    }

    /// Format as "X.YYYYYY" string with specified decimal places
    pub fn format_decimal(&self, decimal_places: usize) -> alloc::string::String {
        let (whole, decimal) = self.format_parts(decimal_places);
        if decimal.is_empty() {
            whole
        } else {
            alloc::format!("{}.{}", whole, decimal)
        }
    }
}

impl Mul<u64> for FatRat {
    type Output = FatRat;

    fn mul(self, rhs: u64) -> Self::Output {
        FatRat(self.0.saturating_mul(rhs))
    }
}

impl Mul<FatRat> for u64 {
    type Output = FatRat;

    fn mul(self, rhs: FatRat) -> Self::Output {
        FatRat(self.saturating_mul(rhs.0))
    }
}

impl Mul<i64> for FatRat {
    type Output = i64;

    fn mul(self, rhs: i64) -> Self::Output {
        ((rhs as i128 * self.0 as i128) / FAT_DENOMINATOR as i128) as i64
    }
}

impl Mul<FatRat> for i64 {
    type Output = i64;

    fn mul(self, rhs: FatRat) -> Self::Output {
        ((self as i128 * rhs.0 as i128) / FAT_DENOMINATOR as i128) as i64
    }
}

impl Mul<FatRat> for FatRat {
    type Output = FatRat;

    fn mul(self, rhs: FatRat) -> Self::Output {
        let value = ((self.0 as u128 * rhs.0 as u128) / FAT_DENOMINATOR as u128) as u64;
        FatRat(value)
    }
}

impl Div<u64> for FatRat {
    type Output = FatRat;

    fn div(self, rhs: u64) -> Self::Output {
        if rhs == 0 {
            FatRat(u64::MAX)
        } else {
            FatRat(self.0 / rhs)
        }
    }
}

impl Div<FatRat> for FatRat {
    type Output = FatRat;

    fn div(self, rhs: FatRat) -> Self::Output {
        if rhs.0 == 0 {
            FatRat(u64::MAX)
        } else {
            let value = ((self.0 as u128 * FAT_DENOMINATOR as u128) / rhs.0 as u128) as u64;
            FatRat(value)
        }
    }
}

impl Add<FatRat> for FatRat {
    type Output = FatRat;

    fn add(self, rhs: FatRat) -> Self::Output {
        FatRat(self.0.saturating_add(rhs.0))
    }
}

impl Sub<FatRat> for FatRat {
    type Output = FatRat;

    fn sub(self, rhs: FatRat) -> Self::Output {
        FatRat(self.0.saturating_sub(rhs.0))
    }
}

impl fmt::Debug for FatRat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "FatRat({}/{})", self.0, FAT_DENOMINATOR)
    }
}

impl fmt::Display for FatRat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let whole = self.0 / FAT_DENOMINATOR;
        let frac = self.0 % FAT_DENOMINATOR;
        if frac == 0 {
            write!(f, "{}", whole)
        } else {
            // Show up to 6 decimal places by default
            let divisor = 10_u64.pow(6);
            let scaled_frac = frac / divisor;
            write!(f, "{}.{:06}", whole, scaled_frac)
        }
    }
}
