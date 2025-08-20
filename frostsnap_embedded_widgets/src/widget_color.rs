use crate::Frac;
use embedded_graphics::pixelcolor::{
    BinaryColor, Gray2, Gray4, GrayColor, PixelColor, Rgb565, RgbColor,
};

/// Trait for colors that can be interpolated
pub trait ColorInterpolate: PixelColor {
    /// Interpolate between two colors. Returns a color that is `frac` of the way from `self` to `other`.
    /// When frac is 0, returns self. When frac is 1, returns other.
    fn interpolate(&self, other: Self, frac: Frac) -> Self;
}

impl ColorInterpolate for Rgb565 {
    fn interpolate(&self, other: Self, frac: Frac) -> Self {
        if frac == Frac::ONE {
            return other;
        }
        if frac == Frac::ZERO {
            return *self;
        }

        // frac represents progress from self to other
        let frac_inv = Frac::ONE - frac;

        // For each color component, calculate: self * (1-frac) + other * frac
        let from_r = (frac_inv * self.r() as u32).round();
        let from_g = (frac_inv * self.g() as u32).round();
        let from_b = (frac_inv * self.b() as u32).round();

        let to_r = (frac * other.r() as u32).round();
        let to_g = (frac * other.g() as u32).round();
        let to_b = (frac * other.b() as u32).round();

        Rgb565::new(
            (from_r + to_r) as u8,
            (from_g + to_g) as u8,
            (from_b + to_b) as u8,
        )
    }
}

impl ColorInterpolate for BinaryColor {
    fn interpolate(&self, other: Self, frac: Frac) -> Self {
        // For binary colors, use a threshold approach at 50%
        if frac > Frac::from_ratio(1, 2) {
            other
        } else {
            *self
        }
    }
}

impl ColorInterpolate for Gray2 {
    fn interpolate(&self, other: Self, frac: Frac) -> Self {
        if frac == Frac::ONE {
            return other;
        }
        if frac == Frac::ZERO {
            return *self;
        }

        let frac_inv = Frac::ONE - frac;

        let from_v = (frac_inv * self.luma() as u32).round();
        let to_v = (frac * other.luma() as u32).round();

        Gray2::new((from_v + to_v) as u8)
    }
}

impl ColorInterpolate for Gray4 {
    fn interpolate(&self, other: Self, frac: Frac) -> Self {
        if frac == Frac::ONE {
            return other;
        }
        if frac == Frac::ZERO {
            return *self;
        }

        let frac_inv = Frac::ONE - frac;

        let from_v = (frac_inv * self.luma() as u32).round();
        let to_v = (frac * other.luma() as u32).round();

        Gray4::new((from_v + to_v) as u8)
    }
}

/// Trait alias for colors that can be used with widgets
pub trait WidgetColor: PixelColor + ColorInterpolate {}

// Blanket implementation for any type that satisfies both bounds
impl<C> WidgetColor for C where C: PixelColor + ColorInterpolate {}
