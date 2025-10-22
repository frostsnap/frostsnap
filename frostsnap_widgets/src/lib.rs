#![no_std]
#![allow(clippy::type_complexity)]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[macro_use]
pub extern crate alloc;

use embedded_graphics::prelude::*;

pub mod compressed_point;
pub mod palette;
pub mod pixel_recorder;

// Widget modules
pub mod address_display;
pub mod animation_speed;
pub mod backup;
pub mod bitmap;
pub mod checkmark;
pub mod cursor;
pub mod debug;
pub mod fader;
pub mod hold_to_confirm;
pub mod hold_to_confirm_border;
pub mod icons;
pub mod image;
pub mod key_touch;
pub mod one_time_clear_hack;
pub mod rat;
// pub mod page_by_page;
// pub mod page_demo;
pub mod page_slider;
pub mod progress;
pub mod progress_bars;
pub mod widget_list;
// pub mod buffered;
pub mod any_of;
pub mod bitcoin_amount_display;
pub mod bobbing_carat;
pub mod circle_button;
pub mod demo_widget;
pub mod device_name;
pub mod erase_progress;
pub mod fade_switcher;
pub mod firmware_upgrade;
pub mod fps;
pub mod gray4_style;
pub mod keygen_check;
pub mod layout;
pub mod p2pkh_address_display;
pub mod p2sh_address_display;
pub mod p2tr_address_display;
pub mod p2wpkh_address_display;
pub mod p2wsh_address_display;
pub mod prelude;
pub mod screen_test;
pub mod scroll_bar;
pub mod share_index;
pub mod sign_message;
pub mod sign_prompt;
pub mod slide_in_transition;
pub mod standby;
pub mod string_ext;
mod super_draw_target;
pub mod swipe_up_chevron;
pub mod switcher;
pub mod text;
pub mod touch_listener;
pub mod translate;
pub mod vec_framebuffer;
pub mod widget_color;
pub mod wipe_device;

// Re-export key types
pub use key_touch::{Key, KeyTouch};
// pub use page_by_page::PageByPage;
// pub use page_demo::PageDemo;
pub use page_slider::PageSlider;
pub use share_index::ShareIndexWidget;
pub use sign_message::SignMessageConfirm;
pub use sign_prompt::SignTxPrompt;
pub use super_draw_target::SuperDrawTarget;
pub use widget_color::{ColorInterpolate, WidgetColor};
pub use widget_list::*;

// Re-export tinybmp for use in demos and public API
pub use tinybmp;

// Re-export all widget items
pub use address_display::{AddressDisplay, AddressWithPath};
pub use backup::*;
pub use checkmark::*;
pub use cursor::*;
pub use fader::*;
pub use hold_to_confirm::HoldToConfirm;
pub use hold_to_confirm_border::HoldToConfirmBorder;
pub use image::{GrayToAlpha, Image};
pub use layout::{
    Align, Alignment, Center, Column, Container, CrossAxisAlignment, MainAxisAlignment,
    MainAxisSize, Padding, Row, SizedBox, Stack,
};
pub use one_time_clear_hack::OneTimeClearHack;
pub use rat::{Frac, Rat};
// pub use hold_to_confirm_button::*;
pub use bobbing_carat::BobbingCarat;
pub use circle_button::*;
pub use device_name::*;
pub use erase_progress::EraseProgress;
pub use fade_switcher::FadeSwitcher;
pub use firmware_upgrade::{FirmwareUpgradeConfirm, FirmwareUpgradeProgress};
pub use fps::Fps;
pub use icons::IconWidget;
pub use keygen_check::*;
pub use progress::{ProgressBar, ProgressIndicator};
pub use progress_bars::*;
pub use screen_test::ScreenTest;
pub use scroll_bar::*;
pub use slide_in_transition::*;
pub use standby::*;
pub use swipe_up_chevron::*;
pub use switcher::Switcher;
pub use text::*;
pub use touch_listener::TouchListener;
pub use translate::*;
pub use wipe_device::*;

// Font re-exports
use frostsnap_fonts::{
    Gray4Font, NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_24_BOLD, NOTO_SANS_MONO_28_BOLD,
};
use gray4_style::Gray4TextStyle;
pub const FONT_HUGE_MONO: &Gray4Font = &NOTO_SANS_MONO_28_BOLD;
pub const FONT_LARGE: &Gray4Font = &NOTO_SANS_24_BOLD;
pub const FONT_MED: &Gray4Font = &NOTO_SANS_18_MEDIUM;
pub const FONT_SMALL: &Gray4Font = &NOTO_SANS_17_REGULAR;

// Type alias for default text styling
pub type DefaultTextStyle = Gray4TextStyle;

// U8g2 fonts for keyboards (monochrome framebuffers)
use u8g2_fonts::U8g2TextStyle;

pub const HOLD_TO_CONFIRM_TIME_SHORT_MS: u32 = 1000;
pub const HOLD_TO_CONFIRM_TIME_MS: u32 = 2000;
pub const HOLD_TO_CONFIRM_TIME_LONG_MS: u32 = 6000;

/// Sizing information for a widget
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sizing {
    pub width: u32,
    pub height: u32,
    /// The actual area within the sized area where this widget will draw.
    /// If None, assumes the widget may draw to the entire sized area.
    /// Widgets like Column, Row, and Padding should set this to the actual area
    /// they give their children to draw in.
    pub dirty_rect: Option<embedded_graphics::primitives::Rectangle>,
}

impl From<Size> for Sizing {
    fn from(size: Size) -> Self {
        Sizing {
            width: size.width,
            height: size.height,
            dirty_rect: None,
        }
    }
}

impl Sizing {
    /// Returns the dirty rectangle, or constructs one from width/height if not set
    pub fn dirty_rect(&self) -> embedded_graphics::primitives::Rectangle {
        self.dirty_rect.unwrap_or_else(|| {
            embedded_graphics::primitives::Rectangle::new(
                embedded_graphics::prelude::Point::zero(),
                Size::new(self.width, self.height),
            )
        })
    }
}

impl From<Sizing> for Size {
    fn from(sizing: Sizing) -> Self {
        Size::new(sizing.width, sizing.height)
    }
}

/// A trait for widgets that can be used as trait objects
/// This contains all the non-generic methods from Widget
pub trait DynWidget {
    /// Set maximum available size for this widget. Parent calls this before asking for size.
    /// This must be called before sizing().
    fn set_constraints(&mut self, max_size: Size);

    /// Get sizing information for this widget given its constraints.
    /// Must be called after set_constraints.
    fn sizing(&self) -> Sizing;

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

    /// Force a full redraw of the widget
    /// This is typically used when the widget needs to be redrawn completely,
    /// such as when fading or other visual effects require a complete refresh
    fn force_full_redraw(&mut self) {
        // Default implementation does nothing
        // Widgets that need this functionality should override
    }
}

/// A trait that combines DynWidget with Any for downcasting
pub trait AnyDynWidget: core::any::Any + DynWidget {}

/// Blanket implementation for any type that implements both Any and DynWidget
impl<T: core::any::Any + DynWidget> AnyDynWidget for T {}

/// A trait for drawable widgets that can handle user interactions
pub trait Widget: DynWidget {
    /// The color type this widget natively draws in
    type Color: WidgetColor;

    /// Draw the widget to the target
    fn draw<D>(
        &mut self,
        target: &mut super_draw_target::SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>;
}

// Implement Widget for () to allow empty containers
impl Widget for () {
    type Color = embedded_graphics::pixelcolor::Rgb565;

    fn draw<D>(
        &mut self,
        _target: &mut super_draw_target::SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Empty widget draws nothing
        Ok(())
    }
}

// Implement Widget for Box<T> where T: Widget
impl<T: Widget + ?Sized> Widget for alloc::boxed::Box<T> {
    type Color = T::Color;

    fn draw<D>(
        &mut self,
        target: &mut super_draw_target::SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        (**self).draw(target, current_time)
    }
}

// Implement DynWidget for Box<T> where T: DynWidget
impl<T: DynWidget + ?Sized> DynWidget for alloc::boxed::Box<T> {
    fn set_constraints(&mut self, max_size: Size) {
        (**self).set_constraints(max_size)
    }

    fn sizing(&self) -> Sizing {
        (**self).sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        (**self).handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        (**self).handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn force_full_redraw(&mut self) {
        (**self).force_full_redraw()
    }
}

// Implement DynWidget for Option<T> where T: DynWidget
impl<T: DynWidget> DynWidget for Option<T> {
    fn set_constraints(&mut self, max_size: Size) {
        if let Some(widget) = self {
            widget.set_constraints(max_size)
        }
    }

    fn sizing(&self) -> Sizing {
        if let Some(widget) = self {
            widget.sizing()
        } else {
            Size::new(0, 0).into()
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        if let Some(widget) = self {
            widget.handle_touch(point, current_time, is_release)
        } else {
            None
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if let Some(widget) = self {
            widget.handle_vertical_drag(prev_y, new_y, is_release)
        }
    }

    fn force_full_redraw(&mut self) {
        if let Some(widget) = self {
            widget.force_full_redraw()
        }
    }
}

// Implement Widget for Option<W> where W: Widget
impl<W: Widget> Widget for Option<W> {
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut super_draw_target::SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if let Some(widget) = self {
            widget.draw(target, current_time)
        } else {
            Ok(())
        }
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
