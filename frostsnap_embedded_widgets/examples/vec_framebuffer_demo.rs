//! Example demonstrating VecFramebuffer as a DrawTarget

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyleBuilder, Rectangle, Triangle},
    text::{Baseline, Text, TextStyleBuilder},
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
};
use frostsnap_embedded_widgets::vec_framebuffer::VecFramebuffer;

fn main() {
    // Create a 128x64 framebuffer
    let mut display = VecFramebuffer::<Rgb565>::new(128, 64);
    
    // Clear the display with a dark blue background
    display.clear(Rgb565::new(0, 0, 15));
    
    // Draw a red rectangle
    Rectangle::new(Point::new(10, 10), Size::new(30, 20))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(Rgb565::RED)
                .stroke_width(2)
                .fill_color(Rgb565::new(15, 0, 0))
                .build()
        )
        .draw(&mut display)
        .unwrap();
    
    // Draw a green circle
    Circle::new(Point::new(50, 15), 15)
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(Rgb565::GREEN)
                .stroke_width(1)
                .fill_color(Rgb565::new(0, 15, 0))
                .build()
        )
        .draw(&mut display)
        .unwrap();
    
    // Draw a yellow triangle
    Triangle::new(
        Point::new(90, 10),
        Point::new(110, 10),
        Point::new(100, 30),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .stroke_color(Rgb565::YELLOW)
            .stroke_width(1)
            .fill_color(Rgb565::new(15, 15, 0))
            .build()
    )
    .draw(&mut display)
    .unwrap();
    
    // Draw some text
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    
    Text::with_baseline("DrawTarget!", Point::new(10, 50), text_style, Baseline::Top)
        .draw(&mut display)
        .unwrap();
    
    // Draw a diagonal line
    Line::new(Point::new(0, 0), Point::new(127, 63))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(Rgb565::CYAN)
                .stroke_width(1)
                .build()
        )
        .draw(&mut display)
        .unwrap();
    
    println!("Framebuffer created with {} bytes", display.data.len());
    println!("Drew various shapes using the DrawTarget trait");
    
    // Demonstrate reading back pixels
    if let Some(color) = display.get_pixel(Point::new(25, 20)) {
        println!("Pixel at (25, 20): R={}, G={}, B={}", 
            color.r(), color.g(), color.b());
    }
}