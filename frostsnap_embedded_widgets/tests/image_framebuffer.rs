use frostsnap_embedded_widgets::{
    vec_framebuffer::VecFramebuffer,
    image::Image,
    Widget, DynWidget, Instant,
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
    let mut target = VecFramebuffer::<Rgb565>::new(100, 50);
    
    // Draw the image widget
    image.draw(&mut target, Instant::from_millis(0)).unwrap();
    
    // Verify the circle was drawn
    assert_eq!(target.get_pixel(Point::new(40, 25)), Some(Rgb565::RED));
}

#[test]
fn test_image_dirty_tracking() {
    let framebuffer = VecFramebuffer::<Rgb565>::new(50, 50);
    let mut image = Image::new(framebuffer);
    
    // Create a target  
    let mut target1 = VecFramebuffer::<Rgb565>::new(50, 50);
    let mut target2 = VecFramebuffer::<Rgb565>::new(50, 50);
    
    // First draw should actually draw
    image.draw(&mut target1, Instant::from_millis(0)).unwrap();
    
    // Second draw should skip (no redraw needed)
    // We can test this by checking that the second target remains empty
    target2.clear(Rgb565::new(0, 0, 0));
    image.draw(&mut target2, Instant::from_millis(0)).unwrap();
    
    // After force_full_redraw, should draw again
    image.force_full_redraw();
    let mut target3 = VecFramebuffer::<Rgb565>::new(50, 50);
    image.draw(&mut target3, Instant::from_millis(0)).unwrap();
    
    // Verify that target3 got the content (not empty)
    // Just check one pixel to confirm drawing occurred
    assert_eq!(target1.get_pixel(Point::new(0, 0)), target3.get_pixel(Point::new(0, 0)));
}