mod alphabetic_keyboard;
mod backup_display;
mod backup_model;
mod backup_status_bar;
mod enter_share_screen;
mod entered_words;
mod input_preview;
mod numeric_keyboard;
mod t9_keyboard;
mod word_selector;

pub use alphabetic_keyboard::AlphabeticKeyboard;
pub use backup_display::{AllWordsPage, BackupDisplay};
pub use backup_model::{BackupModel, FramebufferMutation, MainViewState, ViewState};
pub use enter_share_screen::EnterShareScreen;
pub use entered_words::EnteredWords;
pub use input_preview::InputPreview;
pub use numeric_keyboard::NumericKeyboard;
pub use t9_keyboard::T9Keyboard;
pub use word_selector::WordSelector;

use u8g2_fonts::fonts as u8g2;

#[allow(unused)]
pub(crate) const LEGACY_FONT_MED: u8g2::u8g2_font_profont22_mf = u8g2::u8g2_font_profont22_mf;
#[allow(unused)]
pub(crate) const LEGACY_FONT_SMALL: u8g2::u8g2_font_profont17_mf = u8g2::u8g2_font_profont17_mf;
