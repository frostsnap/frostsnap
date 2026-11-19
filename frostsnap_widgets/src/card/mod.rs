//! Device screens for the 64-word split card: words 1-32 are shown and quizzed here (with
//! the standard backup display and check quiz, in `backup/`), words 33-64 in the app. These are
//! the pages around them: the waits, the device round, the phone round's gate, the verdicts
//! and alarms, and the restore entry of words 1-32.
//!
//! 📐 Every page follows the fitted layout rules for the 240x280 screen: 20 px side
//! padding and a 10 px gap between blocks. Text starts at y = 30 above a hold circle and
//! at y = 46 otherwise, and must end by y = 166 above the hold circle, by y = 222 above
//! the swipe chevron and by y = 270 with no footer. The full-width button sits at
//! y = 236..270. The pages lay their text out by hand so the fit test can check every
//! string against those numbers.
//!
//! 🔐 The device round's answer screen and the restore entry show card words, so they hold
//! them as plain indices and `String`s (accepted for now; design §9). Neither knows the right
//! answer: the answer screen reports only the option index, with the display id of the
//! screen it was made on, and the entry only what was typed. Core resolves them.
mod box_entry;
mod busy;
mod device_round;
mod dots;
mod gate;
mod info_page;
mod restore_entry;
mod restore_review;
mod success;
mod word_entry_model;

pub use box_entry::{BoxEntry, BoxEntryScreen};
pub use busy::CardBusyScreen;
pub use device_round::{DeviceRoundAnswer, DeviceRoundEvent, OPTION_RECTS};
pub use dots::Dots;
pub use gate::GateScreen;
pub use info_page::*;
pub use restore_entry::CardRestoreEntry;
pub use restore_review::CardRestoreReview;
pub use success::CardSuccess;
pub use word_entry_model::{WordEntryModel, WordEntryView};

use crate::{
    fader::Fader, gray4_style::Gray4TextStyle, palette::PALETTE, prelude::*,
    swipe_up_chevron::SwipeUpChevron, Key, KeyTouch, FONT_MED, FONT_SMALL,
};
use alloc::{string::String, vec::Vec};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{renderer::TextRenderer, Baseline},
};
use frostsnap_fonts::{Gray4Font, NOTO_SANS_14_LIGHT};

/// Words the device shows, quizzes and takes at restore: words 1-32, the device half.
pub const CARD_BOXES: usize = frost_backup::verifiable_card::layout::HALF;

pub(crate) const SCREEN_WIDTH: u32 = 240;
pub(crate) const SIDE_PAD: u32 = 20;
/// The widest a line of text may be.
pub(crate) const TEXT_WIDTH: u32 = SCREEN_WIDTH - 2 * SIDE_PAD;
pub(crate) const BLOCK_GAP: u32 = 10;
pub(crate) const TEXT_TOP: u32 = 46;
pub(crate) const TEXT_TOP_HOLD: u32 = 30;
#[cfg(test)]
pub(crate) const TEXT_END_HOLD: u32 = 166;
#[cfg(test)]
pub(crate) const TEXT_END_SWIPE: u32 = 222;
pub(crate) const TEXT_END_NONE: u32 = 270;
/// Text above the full-width button ends here, as on the fitted box screens.
#[cfg(test)]
pub(crate) const TEXT_END_BUTTON: u32 = 231;
/// As wide as the text, so "Can't read this word" fits with its padding.
pub(crate) const BUTTON_SIZE: Size = Size::new(TEXT_WIDTH, 34);
pub(crate) const BUTTON_TOP: i32 = 236;

/// The flash after a pick on the device round's answer screen.
pub const FLASH_MS: u64 = 1200;
/// Undos per word on the device round's answer screen; its second look keeps what is left.
pub const UNDO_CAP_PER_BOX: u8 = 3;
/// Touches are ignored this long after a sub-screen appears, so a finger still down from the
/// last tap can't press the next screen. It is the same after every tap, whatever was tapped.
pub const INPUT_GRACE_MS: u64 = 250;
/// The fade between the device round's questions. Out then in, so it is over by
/// [`INPUT_GRACE_MS`].
pub(crate) const SUB_FADE_OUT_MS: u32 = 100;
pub(crate) const SUB_FADE_IN_MS: u32 = 150;

/// Title (f18), body (f17) and small (f14) fonts.
pub(crate) const FONT_TITLE: &Gray4Font = FONT_MED;
pub(crate) const FONT_BODY: &Gray4Font = FONT_SMALL;
pub(crate) const FONT_SMALL_LINE: &Gray4Font = &NOTO_SANS_14_LIGHT;

/// The full-width button, centred, at y = 236..270.
pub(crate) fn button_rect(screen: Size) -> Rectangle {
    Rectangle::new(
        Point::new(
            (screen.width as i32 - BUTTON_SIZE.width as i32) / 2,
            BUTTON_TOP.min(screen.height as i32 - 44),
        ),
        BUTTON_SIZE,
    )
}

/// The spelling of a word index (11 bits; higher bits are ignored).
///
/// 🔐 Only for words that are drawn on the screen anyway: the device round's options and the
/// restore review's rows.
pub(crate) fn word_spelling(word: u16) -> &'static str {
    frost_backup::bip39_words::BIP39_WORDS[(word & 0x7ff) as usize]
}

pub(crate) fn text_width(text: &str, font: &'static Gray4Font) -> u32 {
    Gray4TextStyle::new(font, PALETTE.on_background)
        .measure_string(text, Point::zero(), Baseline::Top)
        .bounding_box
        .size
        .width
}

/// Break `text` into lines no wider than `max_width`, at spaces. A single word wider than
/// `max_width` gets a line of its own (the fit test catches it).
pub(crate) fn wrap(text: &str, font: &'static Gray4Font, max_width: u32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        if line.is_empty() {
            line.push_str(word);
            continue;
        }
        let candidate_width =
            text_width(&line, font) + text_width(" ", font) + text_width(word, font);
        if candidate_width <= max_width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(core::mem::replace(&mut line, String::from(word)));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// One paragraph of a [`TextBlock`].
#[derive(Clone, Copy)]
pub(crate) struct Para<'a> {
    pub text: &'a str,
    pub font: &'static Gray4Font,
    pub color: Rgb565,
}

impl<'a> Para<'a> {
    pub fn new(text: &'a str, font: &'static Gray4Font, color: Rgb565) -> Self {
        Self { text, font, color }
    }
}

/// Paragraphs wrapped to [`TEXT_WIDTH`] and centred, starting at a fixed `top` and spaced
/// by [`BLOCK_GAP`]. Empty paragraphs are skipped.
pub(crate) struct TextBlock {
    lines: Vec<(Rectangle, Text<Gray4TextStyle>)>,
    bottom: u32,
    size: Size,
}

impl TextBlock {
    /// `width` is the width the lines are centred in (the whole screen, or the area inside
    /// a hold border).
    pub fn new(paras: &[Para<'_>], top: u32, width: u32) -> Self {
        let mut lines = Vec::new();
        let mut y = top;
        let mut first = true;
        for para in paras.iter().filter(|para| !para.text.is_empty()) {
            if !first {
                y += BLOCK_GAP;
            }
            first = false;
            for line in wrap(para.text, para.font, TEXT_WIDTH) {
                let text = Text::new(line, Gray4TextStyle::new(para.font, para.color));
                let line_size: Size = text.sizing().into();
                let x = (width as i32 - line_size.width as i32) / 2;
                lines.push((Rectangle::new(Point::new(x, y as i32), line_size), text));
                y += para.font.line_height;
            }
        }
        Self {
            lines,
            bottom: y,
            size: Size::new(width, y),
        }
    }

    /// Where the last line ends, in this block's coordinates.
    pub fn bottom(&self) -> u32 {
        self.bottom
    }

    #[cfg(test)]
    pub fn widest_line(&self) -> u32 {
        self.lines
            .iter()
            .map(|(rect, _)| rect.size.width)
            .max()
            .unwrap_or(0)
    }
}

impl DynWidget for TextBlock {
    fn set_constraints(&mut self, max_size: Size) {
        self.size = Size::new(
            self.size.width.max(max_size.width),
            max_size.height.max(self.bottom),
        );
    }

    fn sizing(&self) -> crate::Sizing {
        self.size.into()
    }

    fn force_full_redraw(&mut self) {
        for (_, line) in &mut self.lines {
            line.force_full_redraw();
        }
    }
}

impl Widget for TextBlock {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        for (rect, line) in &mut self.lines {
            line.draw(&mut target.clone().crop(*rect), current_time)?;
        }
        Ok(())
    }
}

pub(crate) fn clear_rect<D: DrawTarget<Color = Rgb565>>(
    target: &mut SuperDrawTarget<D, Rgb565>,
    rect: Rectangle,
) -> Result<(), D::Error> {
    rect.into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(PALETTE.background)
            .build(),
    )
    .draw(target)
}

/// Draw `text` once into `rect`, against `background`.
pub(crate) fn draw_text_in<D: DrawTarget<Color = Rgb565>, T: AsRef<str> + Clone>(
    target: &mut SuperDrawTarget<D, Rgb565>,
    text: &mut Text<Gray4TextStyle, T>,
    rect: Rectangle,
    background: Rgb565,
    current_time: Instant,
) -> Result<(), D::Error> {
    text.force_full_redraw();
    text.draw(
        &mut target.clone().crop(rect).with_background_color(background),
        current_time,
    )
}

/// `text` centred in `area`.
pub(crate) fn centred_in(text: &Text<Gray4TextStyle>, area: Rectangle) -> Rectangle {
    let size: Size = text.sizing().into();
    Rectangle::new(
        area.top_left
            + Point::new(
                (area.size.width as i32 - size.width as i32) / 2,
                (area.size.height as i32 - size.height as i32) / 2,
            ),
        size,
    )
}

/// Tap targets that act on release, with the press highlight (design §9.2).
///
/// The device delivers the release that ends a swipe to the drag handler first and then to
/// the touch handler, so every drag must [`Taps::cancel`] the press it started on.
#[derive(Default)]
pub(crate) struct Taps {
    active: Option<KeyTouch>,
    fading: Vec<KeyTouch>,
}

impl Taps {
    pub fn press(&mut self, key: Key, rect: Rectangle) {
        self.cancel();
        self.active = Some(KeyTouch::new(key, rect));
    }

    /// The key pressed, if the finger lifted without a drag.
    pub fn release(&mut self, current_time: Instant) -> Option<Key> {
        let mut touch = self.active.take()?;
        let key = touch.let_go(current_time);
        self.fading.push(touch);
        key
    }

    pub fn cancel(&mut self) {
        if let Some(mut touch) = self.active.take() {
            touch.cancel();
            self.fading.push(touch);
        }
    }

    /// Draw the highlights. `finished` is called for each key whose highlight is gone, so
    /// the caller can redraw what it covered.
    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: Instant,
        mut finished: impl FnMut(Key),
    ) {
        if let Some(active) = &mut self.active {
            active.draw(target, current_time);
        }
        for touch in &mut self.fading {
            touch.draw(target, current_time);
            if touch.is_finished() {
                finished(touch.key);
            }
        }
        self.fading.retain(|touch| !touch.is_finished());
    }
}

/// The swipe-up footer of a single page: the chevron, and the swipe itself.
pub(crate) struct SwipeFooter {
    chevron: Fader<SwipeUpChevron>,
    chevron_rect: Rectangle,
    chevron_started: bool,
    drag_start: Option<u32>,
    swiped: bool,
}

impl SwipeFooter {
    pub fn new() -> Self {
        Self {
            chevron: Fader::new_faded_out(SwipeUpChevron::new(
                PALETTE.on_surface,
                PALETTE.background,
            )),
            chevron_rect: Rectangle::zero(),
            chevron_started: false,
            drag_start: None,
            swiped: false,
        }
    }

    pub fn is_swiped(&self) -> bool {
        self.swiped
    }

    pub fn set_constraints(&mut self, screen: Size) {
        self.chevron.set_constraints(screen);
        let chevron_size: Size = self.chevron.sizing().into();
        self.chevron_rect = Rectangle::new(
            Point::new(
                (screen.width as i32 - chevron_size.width as i32) / 2,
                screen.height as i32 - chevron_size.height as i32,
            ),
            chevron_size,
        );
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if !is_release {
            if self.drag_start.is_none() {
                self.drag_start = prev_y.or(Some(new_y));
            }
            return;
        }
        let Some(start) = self.drag_start.take() else {
            return;
        };
        if self.swiped {
            return;
        }
        if start > new_y {
            self.swiped = true;
            self.chevron.instant_fade();
        }
    }

    pub fn force_full_redraw(&mut self) {
        self.chevron.force_full_redraw();
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        if !self.chevron_started && !self.swiped {
            self.chevron_started = true;
            self.chevron.start_fade_in(400);
        }
        self.chevron
            .draw(&mut target.clone().crop(self.chevron_rect), current_time)
    }
}

/// The full-width outlined button with a centred label, e.g. "Can't read this word".
pub(crate) struct CardButton {
    inner: Container<Center<Text<Gray4TextStyle>>>,
}

impl CardButton {
    pub fn new(label: &str) -> Self {
        let text = Text::new(
            String::from(label),
            Gray4TextStyle::new(FONT_BODY, PALETTE.primary),
        );
        let mut inner = Container::with_size(Center::new(text), BUTTON_SIZE)
            .with_border(PALETTE.outline, 2)
            .with_corner_radius(Size::new(8, 8))
            .with_anti_aliasing(true);
        inner.set_constraints(BUTTON_SIZE);
        Self { inner }
    }

    #[cfg(test)]
    pub fn label_width(&self) -> u32 {
        self.inner.child.child.sizing().width
    }

    pub fn force_full_redraw(&mut self) {
        self.inner.force_full_redraw();
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.inner.draw(target, current_time)
    }
}

#[cfg(test)]
mod test {
    //! The fit check: every card string, laid out with the firmware's own glyph tables,
    //! against the layout rules at the top of this module.
    use super::*;
    use crate::{
        standby::{card_home_line_text, CardHomeLine},
        FONT_HUGE_MONO,
    };
    use alloc::format;
    use frost_backup::bip39_words::{word_to_index, BIP39_WORDS};
    use restore_review::ReviewRowsPage;

    fn limit(footer: Footer) -> u32 {
        match footer {
            Footer::Swipe => TEXT_END_SWIPE,
            Footer::None => TEXT_END_NONE,
            Footer::Hold(_) => TEXT_END_HOLD,
            Footer::Button(_) => TEXT_END_BUTTON,
        }
    }

    struct Fit {
        failures: Vec<String>,
    }

    impl Fit {
        fn check(&mut self, name: &str, bottom: u32, end: u32, widest: u32, max_width: u32) {
            std::println!(
                "{name:<48} ends {bottom:>3} / {end:>3}   widest {widest:>3} / {max_width}"
            );
            if bottom > end {
                self.failures
                    .push(format!("{name}: ends at y = {bottom}, past {end}"));
            }
            if widest > max_width {
                self.failures.push(format!(
                    "{name}: a line is {widest} px wide, over {max_width}"
                ));
            }
        }

        fn width(&mut self, name: &str, widest: u32, max_width: u32) {
            self.check(name, 0, 0, widest, max_width);
        }

        fn page(&mut self, name: &str, page: &CardInfoPage) {
            self.check(
                name,
                page.text_bottom(),
                limit(page.footer()),
                page.widest_line(),
                TEXT_WIDTH,
            );
            if let Some(label) = page.button_label_width() {
                self.width(&format!("{name} (button)"), label, BUTTON_SIZE.width - 16);
            }
        }
    }

    fn fit() -> Fit {
        Fit {
            failures: Vec::new(),
        }
    }

    /// Word masks with the widest lists: one word, two, and three two-digit numbers.
    const MASKS: [WordMask; 4] = [
        1 << 6,
        (1 << 6) | (1 << 39),
        1 << 63,
        (1 << 16) | (1 << 40) | (1 << 63),
    ];

    /// Threshold warnings in their widest forms: a card's threshold that reads and one that
    /// doesn't, against a wallet's and without one (a cold restore).
    const THRESHOLD_CASES: [(&str, Option<u8>, Option<u16>); 3] = [
        ("Threshold differs", Some(255), Some(255)),
        ("Threshold unreadable", None, Some(255)),
        ("Threshold unreadable (no wallet)", None, None),
    ];

    #[test]
    fn threshold_warnings_name_both_thresholds() {
        let texts = |page: &CardInfoPage| page.texts().map(String::from);
        let differs = texts(&threshold_warning_page(Some(3), Some(2)));
        assert_eq!(differs[0], "Threshold differs");
        assert_eq!(differs[1], "This card says 3. The wallet says 2.");
        let unreadable = texts(&threshold_warning_page(None, Some(2)));
        assert_eq!(unreadable[0], "Threshold unreadable");
        assert_eq!(
            unreadable[1],
            "This card's threshold doesn't read. The wallet says 2."
        );
        let cold = texts(&threshold_warning_page(None, None));
        assert_eq!(cold[1], "This card's threshold doesn't read.");

        // consolidation: the same warning, left only by the hold
        let hold = threshold_hold_page(Some(3), 2);
        assert_eq!(hold.footer(), Footer::Hold(Some(HOLD_RESTORE_ANYWAY)));
        assert_eq!(texts(&hold)[1], "This card says 3. The wallet says 2.");
        assert_eq!(texts(&hold)[3], "Hold to restore anyway");

        // the restored key's page labels the threshold as the card's claim
        for (threshold, body) in [
            (Some(3), "Key 2. The card says threshold 3."),
            (Some(0), "Key 2. The card records no threshold."),
            (None, "Key 2. The card's threshold doesn't read."),
        ] {
            assert_eq!(texts(&key_restored_page(2, threshold, false))[1], body);
        }
    }

    fn widest_word() -> u16 {
        BIP39_WORDS
            .iter()
            .enumerate()
            .max_by_key(|(_, word)| text_width(word, FONT_HUGE_MONO))
            .map(|(i, _)| i as u16)
            .unwrap()
    }

    use frost_backup::verifiable_card::layout::WordMask;

    #[test]
    fn every_card_page_fits_the_screen() {
        let mut fit = fit();
        for copy in ALL_CARD_COPY {
            let name = if copy.title.is_empty() {
                copy.body
            } else {
                copy.title
            };
            fit.page(name, &copy.page());
        }
        for j in 32..64 {
            fit.page(
                &format!("Look again at word {}", j + 1),
                &device_round_mismatch_page(j),
            );
        }

        // R4 carries numbers; check the longest forms
        for (name, threshold) in [
            ("Key restored (wide numbers)", Some(255)),
            ("Key restored (no threshold)", Some(0)),
            ("Key restored (threshold unreadable)", None),
        ] {
            fit.page(name, &key_restored_page(255, threshold, true));
        }
        for (name, card, wallet) in THRESHOLD_CASES {
            fit.page(name, &threshold_warning_page(card, wallet));
            if let Some(wallet) = wallet {
                fit.page(
                    &format!("{name} (hold)"),
                    &threshold_hold_page(card, wallet),
                );
            }
        }
        for mask in MASKS {
            fit.page(
                &format!("Didn't fit {mask:#x}"),
                &words_didnt_fit_page(mask),
            );
            fit.page(
                &format!("Unreadable {mask:#x}"),
                &words_unreadable_page(mask),
            );
        }

        for (dots, preparing) in [(5, false), (0, true)] {
            let gate = GateScreen::new(dots, preparing);
            let name = if preparing {
                "Gate (preparing)"
            } else {
                "Gate"
            };
            fit.check(
                name,
                gate.text_bottom(),
                TEXT_END_NONE,
                gate.widest_line(),
                TEXT_WIDTH,
            );
        }

        for (title, status) in [
            BUSY_PREPARING_CARD,
            BUSY_OPENING_CARD,
            BUSY_PROVING,
            BUSY_CHECKING_CARD,
            // the device's busy copy for an opening (core never shows it under Preparing)
            (COPY_GATE_PREPARING.title, COPY_GATE_PREPARING.body),
        ] {
            let busy = CardBusyScreen::new(title, status);
            fit.check(
                &format!("{title} ({status})"),
                busy.content_bottom(),
                TEXT_END_NONE,
                busy.widest_line(),
                TEXT_WIDTH,
            );
        }

        for line in [CardHomeLine::NotChecked, CardHomeLine::Failed] {
            let text = card_home_line_text(line);
            fit.width(text, text_width(text, FONT_SMALL), TEXT_WIDTH);
        }

        assert!(fit.failures.is_empty(), "{:#?}", fit.failures);
    }

    #[test]
    fn the_restore_entry_fits_the_screen() {
        let mut fit = fit();
        // every word number, since digits differ in width
        for number in 1..=CARD_BOXES as u8 {
            let screen = BoxEntryScreen::new(number);
            fit.width(
                &BoxEntryScreen::title(number),
                screen.title_width(),
                TEXT_WIDTH,
            );
            fit.width(
                box_entry::CANT_READ,
                screen.button_label_width(),
                BUTTON_SIZE.width - 16,
            );
        }
        let entries = [Some(widest_word()); CARD_BOXES];
        for page in 0..restore_review::REVIEW_ROW_PAGES {
            let review = ReviewRowsPage::new(page, &entries);
            fit.check(
                &format!("Review page {}", page + 1),
                review.content_bottom(),
                TEXT_END_SWIPE,
                ReviewRowsPage::widest_row_content(),
                TEXT_WIDTH,
            );
        }
        let hold = restore_review::review_hold_page();
        fit.check(
            restore_review::HOLD_TITLE,
            hold.widget().bottom() + 5,
            TEXT_END_HOLD,
            hold.widget().widest_line(),
            TEXT_WIDTH,
        );
        assert!(fit.failures.is_empty(), "{:#?}", fit.failures);
    }

    #[test]
    fn the_device_round_fits_the_screen() {
        let mut fit = fit();
        let widest = widest_word();
        for j in 32..64 {
            let answer = DeviceRoundAnswer::new(j, 4, [widest; 3], 1);
            let (title, word, none, undo_bottom) = answer.fit();
            fit.width(&format!("Select Word {}", j + 1), title, TEXT_WIDTH);
            fit.width("Widest option", word, OPTION_RECTS[0].size.width - 16);
            fit.width("None of these", none, OPTION_RECTS[3].size.width - 16);
            fit.check("Tap to undo (device round)", undo_bottom, 268, 0, 0);
        }
        assert!(fit.failures.is_empty(), "{:#?}", fit.failures);
    }

    /// Every string the card widgets print.
    fn device_strings() -> Vec<String> {
        let mut pages: Vec<CardInfoPage> = ALL_CARD_COPY.iter().map(CardCopy::page).collect();
        for threshold in [Some(2), Some(0), None] {
            pages.push(key_restored_page(1, threshold, false));
        }
        pages.push(key_restored_page(1, Some(2), true));
        for (_, card, wallet) in THRESHOLD_CASES {
            pages.push(threshold_warning_page(card, wallet));
            if let Some(wallet) = wallet {
                pages.push(threshold_hold_page(card, wallet));
            }
        }
        for mask in MASKS {
            pages.push(words_didnt_fit_page(mask));
            pages.push(words_unreadable_page(mask));
        }
        for j in 32..64 {
            pages.push(device_round_mismatch_page(j));
        }
        let mut strings: Vec<String> = pages
            .iter()
            .flat_map(|page| page.texts().map(String::from))
            .collect();
        strings.extend(
            [
                box_entry::CANT_READ,
                device_round::NONE_OF_THESE,
                device_round::UNDO_TEXT,
                restore_review::REVIEW_TITLE,
                restore_review::HOLD_TITLE,
                restore_review::HOLD_BODY,
                BUSY_PREPARING_CARD.0,
                BUSY_PREPARING_CARD.1,
                BUSY_OPENING_CARD.0,
                BUSY_OPENING_CARD.1,
                BUSY_PROVING.0,
                BUSY_PROVING.1,
                BUSY_CHECKING_CARD.0,
                BUSY_CHECKING_CARD.1,
                card_home_line_text(CardHomeLine::NotChecked),
                card_home_line_text(CardHomeLine::Failed),
            ]
            .map(String::from),
        );
        for number in 1..=64u8 {
            strings.push(BoxEntryScreen::title(number));
            strings.push(device_round::answer_title(number - 1));
        }
        strings
    }

    #[test]
    fn device_copy_says_word_and_is_ascii() {
        // the design's rule: every screen says "word", never "box"; and the device fonts have
        // ASCII only, so "1-32" never has an en dash
        for string in device_strings() {
            assert!(string.is_ascii(), "\"{string}\" isn't ASCII");
            assert!(
                !string.to_ascii_lowercase().contains("box"),
                "\"{string}\" says box"
            );
        }
    }

    #[test]
    fn mono_words_are_eight_letters_at_most() {
        // the layouts above size the word column for 8 letters
        assert!(BIP39_WORDS.iter().all(|word| word.len() <= 8));
        assert_eq!(
            text_width("ABSTRACT", FONT_HUGE_MONO),
            text_width("WWWWWWWW", FONT_HUGE_MONO),
            "the word font is monospaced"
        );
        assert!(word_to_index("ABSTRACT").is_some());
    }
}
