//! Touch calibration functions for adjusting touch coordinates on the device
//! These polynomial functions correct for touch sensor inaccuracies

use micromath::F32Ext;

/// Applies x-axis based adjustment to correct touch coordinates
pub fn x_based_adjustment(x: i32) -> i32 {
    let x = x as f32;
    let corrected = 1.3189e-14 * x.powi(7) - 2.1879e-12 * x.powi(6) - 7.6483e-10 * x.powi(5)
        + 3.2578e-8 * x.powi(4)
        + 6.4233e-5 * x.powi(3)
        - 1.2229e-2 * x.powi(2)
        + 0.8356 * x
        - 20.0;
    (-corrected) as i32
}

/// Applies y-axis based adjustment to correct touch coordinates
pub fn y_based_adjustment(y: i32) -> i32 {
    if y > 170 {
        return 0;
    }
    let y = y as f32;
    let corrected =
        -5.5439e-07 * y.powi(4) + 1.7576e-04 * y.powi(3) - 1.5104e-02 * y.powi(2) - 2.3443e-02 * y
            + 40.0;
    (-corrected) as i32
}

/// Applies both x and y adjustments to a touch point
pub fn adjust_touch_point(x: i32, y: i32) -> (i32, i32) {
    let corrected_y = y + x_based_adjustment(x) + y_based_adjustment(y);
    (x, corrected_y)
}
