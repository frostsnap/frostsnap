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
    pub caution: Rgb565,

    pub confirm_progress: Rgb565,
    pub logo: Rgb565,

    /// Secondary text color - darker gray for labels and supporting text
    /// Following Material Design ~60% opacity guideline (medium emphasis)
    pub text_secondary: Rgb565,

    /// Disabled text color - very dark gray for de-emphasized content
    /// Following Material Design ~38% opacity guideline (disabled state)
    pub text_disabled: Rgb565,
}

pub const PALETTE: MaterialDarkPalette565 = MaterialDarkPalette565 {
    primary: Rgb565::new(2, 37, 22),
    on_primary: Rgb565::new(31, 63, 31),
    primary_container: Rgb565::new(1, 26, 16),
    on_primary_container: Rgb565::new(28, 60, 28),

    secondary: Rgb565::new(22, 43, 29),
    on_secondary: Rgb565::new(4, 3, 14),
    secondary_container: Rgb565::new(6, 8, 15),
    on_secondary_container: Rgb565::new(27, 54, 29),

    tertiary: Rgb565::new(21, 58, 25),
    on_tertiary: Rgb565::new(1, 23, 6),
    tertiary_container: Rgb565::new(2, 34, 9),
    on_tertiary_container: Rgb565::new(27, 59, 28),

    background: Rgb565::new(1, 2, 2),
    on_background: Rgb565::new(28, 57, 28),
    surface: Rgb565::new(2, 4, 2),
    on_surface: Rgb565::new(28, 57, 28),
    surface_variant: Rgb565::new(6, 16, 10),
    on_surface_variant: Rgb565::new(25, 54, 27),

    outline: Rgb565::new(16, 41, 21),
    error: Rgb565::new(31, 12, 6),  // Proper red for errors
    on_error: Rgb565::new(12, 5, 2),
    caution: Rgb565::new(31, 55, 0),  // Yellow/amber for cautions

    confirm_progress: Rgb565::new(3, 46, 16),
    logo: Rgb565::new(0, 55, 30),

    // Medium gray for secondary text (~60% of white - Material Design medium emphasis)
    // RGB565: 5 bits red, 6 bits green, 5 bits blue
    // ~60% would be: R=19/31, G=38/63, B=19/31
    text_secondary: Rgb565::new(19, 38, 19),

    // Dark gray for disabled text (~38% of white - Material Design disabled state)
    // ~38% would be: R=12/31, G=24/63, B=12/31
    text_disabled: Rgb565::new(12, 24, 12),
};
