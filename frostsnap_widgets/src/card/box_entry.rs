use super::{
    button_rect, clear_rect, CardButton, WordEntryModel, WordEntryView, FONT_TITLE, SIDE_PAD,
};
use crate::{
    gray4_style::Gray4TextStyle, icons, palette::PALETTE, prelude::*, AlphabeticKeyboard, Key,
    KeyTouch, OneTimeClearHack, WordSelector, FONT_HUGE_MONO,
};
use alloc::{format, string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use frost_backup::bip39_words::ValidLetters;

const TITLE_HEIGHT: u32 = 28;
const TITLE_Y: i32 = 6;
const PREVIEW_HEIGHT: u32 = 34;
const BACKSPACE_WIDTH: u32 = 60;
pub(crate) const CANT_READ: &str = "Can't read this word";

/// What the user entered for a word.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BoxEntry {
    /// The index of the word picked.
    Word(u16),
    /// "Can't read this word".
    CantRead,
}

impl core::fmt::Debug for BoxEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BoxEntry::Word(_) => f.write_str("Word(<redacted>)"),
            BoxEntry::CantRead => f.write_str("CantRead"),
        }
    }
}

/// "Word 13": a one-row preview with backspace, the standard alphabetic keyboard (the word
/// selector once 8 or fewer words match) and the "Can't read this word" button.
///
/// It only reports what was entered; it never knows the right word. Between words
/// [`BoxEntryScreen::reset`] reuses the keyboard rather than building a second one.
pub struct BoxEntryScreen {
    box_number: u8,
    model: WordEntryModel,
    title: Text<Gray4TextStyle>,
    title_dirty: bool,
    preview: Option<Text<Gray4TextStyle>>,
    preview_dirty: bool,
    keyboard: AlphabeticKeyboard,
    word_selector: Option<OneTimeClearHack<WordSelector>>,
    button: CardButton,
    touches: Vec<KeyTouch>,
    pending_update: bool,
    result: Option<BoxEntry>,
    title_rect: Rectangle,
    preview_rect: Rectangle,
    backspace_rect: Rectangle,
    keyboard_rect: Rectangle,
    button_rect: Rectangle,
    size: Size,
}

impl BoxEntryScreen {
    /// Word `box_number` (1-based).
    pub fn new(box_number: u8) -> Self {
        let mut screen = Self {
            box_number,
            model: WordEntryModel::new(),
            title: Self::title_text(box_number),
            title_dirty: true,
            preview: None,
            preview_dirty: true,
            keyboard: AlphabeticKeyboard::new(),
            word_selector: None,
            button: CardButton::new(CANT_READ),
            touches: Vec::new(),
            pending_update: false,
            result: None,
            title_rect: Rectangle::zero(),
            preview_rect: Rectangle::zero(),
            backspace_rect: Rectangle::zero(),
            keyboard_rect: Rectangle::zero(),
            button_rect: Rectangle::zero(),
            size: Size::zero(),
        };
        screen.preview = screen.preview_text();
        screen
    }

    pub(crate) fn title(box_number: u8) -> String {
        format!("Word {box_number}")
    }

    fn title_text(box_number: u8) -> Text<Gray4TextStyle> {
        Text::new(
            Self::title(box_number),
            Gray4TextStyle::new(FONT_TITLE, PALETTE.on_background),
        )
    }

    fn preview_text(&self) -> Option<Text<Gray4TextStyle>> {
        (!self.model.is_empty()).then(|| {
            Text::new(
                String::from(self.model.current()),
                Gray4TextStyle::new(FONT_HUGE_MONO, PALETTE.on_background),
            )
        })
    }

    /// Ask for another word, keeping the keyboard.
    pub fn reset(&mut self, box_number: u8) {
        self.box_number = box_number;
        self.model.clear();
        self.result = None;
        self.pending_update = false;
        for touch in &mut self.touches {
            touch.cancel();
        }
        if self.word_selector.take().is_some() {
            self.keyboard.force_full_redraw();
        }
        self.keyboard.set_valid_keys(ValidLetters::default());
        self.keyboard.reset_scroll();
        self.title = Self::title_text(box_number);
        self.title_dirty = true;
        self.preview = self.preview_text();
        self.preview_dirty = true;
    }

    pub fn box_number(&self) -> u8 {
        self.box_number
    }

    /// What the user entered, once they picked a word or tapped "Can't read this word".
    pub fn result(&self) -> Option<BoxEntry> {
        self.result
    }

    /// No key highlight is still fading, so the screen can be reset without a stray mark.
    pub fn is_settled(&self) -> bool {
        self.touches.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn title_width(&self) -> u32 {
        self.title.sizing().width
    }

    #[cfg(test)]
    pub(crate) fn button_label_width(&self) -> u32 {
        self.button.label_width()
    }

    fn entry_changed(&mut self) {
        self.preview = self.preview_text();
        self.preview_dirty = true;
        self.pending_update = true;
    }

    fn cancel_all_touches(&mut self) {
        for touch in &mut self.touches {
            touch.cancel();
        }
    }

    /// Swap between keyboard and word selector once the key highlight has faded.
    fn update_view(&mut self) {
        match self.model.view() {
            WordEntryView::Keyboard { valid_letters } => {
                if self.word_selector.take().is_some() {
                    self.keyboard.force_full_redraw();
                }
                self.keyboard.set_valid_keys(valid_letters);
            }
            WordEntryView::Select { possible_words } => {
                if self.word_selector.is_none() {
                    let mut selector = OneTimeClearHack::new(WordSelector::new(
                        possible_words,
                        self.model.current(),
                    ));
                    selector.set_constraints(self.keyboard_rect.size);
                    self.word_selector = Some(selector);
                    self.cancel_all_touches();
                }
            }
        }
    }
}

impl DynWidget for BoxEntryScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.size = max_size;
        self.title_rect = Rectangle::new(Point::zero(), Size::new(max_size.width, TITLE_HEIGHT));
        self.preview_rect = Rectangle::new(
            Point::new(0, TITLE_HEIGHT as i32),
            Size::new(max_size.width, PREVIEW_HEIGHT),
        );
        self.backspace_rect = Rectangle::new(
            Point::new(
                (max_size.width - BACKSPACE_WIDTH) as i32,
                TITLE_HEIGHT as i32,
            ),
            Size::new(BACKSPACE_WIDTH, PREVIEW_HEIGHT),
        );
        self.button_rect = button_rect(max_size);
        let keyboard_top = TITLE_HEIGHT + PREVIEW_HEIGHT;
        let keyboard_bottom = (self.button_rect.top_left.y - 2) as u32;
        self.keyboard_rect = Rectangle::new(
            Point::new(0, keyboard_top as i32),
            Size::new(max_size.width, keyboard_bottom - keyboard_top),
        );
        self.keyboard.set_constraints(self.keyboard_rect.size);
        if let Some(selector) = &mut self.word_selector {
            selector.set_constraints(self.keyboard_rect.size);
        }
    }

    fn sizing(&self) -> crate::Sizing {
        self.size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        lift_up: bool,
    ) -> Option<KeyTouch> {
        if lift_up {
            let blocked = self.pending_update || self.result.is_some();
            let active = self
                .touches
                .iter_mut()
                .rev()
                .find(|t| !t.has_been_let_go())?;
            if blocked {
                active.let_go(current_time);
                return None;
            }
            match active.let_go(current_time) {
                Some(Key::Keyboard('⌫')) => {
                    self.model.backspace();
                    self.entry_changed();
                }
                Some(Key::Keyboard(c)) if c.is_ascii_alphabetic() => {
                    self.model.add_character(c);
                    self.entry_changed();
                }
                Some(Key::WordSelector(word)) => {
                    if let Some(index) = self.model.select(word) {
                        self.result = Some(BoxEntry::Word(index));
                        self.preview = self.preview_text();
                        self.preview_dirty = true;
                    }
                }
                Some(Key::CantRead) => self.result = Some(BoxEntry::CantRead),
                _ => {}
            }
            return None;
        }

        // Ignore new touches while a key highlight fades before the keyboard redraws
        if self.pending_update || self.result.is_some() {
            return None;
        }

        let key_touch = if self.button_rect.contains(point) {
            Some(KeyTouch::new(Key::CantRead, self.button_rect))
        } else if self.backspace_rect.contains(point) {
            (!self.model.is_empty()).then(|| KeyTouch::new(Key::Keyboard('⌫'), self.backspace_rect))
        } else if self.keyboard_rect.contains(point) {
            let offset = self.keyboard_rect.top_left;
            if let Some(selector) = &mut self.word_selector {
                selector
                    .handle_touch(point - offset, current_time, false)
                    .map(|mut key_touch| {
                        key_touch.translate(offset);
                        key_touch
                    })
            } else {
                self.keyboard
                    .handle_touch(point - offset, current_time, false)
                    .map(|mut key_touch| {
                        key_touch.translate(offset);
                        key_touch.clamp_to(self.keyboard_rect);
                        key_touch
                    })
            }
        } else {
            None
        };

        if let Some(key_touch) = key_touch {
            if let Some(last) = self.touches.last_mut() {
                if last.key == key_touch.key {
                    self.touches.pop();
                } else {
                    last.cancel();
                }
            }
            self.touches.push(key_touch);
        }
        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if !self.pending_update {
            if let Some(active) = self.touches.last_mut() {
                active.cancel();
            }
        }
        if self.word_selector.is_none() {
            self.keyboard
                .handle_vertical_drag(prev_y, new_y, is_release);
        }
    }

    fn force_full_redraw(&mut self) {
        self.title_dirty = true;
        self.preview_dirty = true;
        self.keyboard.force_full_redraw();
        self.word_selector.force_full_redraw();
        self.button.force_full_redraw();
    }
}

impl Widget for BoxEntryScreen {
    type Color = Rgb565;

    // Keep this screen's temporaries out of the caller's frame.
    #[inline(never)]
    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut button_touched = false;
        for touch in &mut self.touches {
            touch.draw(target, current_time);
            button_touched |= touch.key == Key::CantRead && touch.is_finished();
        }
        self.touches.retain(|touch| !touch.is_finished());
        if button_touched {
            self.button.force_full_redraw();
        }

        if self.pending_update && self.touches.is_empty() {
            self.pending_update = false;
            self.update_view();
        }

        if self.title_dirty {
            self.title_dirty = false;
            clear_rect(target, self.title_rect)?;
            self.title.force_full_redraw();
            let size: Size = self.title.sizing().into();
            let rect = Rectangle::new(
                Point::new((self.size.width as i32 - size.width as i32) / 2, TITLE_Y),
                size,
            );
            self.title
                .draw(&mut target.clone().crop(rect), current_time)?;
        }

        if self.preview_dirty {
            self.preview_dirty = false;
            clear_rect(target, self.preview_rect)?;
            if let Some(preview) = &mut self.preview {
                let size: Size = preview.sizing().into();
                let x = SIDE_PAD as i32;
                let y =
                    self.preview_rect.top_left.y + (PREVIEW_HEIGHT as i32 - size.height as i32) / 2;
                preview.force_full_redraw();
                preview.draw(
                    &mut target.clone().crop(Rectangle::new(Point::new(x, y), size)),
                    current_time,
                )?;
            }
            if !self.model.is_empty() {
                icons::backspace()
                    .with_color(PALETTE.error)
                    .with_center(self.backspace_rect.center())
                    .draw(target);
            }
        }

        let mut keyboard_target = target.clone().crop(self.keyboard_rect);
        if let Some(selector) = &mut self.word_selector {
            selector.draw(&mut keyboard_target, current_time)?;
        } else {
            self.keyboard.draw(&mut keyboard_target, current_time)?;
        }

        self.button
            .draw(&mut target.clone().crop(self.button_rect), current_time)
    }
}
