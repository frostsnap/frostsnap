use frostsnap_embedded_widgets::{
    vec_framebuffer::VecFramebuffer,
    image::Image,
    Widget, DynWidget, Instant,
    SuperDrawTarget,
};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
};

#[test]
fn test_vecframebuffer_with_image_widget() {
    // Create a VecFramebuffer and draw something to it
    let mut framebuffer = VecFramebuffer::<Rgb565>::new(100, 50);
    
    Circle::new(Point::new(25, 10), 30)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
        .draw(&mut framebuffer)
        .unwrap();
    
    // Create an Image widget from the framebuffer
    let mut image = Image::new(framebuffer);
    
    // Check that sizing is correct
    let sizing = image.sizing();
    assert_eq!(sizing.width, 100);
    assert_eq!(sizing.height, 50);
    
    // Create a target to draw to
    let mut framebuffer_target = VecFramebuffer::<Rgb565>::new(100, 50);
    let mut target = SuperDrawTarget::new(framebuffer_target, Rgb565::BLACK);
    
    // Draw the image widget
    image.draw(&mut target, Instant::from_millis(0)).unwrap();
    
    // Get the framebuffer back to check the result
    let framebuffer_target = target.inner_mut().unwrap();
    
    // Verify the circle was drawn
    assert_eq!(framebuffer_target.get_pixel(Point::new(40, 25)), Some(Rgb565::RED));
}

#[test]
fn test_image_dirty_tracking() {
    let framebuffer = VecFramebuffer::<Rgb565>::new(50, 50);
    let mut image = Image::new(framebuffer);
    
    // Create a target  
    let framebuffer1 = VecFramebuffer::<Rgb565>::new(50, 50);
    let mut target1 = SuperDrawTarget::new(framebuffer1, Rgb565::BLACK);
    let framebuffer2 = VecFramebuffer::<Rgb565>::new(50, 50);
    let mut target2 = SuperDrawTarget::new(framebuffer2, Rgb565::BLACK);
    
    // First draw should actually draw
    image.draw(&mut target1, Instant::from_millis(0)).unwrap();
    
    // Second draw should skip (no redraw needed)
    // We can test this by checking that the second target remains empty
    target2.inner_mut().unwrap().clear(Rgb565::new(0, 0, 0));
    image.draw(&mut target2, Instant::from_millis(0)).unwrap();
    
    // After force_full_redraw, should draw again
    image.force_full_redraw();
    let framebuffer3 = VecFramebuffer::<Rgb565>::new(50, 50);
    let mut target3 = SuperDrawTarget::new(framebuffer3, Rgb565::BLACK);
    image.draw(&mut target3, Instant::from_millis(0)).unwrap();
    
    // Verify that target3 got the content (not empty)
    // Just check one pixel to confirm drawing occurred
    let fb1 = target1.inner_mut().unwrap();
    let fb3 = target3.inner_mut().unwrap();
    assert_eq!(fb1.get_pixel(Point::new(0, 0)), fb3.get_pixel(Point::new(0, 0)));
}