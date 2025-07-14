#![no_std]

extern crate alloc;

use embedded_graphics::prelude::*;

pub mod palette;
pub mod pixel_recorder;

// Widget modules
pub mod bip39;
pub mod center;
pub mod checkmark;
pub mod color_map;
pub mod column;
pub mod hold_to_confirm;
pub mod hold_to_confirm_button;
pub mod icons;
pub mod key_touch;
pub mod legacy;
pub mod memory_debug;
pub mod row;
pub mod sized_box;
pub mod text;

// Re-export key types
pub use key_touch::{Key, KeyTouch};

// Re-export all widget items
pub use bip39::*;
pub use center::*;
pub use checkmark::*;
pub use color_map::*;
pub use column::*;
pub use hold_to_confirm::*;
pub use hold_to_confirm_button::*;
pub use row::*;
pub use sized_box::*;
pub use text::*;

// Font re-exports
use u8g2_fonts::fonts;
pub const FONT_LARGE: fonts::u8g2_font_profont29_mf = fonts::u8g2_font_profont29_mf;
pub const FONT_MED: fonts::u8g2_font_profont22_mf = fonts::u8g2_font_profont22_mf;
pub const FONT_SMALL: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;

/// A trait for drawable widgets that can handle user interactions
pub trait Widget {
    /// The color type this widget natively draws in
    type Color: PixelColor;
    
    /// Draw the widget to the target
    fn draw<D: DrawTarget<Color = Self::Color>>(
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

    /// Get the preferred size of this widget, if it has one
    fn size_hint(&self) -> Option<Size> {
        None
    }
}

/// Milliseconds since device start
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(u64);

impl Instant {
    /// Create from milliseconds
    pub fn from_millis(millis: u64) -> Self {
        Self(millis)
    }
    
    /// Get milliseconds value
    pub fn as_millis(&self) -> u64 {
        self.0
    }
    
    /// Calculate duration since an earlier instant
    /// Returns None if earlier is later than self
    pub fn duration_since(&self, earlier: Instant) -> Option<u64> {
        self.0.checked_sub(earlier.0)
    }
    
    /// Calculate duration since an earlier instant, saturating at 0
    pub fn saturating_duration_since(&self, earlier: Instant) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}