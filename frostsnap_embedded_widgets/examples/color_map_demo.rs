//! Example demonstrating ColorMap widget for rendering to different color targets

use embedded_graphics::{
    mock_display::MockDisplay,
    pixelcolor::{BinaryColor, Gray2, Rgb565},
    prelude::*,
};
use frostsnap_embedded_widgets::{
    palette_map::{palette_rgb565_to_binary, palette_rgb565_to_gray2},
    widgets::{checkmark::Checkmark, ColorMap, Widget},
    Instant,
};

fn main() {
    // Create displays with different color modes
    let mut rgb_display = MockDisplay::<Rgb565>::new();
    let mut binary_display = MockDisplay::<BinaryColor>::new();
    let mut gray2_display = MockDisplay::<Gray2>::new();
    
    // Create a checkmark widget (draws in Rgb565)
    let mut checkmark = Checkmark::new(Size::new(50, 50));
    checkmark.start_animation();
    
    let time = Instant::from_millis(500); // Halfway through animation
    
    // Draw directly to RGB display
    checkmark.draw(&mut rgb_display, time).unwrap();
    println!("RGB Display:");
    println!("{}", rgb_display);
    
    // Wrap with ColorMap to draw to binary display
    let mut binary_checkmark = ColorMap::new(checkmark, palette_rgb565_to_binary);
    binary_checkmark.draw(&mut binary_display, time).unwrap();
    println!("\nBinary Display:");
    println!("{}", binary_display);
    
    // Wrap with ColorMap to draw to Gray2 display
    let mut gray2_checkmark = ColorMap::new(
        Checkmark::new(Size::new(50, 50)), // Create new instance
        palette_rgb565_to_gray2,
    );
    gray2_checkmark.draw(&mut gray2_display, time).unwrap();
    println!("\nGray2 Display:");
    println!("{}", gray2_display);
}