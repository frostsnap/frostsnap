use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::{RgbColor, WebColors},
};

#[allow(unused)]
pub struct Palette {
    pub(crate) primary: Rgb565,
    pub(crate) secondary: Rgb565,
    pub(crate) success: Rgb565,
    pub(crate) info: Rgb565,
    pub(crate) warning: Rgb565,
    pub(crate) error: Rgb565,
    pub(crate) background: Rgb565,
    pub(crate) disabled: Rgb565,
    pub(crate) hold_border: Rgb565,
    pub(crate) hold_progress: Rgb565,
}

// Default color theme
pub const COLORS: Palette = Palette {
    primary: Rgb565::WHITE,
    secondary: Rgb565::new(11, 27, 16),
    success: Rgb565::new(13, 52, 14),
    info: Rgb565::CYAN,
    warning: Rgb565::CSS_ORANGE,
    error: Rgb565::RED,
    background: Rgb565::BLACK,
    disabled: Rgb565::CSS_DARK_GRAY,
    hold_border: Rgb565::new(4, 8, 4), // Dark gray in RGB565
    hold_progress: Rgb565::new(3, 37, 22),
};

pub struct MaterialDarkPalette565 {
    pub primary: Rgb565,
    pub on_primary: Rgb565,
    pub primary_container: Rgb565,
    pub on_primary_container: Rgb565,

    pub secondary: Rgb565,
    pub on_secondary: Rgb565,
    pub secondary_container: Rgb565,
    pub on_secondary_container: Rgb565,

    pub tertiary: Rgb565,
    pub on_tertiary: Rgb565,
    pub tertiary_container: Rgb565,
    pub on_tertiary_container: Rgb565,

    pub background: Rgb565,
    pub on_background: Rgb565,
    pub surface: Rgb565,
    pub on_surface: Rgb565,
    pub surface_variant: Rgb565,
    pub on_surface_variant: Rgb565,

    pub outline: Rgb565,
    pub error: Rgb565,
    pub on_error: Rgb565,
}

/// `const` instance – drop it in any module and `use PALETTE.<role>`.
pub const PALETTE: MaterialDarkPalette565 = MaterialDarkPalette565 {
    /*               R     G     B    */
    primary: Rgb565::new(21, 55, 29),          // tone 80  ≈ #ADE0EB
    on_primary: Rgb565::new(1, 19, 11),        // tone 20  ≈ #0A4D5C
    primary_container: Rgb565::new(2, 29, 17), // tone 30  ≈ #0F748A
    on_primary_container: Rgb565::new(26, 59, 29), // tone 90  ≈ #D9EEF2

    secondary: Rgb565::new(22, 43, 29),  // tone 80  ≈ #B8ADEB
    on_secondary: Rgb565::new(4, 3, 14), // tone 20  ≈ #1E0D73
    secondary_container: Rgb565::new(6, 8, 15), // tone 30  ≈ #2E1F7A
    on_secondary_container: Rgb565::new(27, 54, 29), // tone 90  ≈ #DFDBF0

    tertiary: Rgb565::new(21, 58, 25),         // tone 80  ≈ #ADEBCC
    on_tertiary: Rgb565::new(1, 23, 6),        // tone 20  ≈ #0A5C33
    tertiary_container: Rgb565::new(2, 34, 9), // tone 30  ≈ #0F8A4D
    on_tertiary_container: Rgb565::new(27, 59, 28), // tone 90  ≈ #DBF0E6

    background: Rgb565::new(0, 0, 0),       // tone 6   ≈ #0E1111
    on_background: Rgb565::new(28, 57, 28), // tone 90  ≈ #E3E7E8
    surface: Rgb565::new(2, 4, 2),          // same tone as background
    on_surface: Rgb565::new(28, 57, 28),
    surface_variant: Rgb565::new(7, 21, 11), // tone 30  neutral‑variant
    on_surface_variant: Rgb565::new(24, 52, 26), // tone 80

    outline: Rgb565::new(16, 41, 21), // tone 60  ≈ #85A6AD
    error: Rgb565::new(29, 45, 22),   // tone 80  ≈ #F2B8B5
    on_error: Rgb565::new(12, 5, 2),  // tone 20  ≈ #601410
};
