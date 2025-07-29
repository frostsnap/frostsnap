// Re-export widgets from frostsnap_embedded_widgets
pub use frostsnap_embedded_widgets::{
    bip39::*, checkmark::*, hold_to_confirm::*, hold_to_confirm_border::*, icons, key_touch::{Key, KeyTouch}, 
    memory_debug::*, sized_box::*, Widget as EmbeddedWidget, FONT_LARGE, FONT_MED, FONT_SMALL
};

// Re-export legacy widgets
pub use frostsnap_embedded_widgets::legacy::*;

// Re-export the Widget trait for convenience
pub use frostsnap_embedded_widgets::Widget;