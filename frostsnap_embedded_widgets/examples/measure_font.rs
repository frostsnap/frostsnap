use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use u8g2_fonts::{fonts, U8g2TextStyle};

fn main() {
    // Create text style with ProFont17
    let character_style = U8g2TextStyle::new(
        fonts::u8g2_font_profont17_mf,
        BinaryColor::On,
    );
    
    // Measure different strings
    let strings = [
        "FPS: 0",
        "FPS: 999",
        "Mem: 0", 
        "Mem: 262144",
    ];
    
    for s in &strings {
        let text = Text::new(s, Point::zero(), character_style.clone());
        let bounds = text.bounding_box();
        println!("{:15} -> width: {} pixels", s, bounds.size.width);
    }
    
    // Also calculate character width
    let single_char = Text::new("M", Point::zero(), character_style.clone());
    let char_bounds = single_char.bounding_box();
    println!("\nSingle 'M' char -> width: {} pixels", char_bounds.size.width);
    
    // Check a full line of each
    for i in 1..=15 {
        let s = "M".repeat(i);
        let text = Text::new(&s, Point::zero(), character_style.clone());
        let bounds = text.bounding_box();
        println!("{} chars -> width: {} pixels", i, bounds.size.width);
    }
}