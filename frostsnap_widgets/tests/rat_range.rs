//! Where `Rat` (U16F16, below 65,536) and `Frac` (U1F15) run out of range or resolution.
use embedded_graphics::geometry::Point;
use frostsnap_widgets::{Frac, Rat};

#[test]
fn rat_saturates_at_its_range() {
    assert_eq!(Rat::from_int(65_535) + Rat::ONE, Rat::MAX);
    assert_eq!(Rat::from_int(70_000), Rat::MAX);
    assert_eq!(Rat::from_ratio(70_000, 1), Rat::MAX);
    assert_eq!(Rat::from_int(40_000) * 2u32, Rat::MAX);
    assert_eq!(Rat::from_int(300) * Rat::from_int(300), Rat::MAX);
    assert_eq!(Rat::from_int(1) - Rat::from_int(2), Rat::ZERO);
    assert_eq!(Rat::MAX.round(), 65_536);
}

#[test]
fn integer_minus_rat_clamps_the_integer_first() {
    assert_eq!(
        70_000 - Rat::from_int(10_000),
        Rat::MAX - Rat::from_int(10_000)
    );
    assert_eq!(1 - Rat::from_int(2), Rat::ZERO);
}

#[test]
fn rat_times_i32_rounds_toward_zero() {
    assert_eq!(Rat::from_ratio(1, 3) * -1, 0);
    assert_eq!(Rat::from_int(2) * i32::MAX, (2 * i32::MAX as i64) as i32);
    assert_eq!(Rat::from_ratio(1, 2) * Point::new(7, -7), Point::new(3, -3));
}

#[test]
fn frac_from_ratio_handles_denominators_past_16_bits() {
    // OTA progress: bytes of a firmware image over its size.
    assert_eq!(Frac::from_ratio(700_000, 1_400_000), Frac::from_ratio(1, 2));
    assert_eq!(Frac::from_ratio(u32::MAX - 1, u32::MAX), Frac::ONE);
    assert!(Frac::from_ratio(1, 1_400_000) == Frac::ZERO);
    assert!(Frac::from_ratio(22, 1_400_000) > Frac::ZERO);
    assert_eq!(Frac::from_ratio(1, 0), Frac::ONE);
}

#[test]
fn frac_ratio_and_large_products() {
    let smallest = Frac::from_ratio(1, 32_768);
    assert!(smallest > Frac::ZERO);
    assert_eq!(Frac::ONE / smallest, Rat::from_int(32_768));
    assert_eq!(Frac::ONE / Frac::ZERO, Rat::MAX);
    // A screen perimeter in the rounded rect's scaled units is ~230,000, beyond Rat.
    assert_eq!(Frac::from_ratio(1, 2).mul_floor(230_000), 115_000);
    assert_eq!(Frac::ONE.mul_floor(230_000), 230_000);
}
