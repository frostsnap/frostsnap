use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

pub mod bech32_input_preview;
pub mod bech32_keyboard;
pub mod bip39;
pub mod enter_share_index_screen;
pub mod enter_share_screen;
pub mod icons;
pub mod key_touch;
pub mod memory_debug;
pub mod numeric_keyboard;
pub mod share_index_input;

pub use key_touch::{Key, KeyTouch};

// Re-export all submodule items
pub use bech32_input_preview::*;
pub use bech32_keyboard::*;
pub use bip39::*;
pub use enter_share_index_screen::*;
pub use enter_share_screen::*;
pub use numeric_keyboard::*;
pub use share_index_input::*;

// Font re-exports
use u8g2_fonts::fonts;
pub const FONT_LARGE: fonts::u8g2_font_profont29_mf = fonts::u8g2_font_profont29_mf;
pub const FONT_MED: fonts::u8g2_font_profont22_mf = fonts::u8g2_font_profont22_mf;
pub const FONT_SMALL: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;

/// A trait for drawable widgets that can handle user interactions
pub trait Widget {
    /// Draw the widget to the target
    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>;

    /// Handle touch events. Returns true if the touch was handled.
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<KeyTouch> {
        None
    }

    /// Handle vertical drag events. Returns true if the drag was handled.
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32) {}
}
