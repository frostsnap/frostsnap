use embedded_graphics::pixelcolor::Rgb565;

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

    pub confirm_progress: Rgb565,
}

/// `const` instance – drop it in any module and `use PALETTE.<role>`.
pub const PALETTE: MaterialDarkPalette565 = MaterialDarkPalette565 {
    primary: Rgb565::new(21, 55, 29),
    on_primary: Rgb565::new(1, 19, 11),
    primary_container: Rgb565::new(2, 29, 17),
    on_primary_container: Rgb565::new(26, 59, 29),

    secondary: Rgb565::new(22, 43, 29),
    on_secondary: Rgb565::new(4, 3, 14),
    secondary_container: Rgb565::new(6, 8, 15),
    on_secondary_container: Rgb565::new(27, 54, 29),

    tertiary: Rgb565::new(21, 58, 25),
    on_tertiary: Rgb565::new(1, 23, 6),
    tertiary_container: Rgb565::new(2, 34, 9),
    on_tertiary_container: Rgb565::new(27, 59, 28),

    background: Rgb565::new(0, 0, 0),
    on_background: Rgb565::new(28, 57, 28),
    surface: Rgb565::new(2, 4, 2),
    on_surface: Rgb565::new(28, 57, 28),
    surface_variant: Rgb565::new(6, 16, 10),
    on_surface_variant: Rgb565::new(25, 54, 27),

    outline: Rgb565::new(16, 41, 21),
    error: Rgb565::new(29, 45, 22),
    on_error: Rgb565::new(12, 5, 2),

    confirm_progress: Rgb565::new(2, 46, 16),
};
