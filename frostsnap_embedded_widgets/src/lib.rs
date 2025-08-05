#![no_std]
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[allow(unused)]
#[macro_use]
pub extern crate alloc;

use embedded_graphics::prelude::*;

pub mod palette;
pub mod pixel_recorder;
pub mod compressed_point;

// Widget modules
pub mod animation;
pub mod bip39;
pub mod bitmap;
pub mod center;
pub mod checkmark;
pub mod color_map;
pub mod column;
pub mod cursor;
pub mod fader;
pub mod rat;
pub mod hold_to_confirm;
pub mod hold_to_confirm_border;
pub mod image;
pub mod icons;
pub mod key_touch;
pub mod legacy;
pub mod memory_debug;
pub mod page_by_page;
pub mod page_demo;
pub mod paginator_with_scrollbar;
pub mod progress_bars;
pub mod row;
pub mod buffered;
pub mod scroll_bar;
pub mod sized_box;
pub mod container;
pub mod swipe_up_chevron;
pub mod text;
pub mod mut_text;
pub mod translate;
pub mod welcome;
pub mod device_name;
pub mod bobbing_carat;
pub mod keygen_check;
pub mod padding;
pub mod either;
pub mod circle_button;
pub mod fade_switcher;
pub mod widget_tuple;
pub mod select_widget;
pub mod bitcoin_amount_display;
pub mod sign_prompt;

// Re-export key types
pub use key_touch::{Key, KeyTouch};
pub use page_by_page::PageByPage;
pub use page_demo::PageDemo;
pub use sign_prompt::SignPrompt;

// Re-export all widget items
pub use animation::*;
pub use bip39::*;
pub use center::*;
pub use checkmark::*;
pub use color_map::*;
pub use container::*;
pub use column::*;
pub use cursor::*;
pub use fader::*;
pub use rat::{Rat, Frac};
pub use hold_to_confirm::HoldToConfirm;
pub use hold_to_confirm_border::HoldToConfirmBorder;
// pub use hold_to_confirm_button::*;
pub use paginator_with_scrollbar::*;
pub use progress_bars::*;
pub use row::*;
pub use scroll_bar::*;
pub use sized_box::*;
pub use swipe_up_chevron::*;
pub use text::*;
pub use mut_text::*;
pub use welcome::*;
pub use device_name::*;
pub use keygen_check::*;
pub use padding::*;
pub use either::Either;
pub use circle_button::*;
pub use fade_switcher::FadeSwitcher;

// Font re-exports
use u8g2_fonts::fonts;
pub const FONT_LARGE: fonts::u8g2_font_profont29_mf = fonts::u8g2_font_profont29_mf;
pub const FONT_MED: fonts::u8g2_font_profont22_mf = fonts::u8g2_font_profont22_mf;
pub const FONT_SMALL: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;
pub const CODE_FONT: fonts::u8g2_font_profont29_mr = fonts::u8g2_font_profont29_mr;

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
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

    /// Get the preferred size of this widget, if it has one
    fn size_hint(&self) -> Option<Size> {
        None
    }
    
    /// Create a new widget that maps this widget's colors to a different color space
    fn color_map<C: PixelColor>(self, map_fn: fn(Self::Color) -> C) -> color_map::ColorMap<Self, C>
    where
        Self: Sized,
    {
        color_map::ColorMap::new(self, map_fn)
    }
    
    /// Force a full redraw of the widget
    /// This is typically used when the widget needs to be redrawn completely,
    /// such as when fading or other visual effects require a complete refresh
    fn force_full_redraw(&mut self) {
        // Default implementation does nothing
        // Widgets that need this functionality should override
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
