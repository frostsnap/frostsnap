use frostsnap_widgets::rat::{Frac, Rat};

#[test]
fn test_rat_from_ratio() {
    // Test basic ratios
    let half = Rat::from_ratio(1, 2);
    let quarter = Rat::from_ratio(1, 4);
    let three_quarters = Rat::from_ratio(3, 4);
    let two = Rat::from_ratio(2, 1);

    // Can't access internal values directly anymore, so test via Display
    assert_eq!(half.to_string(), "0.5");
    assert_eq!(quarter.to_string(), "0.25");
    assert_eq!(three_quarters.to_string(), "0.75");
    assert_eq!(two.to_string(), "2");

    // Test edge cases
    let zero = Rat::from_ratio(0, 100);
    assert_eq!(zero.to_string(), "0");

    // Division by zero should give a very large value
    let div_by_zero = Rat::from_ratio(1, 0);
    assert!(div_by_zero > Rat::from_ratio(1000000, 1));
}

#[test]
fn test_rat_add() {
    let half = Rat::from_ratio(1, 2);
    let quarter = Rat::from_ratio(1, 4);
    let three_quarters = half + quarter;
    assert_eq!(three_quarters, Rat::from_ratio(3, 4));
}

#[test]
fn test_rat_sub() {
    let three_quarters = Rat::from_ratio(3, 4);
    let quarter = Rat::from_ratio(1, 4);
    let half = three_quarters - quarter;
    assert_eq!(half, Rat::from_ratio(1, 2));

    // Test underflow protection
    let small = Rat::from_ratio(1, 4);
    let large = Rat::from_ratio(3, 4);
    let result = small - large;
    assert_eq!(result, Rat::ZERO);
}

#[test]
fn test_rat_mul_u32() {
    let half = Rat::from_ratio(1, 2);
    assert_eq!(half * 100, 50);
    assert_eq!(100 * half, 50);

    let quarter = Rat::from_ratio(1, 4);
    assert_eq!(quarter * 100, 25);
    assert_eq!(100 * quarter, 25);
}

#[test]
fn test_rat_mul_i32() {
    let half = Rat::from_ratio(1, 2);
    assert_eq!(half * -100, -50);
    assert_eq!(-100 * half, -50);

    let quarter = Rat::from_ratio(1, 4);
    assert_eq!(quarter * -100, -25);
    assert_eq!(-100 * quarter, -25);
}

#[test]
fn test_rat_mul_rat() {
    let half = Rat::from_ratio(1, 2);
    let quarter = Rat::from_ratio(1, 4);
    let eighth = half * quarter;
    assert_eq!(eighth, Rat::from_ratio(1, 8));

    // Test identity
    let one = Rat::ONE;
    assert_eq!(half * one, half);

    // Test zero
    let zero = Rat::ZERO;
    assert_eq!(half * zero, Rat::ZERO);
}

#[test]
fn test_rat_div() {
    let half = Rat::from_ratio(1, 2);
    assert_eq!(half / 2, 2_500); // Internal representation detail

    let three_quarters = Rat::from_ratio(3, 4);
    assert_eq!(three_quarters / 3, 2_500);
}

#[test]
fn test_rat_display() {
    assert_eq!(Rat::from_ratio(1, 2).to_string(), "0.5");
    assert_eq!(Rat::from_ratio(1, 4).to_string(), "0.25");
    assert_eq!(Rat::from_ratio(3, 4).to_string(), "0.75");
    assert_eq!(Rat::from_ratio(5, 4).to_string(), "1.25");
    assert_eq!(Rat::from_ratio(2, 1).to_string(), "2");
    assert_eq!(Rat::ZERO.to_string(), "0");
    assert_eq!(Rat::ONE.to_string(), "1");
    // Test a value with trailing zeros
    assert_eq!(Rat::from_ratio(1, 10).to_string(), "0.1");
    assert_eq!(Rat::from_ratio(1, 8).to_string(), "0.125");
}

#[test]
fn test_rat_debug() {
    assert_eq!(format!("{:?}", Rat::from_ratio(1, 2)), "5000/10000");
    assert_eq!(format!("{:?}", Rat::from_ratio(1, 4)), "2500/10000");
    assert_eq!(format!("{:?}", Rat::ONE), "10000/10000");
}

#[test]
fn test_frac_clamping() {
    // Test normal values
    let half_rat = Rat::from_ratio(1, 2);
    let half_frac = Frac::new(half_rat);
    assert_eq!(half_frac.as_rat(), half_rat);

    // Test clamping values > 1
    let two = Rat::from_ratio(2, 1);
    let clamped = Frac::new(two);
    assert_eq!(clamped, Frac::ONE);

    // Test from_ratio with values > 1
    let over_one = Frac::from_ratio(3, 2);
    assert_eq!(over_one, Frac::ONE);
}

#[test]
fn test_frac_add() {
    let quarter = Frac::from_ratio(1, 4);
    let half = Frac::from_ratio(1, 2);
    let three_quarters = quarter + half;
    assert_eq!(three_quarters, Frac::from_ratio(3, 4));

    // Test clamping on overflow
    let three_quarters = Frac::from_ratio(3, 4);
    let half = Frac::from_ratio(1, 2);
    let result = three_quarters + half;
    assert_eq!(result, Frac::ONE);
}

#[test]
fn test_frac_sub() {
    let three_quarters = Frac::from_ratio(3, 4);
    let quarter = Frac::from_ratio(1, 4);
    let half = three_quarters - quarter;
    assert_eq!(half, Frac::from_ratio(1, 2));

    // Test clamping at 0
    let small = Frac::from_ratio(1, 4);
    let large = Frac::from_ratio(3, 4);
    let zero = small - large;
    assert_eq!(zero, Frac::ZERO);
}

#[test]
fn test_frac_display() {
    assert_eq!(Frac::from_ratio(1, 2).to_string(), "0.5");
    assert_eq!(Frac::from_ratio(1, 4).to_string(), "0.25");
    assert_eq!(Frac::from_ratio(3, 4).to_string(), "0.75");
    assert_eq!(Frac::ONE.to_string(), "1");
    assert_eq!(Frac::ZERO.to_string(), "0");
}

#[test]
fn test_frac_debug() {
    assert_eq!(format!("{:?}", Frac::from_ratio(1, 2)), "Frac(5000/10000)");
    assert_eq!(format!("{:?}", Frac::ONE), "Frac(10000/10000)");
}

#[test]
fn test_small_frac() {
    let nn = Frac::from_ratio(99, 100);
    let one = Frac::from_ratio(1, 100);

    assert_eq!((nn * 5u32) + (one * 5u32), 5 * Frac::ONE);
}
