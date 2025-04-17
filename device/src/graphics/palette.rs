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
}

// Default color theme
pub const COLORS: Palette = Palette {
    primary: Rgb565::WHITE,
    secondary: Rgb565::new(11, 27, 16),
    success: Rgb565::new(13, 52, 14),
    info: Rgb565::CYAN,
    warning: Rgb565::CSS_RED,
    error: Rgb565::RED,
    background: Rgb565::BLACK,
    disabled: Rgb565::CSS_DARK_GRAY,
};
