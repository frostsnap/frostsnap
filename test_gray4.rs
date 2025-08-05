use embedded_graphics::pixelcolor::{Gray2, Gray4, Gray8};

fn main() {
    // Test if Gray4 exists
    let _g2 = Gray2::new(3);  // 4 levels (0-3)
    let _g4 = Gray4::new(15); // 16 levels (0-15)
    let _g8 = Gray8::new(255); // 256 levels (0-255)
    
    println!("Gray types available!");
}