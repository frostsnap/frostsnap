#![no_std]

extern crate alloc;

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
pub mod fraction;
pub mod hold_to_confirm;
pub mod hold_to_confirm_border;
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
pub mod welcome;

// Re-export key types
pub use key_touch::{Key, KeyTouch};
pub use page_by_page::PageByPage;
pub use page_demo::PageDemo;

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
pub use fraction::Fraction;
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
pub use welcome::*;

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

/// Macro for selecting and running demo widgets
#[macro_export]
macro_rules! select_widget {
    ($demo:expr, $screen_size:expr, $run_macro:ident) => {
        match $demo.as_ref() {
            "bip39_entry" => {
                let widget = $crate::bip39::EnterBip39ShareScreen::new($screen_size);
                $run_macro!(widget);
            }
            "bip39_t9" => {
                let widget = $crate::bip39::EnterBip39T9Screen::new($screen_size);
                $run_macro!(widget);
            }
            "confirm_touch" | "hold_confirm" | "hold_checkmark" => {
                use $crate::{text::Text, HoldToConfirm, palette::PALETTE};
                use embedded_graphics::pixelcolor::BinaryColor;
                
                let prompt_text = Text::new("Confirm\ntransaction");
                let prompt_widget = prompt_text.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_surface,
                    BinaryColor::Off => PALETTE.background,
                });
                
                let success_text = Text::new("Transaction\nsigned");
                let success_widget = success_text.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_surface,
                    BinaryColor::Off => PALETTE.background,
                });
                
                let widget = HoldToConfirm::new($screen_size, 2000.0, prompt_widget, success_widget);
                $run_macro!(widget);
            }
            "welcome" => {
                use $crate::welcome::Welcome;
                let widget = Welcome::new();
                $run_macro!(widget);
            }
            "vertical_slide" => {
                use $crate::{PageDemo, VerticalPaginator, palette::PALETTE};
                use embedded_graphics::{prelude::*, framebuffer::buffer_size};
                
                let page_demo = PageDemo::new($screen_size);
                const SCREEN_WIDTH: usize = 240;
                const SCREEN_HEIGHT: usize = 280;
                const BUFFER_SIZE: usize = buffer_size::<<PageDemo as Widget>::Color>(SCREEN_WIDTH, SCREEN_HEIGHT);
                let paginator = VerticalPaginator::<_, SCREEN_WIDTH, SCREEN_HEIGHT, BUFFER_SIZE>::new(page_demo);
                
                let widget = paginator.color_map(|c| match c.luma() {
                    0b00 => PALETTE.background,
                    0b01 => PALETTE.outline,
                    0b10 => PALETTE.primary,
                    0b11|_ => PALETTE.on_background
                });
                
                $run_macro!(widget);
            }
            "bip39_backup" => {
                use $crate::{bip39::Bip39BackupDisplay, VerticalPaginator, PaginatorWithScrollBar, palette::PALETTE};
                use embedded_graphics::{prelude::*, framebuffer::buffer_size};
                use embedded_text::alignment::HorizontalAlignment;
                
                // Generate test word indices - same words as original display
                const TEST_WORD_INDICES: [u16; 25] = [
                    1337, // owner
                    432,  // deny
                    1789, // survey
                    923,  // journey
                    567,  // embark
                    1456, // recall
                    234,  // churn
                    1678, // spawn
                    890,  // invest
                    345,  // crater
                    1234, // neutral
                    678,  // fiscal
                    1890, // thumb
                    456,  // diamond
                    1567, // robot
                    789,  // guitar
                    1345, // oyster
                    123,  // badge
                    1789, // survey
                    567,  // embark
                    1012, // lizard
                    1456, // recall
                    789,  // guitar
                    1678, // spawn
                    234,  // churn
                ];
                let share_index = 42;
                
                let backup_display = Bip39BackupDisplay::new($screen_size, TEST_WORD_INDICES, share_index);
                const SCREEN_WIDTH: usize = 240;
                const SCREEN_HEIGHT: usize = 280; // Full screen height
                const BUFFER_SIZE: usize = buffer_size::<<Bip39BackupDisplay as Widget>::Color>(SCREEN_WIDTH, SCREEN_HEIGHT);
                let paginator = VerticalPaginator::<_, SCREEN_WIDTH, SCREEN_HEIGHT, BUFFER_SIZE>::new(backup_display);
                
                let paginator_mapped = paginator.color_map(|c| match c.luma() {
                    0b00 => PALETTE.background,           // Black background
                    0b01 => PALETTE.on_surface_variant,   // Gray for secondary text
                    0b10 => PALETTE.outline,              // Not used currently
                    0b11 => PALETTE.primary,              // Cyan/blue for primary text
                    _ => PALETTE.on_background
                });
                
                // Create HoldToConfirm widget for final page
                use $crate::{HoldToConfirm, text::Text};
                use embedded_graphics::pixelcolor::BinaryColor;
                
                let confirm_prompt = Text::new("I have written down:\n\n- the key index\n- all 25 words");
                let confirm_prompt_rgb = confirm_prompt.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_surface,
                    BinaryColor::Off => PALETTE.background,
                });
                
                let mut success_text = Text::new("Keep it secret\nKeep it safe").with_horizontal_alignment(HorizontalAlignment::Center);
                let success_text_rgb = success_text.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_surface,
                    BinaryColor::Off => PALETTE.background,
                });
                
                let hold_to_confirm = HoldToConfirm::new($screen_size, 2000.0, confirm_prompt_rgb, success_text_rgb);
                
                let widget = PaginatorWithScrollBar::new(paginator_mapped, hold_to_confirm, $screen_size);
                
                $run_macro!(widget);
            }
            "fade_in_fade_out" => {
                use $crate::{fader::Fader, text::Text, palette::PALETTE};
                use embedded_graphics::pixelcolor::BinaryColor;
                
                // Simple text widget that will fade in/out
                let text = Text::new("Fade Demo");
                let text_colored = text.color_map(|c| match c {
                    BinaryColor::On => PALETTE.on_background,
                    BinaryColor::Off => PALETTE.background,
                });
                
                // Create a fader starting faded out
                let mut fader = Fader::new_faded_out(text_colored);
                // Start the fade-in immediately
                fader.start_fade_in(1000, 50, PALETTE.background);
                
                $run_macro!(fader);
            }
            _ => {
                panic!("Unknown demo: '{}'. Valid demos: bip39_entry, bip39_t9, hold_confirm, checkmark, welcome, vertical_slide, bip39_backup, fade_in_fade_out", $demo);
            }
        }
    };
}
