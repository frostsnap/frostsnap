use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use frostsnap_widgets::{ColorInterpolate, Frac};

/// How a channel blended when `Frac` was decimal: a fraction in 1e-4 steps, each term rounded half-up
/// on its own.
fn decimal_lerp(numerator: u32, denominator: u32, from: u32, to: u32) -> u32 {
    let frac = ((numerator * 10_000 + denominator / 2) / denominator).min(10_000);
    let round = |scaled: u32| (scaled + 5_000) / 10_000;
    round((10_000 - frac) * from) + round(frac * to)
}

fn green(value: u32) -> Rgb565 {
    Rgb565::new(0, value as u8, 0)
}

#[test]
fn blend_stays_between_its_ends_and_within_one_lsb_of_decimal() {
    for denominator in [15, 255, 256] {
        for numerator in 0..=denominator {
            let frac = Frac::from_ratio(numerator, denominator);
            for from in 0..64 {
                for to in 0..64 {
                    let old = decimal_lerp(numerator, denominator, from, to);
                    let new = green(from).interpolate(green(to), frac).g() as u32;
                    let case = format!("{numerator}/{denominator} of {from}->{to}");
                    assert!(old.abs_diff(new) <= 1, "{case}: {old} vs {new}");
                    assert!(
                        (from.min(to)..=from.max(to)).contains(&new),
                        "{case}: {new}"
                    );
                }
            }
        }
    }
}

#[test]
fn half_blend_of_a_full_channel_does_not_wrap() {
    let half = Frac::from_ratio(128, 256);
    assert_eq!(green(63).interpolate(green(63), half).g(), 63);
    let white = Rgb565::new(31, 63, 31);
    assert_eq!(white.interpolate(white, half), white);
}

#[test]
fn chained_blends_within_one_lsb_of_decimal() {
    for (d1, d2) in [(15, 15), (256, 15), (256, 256)] {
        for n1 in (0..=d1).step_by(d1 as usize / 15) {
            for n2 in (0..=d2).step_by(d2 as usize / 15) {
                let (f1, f2) = (Frac::from_ratio(n1, d1), Frac::from_ratio(n2, d2));
                for from in 0..64 {
                    for mid in (0..64).step_by(3) {
                        let old_mid = decimal_lerp(n1, d1, from, mid);
                        let new_mid = green(from).interpolate(green(mid), f1);
                        for to in (0..64).step_by(5) {
                            let old = decimal_lerp(n2, d2, old_mid, to);
                            let new = new_mid.interpolate(green(to), f2).g() as u32;
                            assert!(
                                old.abs_diff(new) <= 1,
                                "{n1}/{d1} of {from}->{mid}, then {n2}/{d2} to {to}: {old} vs {new}"
                            );
                        }
                    }
                }
            }
        }
    }
    // Rounding the blend's sum once, rather than each term, drifts to 44 here.
    let mid = green(42).interpolate(green(43), Frac::from_ratio(12, 15));
    assert_eq!(mid.interpolate(green(47), Frac::from_ratio(2, 15)).g(), 42);
}
