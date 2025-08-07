use embedded_graphics::pixelcolor::Gray2;

/// Semantic color constants for Gray2 displays following Material Design principles
/// Gray2 has 4 levels: 0 (black) to 3 (white)
pub struct GrayTheme;

impl GrayTheme {
    /// Background color (black)
    pub const BACKGROUND: Gray2 = Gray2::new(0);
    
    /// Primary text color (high emphasis - ~87% opacity equivalent)
    pub const TEXT_PRIMARY: Gray2 = Gray2::new(3);
    
    /// Secondary text color (medium emphasis - ~60% opacity equivalent)
    /// Used for supporting text, labels, captions
    pub const TEXT_SECONDARY: Gray2 = Gray2::new(1);
    
    /// Accent/Primary color for important UI elements
    /// Used for significant digits, primary actions
    pub const ACCENT: Gray2 = Gray2::new(2);
    
    /// Surface color (slightly lighter than background)
    pub const SURFACE: Gray2 = Gray2::new(0);
    
    /// Divider/border color
    pub const DIVIDER: Gray2 = Gray2::new(1);
}

// Convenience constants
pub const GRAY_BACKGROUND: Gray2 = GrayTheme::BACKGROUND;
pub const GRAY_TEXT_PRIMARY: Gray2 = GrayTheme::TEXT_PRIMARY;
pub const GRAY_TEXT_SECONDARY: Gray2 = GrayTheme::TEXT_SECONDARY;
pub const GRAY_ACCENT: Gray2 = GrayTheme::ACCENT;