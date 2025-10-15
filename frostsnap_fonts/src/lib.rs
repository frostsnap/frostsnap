#![no_std]

// Core font types
pub mod gray4_font;
pub use gray4_font::{GlyphInfo, Gray4Font};

// Font modules
pub mod noto_sans_14_light;
pub mod noto_sans_17_regular;
pub mod noto_sans_18_light;
pub mod noto_sans_18_medium;
pub mod noto_sans_24_bold;
pub mod noto_sans_mono_14_regular;
pub mod noto_sans_mono_15_regular;
pub mod noto_sans_mono_17_regular;
pub mod noto_sans_mono_18_light;
pub mod noto_sans_mono_24_bold;
pub mod noto_sans_mono_28_bold;
pub mod warning_icon;

// Font exports
pub use noto_sans_14_light::NOTO_SANS_14_LIGHT;
pub use noto_sans_17_regular::NOTO_SANS_17_REGULAR;
pub use noto_sans_18_light::NOTO_SANS_18_LIGHT;
pub use noto_sans_18_medium::NOTO_SANS_18_MEDIUM;
pub use noto_sans_24_bold::NOTO_SANS_24_BOLD;
pub use noto_sans_mono_14_regular::NOTO_SANS_MONO_14_REGULAR;
pub use noto_sans_mono_15_regular::NOTO_SANS_MONO_15_REGULAR;
pub use noto_sans_mono_17_regular::NOTO_SANS_MONO_17_REGULAR;
pub use noto_sans_mono_18_light::NOTO_SANS_MONO_18_LIGHT;
pub use noto_sans_mono_24_bold::NOTO_SANS_MONO_24_BOLD;
pub use noto_sans_mono_28_bold::NOTO_SANS_MONO_28_BOLD;
pub use warning_icon::WARNING_ICON;
