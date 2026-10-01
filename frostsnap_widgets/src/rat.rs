use core::fmt;
use core::ops::{Add, Div, Mul, Sub};
use embedded_graphics::geometry::Point;
use fixed::types::{U16F16, U1F15};

/// A non-negative fixed-point number for pixel-scale arithmetic, in steps of 2^-16 up to 65,536.
/// Arithmetic saturates rather than wrapping, except `Rat * i32` and `u32 - Rat` (see each).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Rat(U16F16);

impl Rat {
    pub const ZERO: Self = Self(U16F16::ZERO);
    pub const MIN: Self = Self::ZERO;
    pub const ONE: Self = Self(U16F16::ONE);
    pub const MAX: Self = Self(U16F16::MAX);

    pub const fn from_int(int: u32) -> Self {
        if int > u16::MAX as u32 {
            return Self::MAX;
        }
        Self(U16F16::from_bits(int << 16))
    }

    /// `numerator / denominator` rounded to nearest. A zero denominator gives `MAX`.
    pub const fn from_ratio(numerator: u32, denominator: u32) -> Self {
        if denominator == 0 {
            return Self::MAX;
        }
        let bits = (((numerator as u64) << 16) + denominator as u64 / 2) / denominator as u64;
        if bits > u32::MAX as u64 {
            return Self::MAX;
        }
        Self(U16F16::from_bits(bits as u32))
    }

    /// Rounds half up.
    pub fn round(&self) -> u32 {
        let bits = self.0.to_bits();
        (bits >> 16) + ((bits >> 15) & 1)
    }

    pub fn floor(&self) -> u32 {
        self.0.to_bits() >> 16
    }

    pub fn ceil(&self) -> u32 {
        let bits = self.0.to_bits();
        (bits >> 16) + (bits & 0xffff != 0) as u32
    }
}

impl Mul<u32> for Rat {
    type Output = Rat;

    fn mul(self, rhs: u32) -> Self::Output {
        Rat(self.0.saturating_mul_int(rhs))
    }
}

impl Mul<Rat> for u32 {
    type Output = Rat;

    fn mul(self, rhs: Rat) -> Self::Output {
        rhs * self
    }
}

impl Mul<i32> for Rat {
    type Output = i32;

    /// Rounds toward zero. Unlike the other operators this does not saturate: a product beyond
    /// `i32` wraps, as `as` does, which no pixel-scale caller comes near.
    fn mul(self, rhs: i32) -> Self::Output {
        ((rhs as i64 * self.0.to_bits() as i64) / (1 << 16)) as i32
    }
}

impl Mul<Rat> for i32 {
    type Output = i32;

    fn mul(self, rhs: Rat) -> Self::Output {
        rhs * self
    }
}

impl Mul<Rat> for Rat {
    type Output = Rat;

    fn mul(self, rhs: Rat) -> Self::Output {
        Rat(self.0.saturating_mul(rhs.0))
    }
}

impl Div<u32> for Rat {
    type Output = Rat;

    fn div(self, rhs: u32) -> Self::Output {
        match self.0.checked_div_int(rhs) {
            Some(quotient) => Rat(quotient),
            None => Rat::MAX,
        }
    }
}

impl Div for Rat {
    type Output = Rat;

    /// Truncates. Dividing by zero gives `MAX`.
    fn div(self, rhs: Self) -> Self::Output {
        if rhs.0 == U16F16::ZERO {
            return Rat::MAX;
        }
        let bits = ((self.0.to_bits() as u64) << 16) / rhs.0.to_bits() as u64;
        Rat(U16F16::from_bits(bits.min(u32::MAX as u64) as u32))
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

    /// Clamps `self` to `Rat`'s range before subtracting, so an integer above 65,535 gives
    /// `MAX - rhs` rather than the true difference.
    fn sub(self, rhs: Rat) -> Self::Output {
        Rat::from_int(self) - rhs
    }
}

impl Mul<Point> for Rat {
    type Output = Point;

    fn mul(self, rhs: Point) -> Self::Output {
        Point::new(self * rhs.x, self * rhs.y)
    }
}

impl Mul<Rat> for Point {
    type Output = Point;

    fn mul(self, rhs: Rat) -> Self::Output {
        rhs * self
    }
}

impl fmt::Debug for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/65536", self.0.to_bits())
    }
}

impl fmt::Display for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A number in [0, 1].
///
/// `U1F15` rather than `U0F16` so that `ONE` is exact, which widgets compare against to detect
/// finished animations and fully opaque colours.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frac(U1F15);

impl Frac {
    pub const ZERO: Self = Self(U1F15::ZERO);
    pub const MIN: Self = Self::ZERO;
    pub const ONE: Self = Self(U1F15::ONE);
    pub const MAX: Self = Self::ONE;

    /// Clamps `rat` to 1.
    pub fn new(rat: Rat) -> Self {
        Self(U1F15::from_num(rat.min(Rat::ONE).0))
    }

    /// `numerator / denominator` rounded to nearest, clamped to 1. A zero denominator gives 1.
    pub const fn from_ratio(numerator: u32, denominator: u32) -> Self {
        if numerator >= denominator {
            return Self::ONE;
        }
        // The per-pixel callers all have small denominators, which keeps them in 32 bits: 64-bit
        // division is a libcall on riscv32.
        let bits = if denominator <= u16::MAX as u32 {
            ((numerator << 15) + denominator / 2) / denominator
        } else {
            ((((numerator as u64) << 15) + denominator as u64 / 2) / denominator as u64) as u32
        };
        Self(U1F15::from_bits(bits as u16))
    }

    pub const fn as_rat(&self) -> Rat {
        Rat(U16F16::from_bits((self.0.to_bits() as u32) << 1))
    }

    /// `self * n` rounded down, for `n` too large for `Rat`.
    pub fn mul_floor(self, n: u64) -> u64 {
        (n * self.0.to_bits() as u64) >> 15
    }
}

impl Mul<u32> for Frac {
    type Output = Rat;

    fn mul(self, rhs: u32) -> Self::Output {
        self.as_rat() * rhs
    }
}

impl Mul<Frac> for u32 {
    type Output = Rat;

    fn mul(self, rhs: Frac) -> Self::Output {
        rhs * self
    }
}

impl Mul<Frac> for Frac {
    type Output = Frac;

    fn mul(self, rhs: Frac) -> Self::Output {
        Frac(self.0 * rhs.0)
    }
}

impl Mul<Point> for Frac {
    type Output = Point;

    fn mul(self, rhs: Point) -> Self::Output {
        self.as_rat() * rhs
    }
}

impl Mul<Frac> for Point {
    type Output = Point;

    fn mul(self, rhs: Frac) -> Self::Output {
        rhs * self
    }
}

impl Add for Frac {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0).min(U1F15::ONE))
    }
}

impl Sub for Frac {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Div for Frac {
    type Output = Rat;

    /// Truncates. Dividing by zero gives `Rat::MAX`.
    fn div(self, rhs: Self) -> Self::Output {
        // The ratio of the bit patterns is the ratio of the values, and dividing by an integer
        // stays in 32 bits where dividing by a fixed-point value would widen to 64.
        match U16F16::from_num(self.0.to_bits()).checked_div_int(rhs.0.to_bits().into()) {
            Some(quotient) => Rat(quotient),
            None => Rat::MAX,
        }
    }
}

impl fmt::Debug for Frac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frac({:?})", self.as_rat())
    }
}

impl fmt::Display for Frac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.as_rat(), f)
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
        FatRat(self.0.checked_div(rhs).unwrap_or(u64::MAX))
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
