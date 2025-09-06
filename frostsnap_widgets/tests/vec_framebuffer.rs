//! Tests for VecFramebuffer

use embedded_graphics::{
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, Gray8, Rgb565},
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
};
use frostsnap_widgets::vec_framebuffer::VecFramebuffer;

#[test]
fn test_framebuffer_creation_rgb565() {
    let fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(240, 280);
    assert_eq!(fb.width, 240);
    assert_eq!(fb.height, 280);
    assert_eq!(fb.data.len(), 240 * 280 * 2); // 2 bytes per pixel
}

#[test]
fn test_framebuffer_creation_gray8() {
    let fb: VecFramebuffer<Gray8> = VecFramebuffer::new(100, 100);
    assert_eq!(fb.width, 100);
    assert_eq!(fb.height, 100);
    assert_eq!(fb.data.len(), 100 * 100); // 1 byte per pixel
}

#[test]
fn test_framebuffer_creation_binary() {
    let fb: VecFramebuffer<BinaryColor> = VecFramebuffer::new(128, 64);
    assert_eq!(fb.width, 128);
    assert_eq!(fb.height, 64);
    assert_eq!(fb.data.len(), 128 * 64 / 8); // 1 bit per pixel
}

#[test]
fn test_pixel_setting_rgb565() {
    let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(10, 10);
    fb.set_pixel(Point::new(5, 5), Rgb565::RED);

    // Verify the pixel was set
    let color = fb.get_pixel(Point::new(5, 5));
    assert_eq!(color, Some(Rgb565::RED));
}

#[test]
fn test_pixel_setting_gray8() {
    let mut fb: VecFramebuffer<Gray8> = VecFramebuffer::new(10, 10);
    let gray_val = Gray8::new(128);
    fb.set_pixel(Point::new(3, 3), gray_val);

    // Verify the pixel was set
    let color = fb.get_pixel(Point::new(3, 3));
    assert_eq!(color, Some(gray_val));
}

#[test]
fn test_bounds_checking() {
    let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(10, 10);

    // This should not panic
    fb.set_pixel(Point::new(100, 100), Rgb565::RED);

    // This should return None
    let color = fb.get_pixel(Point::new(100, 100));
    assert_eq!(color, None);
}

#[test]
fn test_negative_coordinates() {
    let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(10, 10);

    // This should not panic
    fb.set_pixel(Point::new(-5, -5), Rgb565::BLUE);

    // This should return None
    let color = fb.get_pixel(Point::new(-5, -5));
    assert_eq!(color, None);
}

#[test]
fn test_fill_rect() {
    let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(20, 20);

    let rect = Rectangle::new(Point::new(5, 5), Size::new(10, 10));
    fb.fill_rect(rect, Rgb565::GREEN);

    // Check that pixels inside the rect are green
    assert_eq!(fb.get_pixel(Point::new(7, 7)), Some(Rgb565::GREEN));
    assert_eq!(fb.get_pixel(Point::new(14, 14)), Some(Rgb565::GREEN));

    // Check that pixels outside the rect are black (default)
    assert_eq!(fb.get_pixel(Point::new(4, 4)), Some(Rgb565::BLACK));
    assert_eq!(fb.get_pixel(Point::new(15, 15)), Some(Rgb565::BLACK));
}

#[test]
fn test_clear() {
    let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(10, 10);

    // Set some pixels
    fb.set_pixel(Point::new(5, 5), Rgb565::RED);
    fb.set_pixel(Point::new(7, 7), Rgb565::BLUE);

    // Clear with green
    fb.clear(Rgb565::GREEN);

    // All pixels should be green
    for y in 0..10 {
        for x in 0..10 {
            assert_eq!(fb.get_pixel(Point::new(x, y)), Some(Rgb565::GREEN));
        }
    }
}

#[test]
fn test_draw_target() {
    let mut fb: VecFramebuffer<Rgb565> = VecFramebuffer::new(50, 50);

    // Draw a filled circle
    let circle =
        Circle::new(Point::new(10, 10), 30).into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN));

    circle.draw(&mut fb).unwrap();

    // Check that some pixels inside the circle are cyan
    assert_eq!(fb.get_pixel(Point::new(25, 25)), Some(Rgb565::CYAN));

    // Draw a rectangle
    use embedded_graphics::primitives::Primitive;
    let rect = Rectangle::new(Point::new(0, 0), Size::new(10, 10))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED));

    rect.draw(&mut fb).unwrap();

    // Check that pixels in the rectangle are red
    assert_eq!(fb.get_pixel(Point::new(5, 5)), Some(Rgb565::RED));

    // Check that the circle pixel is still cyan (wasn't overwritten)
    assert_eq!(fb.get_pixel(Point::new(25, 25)), Some(Rgb565::CYAN));
}

#[test]
fn test_draw_target_binary() {
    use embedded_graphics::primitives::{Line, Primitive};

    let mut fb: VecFramebuffer<BinaryColor> = VecFramebuffer::new(20, 20);

    // Draw a line
    let line = Line::new(Point::new(0, 0), Point::new(19, 19))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1));

    line.draw(&mut fb).unwrap();

    // Check some points on the diagonal
    assert_eq!(fb.get_pixel(Point::new(0, 0)), Some(BinaryColor::On));
    assert_eq!(fb.get_pixel(Point::new(10, 10)), Some(BinaryColor::On));
    assert_eq!(fb.get_pixel(Point::new(19, 19)), Some(BinaryColor::On));

    // Check a point off the diagonal
    assert_eq!(fb.get_pixel(Point::new(0, 1)), Some(BinaryColor::Off));
}
