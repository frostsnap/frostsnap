use crate::gray4_style::Gray4TextStyle;
use crate::DefaultTextStyle;
use crate::HOLD_TO_CONFIRM_TIME_MS;
use frostsnap_fonts::{NOTO_SANS_14_LIGHT, NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM};

const FONT_CONFIRM_TITLE: &frostsnap_fonts::Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CONFIRM_TEXT: &frostsnap_fonts::Gray4Font = &NOTO_SANS_17_REGULAR;
use crate::{
    page_slider::PageSlider,
    palette::PALETTE,
    prelude::*,
    share_index::ShareIndexWidget,
    widget_list::{WidgetList, WidgetListItem},
    FadeSwitcher, HoldToConfirm, FONT_HUGE_MONO, FONT_MED,
};
use alloc::{format, string::String, string::ToString, vec::Vec};
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565, prelude::*, text::Alignment};
use frost_backup::{bip39_words::BIP39_WORDS, NUM_WORDS};

const FONT_ALL_WORDS: &frostsnap_fonts::Gray4Font = &NOTO_SANS_14_LIGHT;
const WORDS_PER_PAGE: usize = 3;
/// Words in a half of the 64-word split card.
pub const CARD_HALF_WORDS: usize = frost_backup::verifiable_card::layout::HALF;

/// Which backup the pages show, for the copy around the words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupCopy {
    /// The standard 25-word backup and its key number.
    Master,
    /// Words 1-32 of a split card; the phone shows words 33-64.
    CardHalf,
}

/// A single page showing the share index
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexPage {
    #[widget_delegate]
    center: Center<Column<(Text, ShareIndexWidget)>>,
}

impl ShareIndexPage {
    fn new(share_index: u16) -> Self {
        let label = Text::new(
            "Key number",
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let share_index_widget = ShareIndexWidget::new(share_index, FONT_HUGE_MONO);

        let column = Column::builder()
            .push(label)
            .gap(8)
            .push(share_index_widget)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let center = Center::new(column);

        Self { center }
    }
}

/// A split card's first page, where master shows the key number: which words this screen
/// shows.
#[derive(frostsnap_macros::Widget)]
pub struct CardHalfLeadPage {
    #[widget_delegate]
    center: Center<Column<(Text, Text, Text)>>,
}

impl CardHalfLeadPage {
    pub(crate) const LINES: [&'static str; 3] = ["Your card's words", "1-32", "of 64"];

    fn new() -> Self {
        let [label, words, of] = Self::LINES;
        let label = Text::new(
            label,
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );
        let words = Text::new(
            words,
            DefaultTextStyle::new(FONT_HUGE_MONO, PALETTE.primary),
        );
        let of = Text::new(of, DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary));
        let column = Column::builder()
            .push(label)
            .gap(8)
            .push(words)
            .gap(8)
            .push(of)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        Self {
            center: Center::new(column),
        }
    }
}

/// A row showing a word number and the word itself
#[derive(frostsnap_macros::Widget)]
pub struct WordRow {
    #[widget_delegate]
    row: Row<(Text, Text)>,
}

impl WordRow {
    fn new(word_number: usize, word: &str) -> Self {
        let number_text = Text::new(
            format!("{}.", word_number),
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Left);

        let word_text = Text::new(
            String::from(word),
            DefaultTextStyle::new(FONT_HUGE_MONO, PALETTE.primary),
        )
        .with_alignment(Alignment::Left);

        let row = Row::builder().push(number_text).gap(10).push(word_text);

        Self { row }
    }
}

/// Enum for different word page layouts
type WordsPageLayout = Center<
    crate::any_of::AnyOf<(
        Column<(WordRow,)>,
        Column<(WordRow, WordRow)>,
        Column<(WordRow, WordRow, WordRow)>,
    )>,
>;

/// A page showing up to 3 words
#[derive(frostsnap_macros::Widget)]
pub struct WordsPage {
    #[widget_delegate]
    layout: WordsPageLayout,
}

impl WordsPage {
    fn new(words: Vec<(usize, String)>) -> Self {
        // Build the layout based on how many words we have
        let layout = match words.len() {
            1 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::new((row1,)).with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
            2 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::builder()
                        .push(row1)
                        .gap(20)
                        .push(row2)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
            3 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                let row3 = WordRow::new(words[2].0, &words[2].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::builder()
                        .push(row1)
                        .gap(20)
                        .push(row2)
                        .gap(20)
                        .push(row3)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
            _ => {
                // Should never happen but handle gracefully
                let row1 = WordRow::new(1, "error");
                Center::new(crate::any_of::AnyOf::new(
                    Column::new((row1,)).with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
        };

        Self { layout }
    }
}

type SingleWordRow = Row<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>;

/// A page showing all the words in two columns
#[derive(Clone, frostsnap_macros::Widget)]
pub struct AllWordsPage {
    #[widget_delegate]
    content: Row<(Column<Vec<SingleWordRow>>, Column<Vec<SingleWordRow>>)>,
}

impl AllWordsPage {
    pub fn new(word_indices: &[u16; 25], share_index: u16) -> Self {
        let make_word_row = |word_idx: usize| -> SingleWordRow {
            Self::word_row(word_idx + 1, word_indices[word_idx])
        };

        let left_column = {
            let share_row = Row::new((
                Text::new(
                    " #.",
                    Gray4TextStyle::new(FONT_ALL_WORDS, PALETTE.text_secondary),
                ),
                Text::new(
                    format!("{}", share_index),
                    Gray4TextStyle::new(FONT_ALL_WORDS, PALETTE.primary),
                )
                .with_underline(PALETTE.surface),
            ))
            .with_main_axis_alignment(MainAxisAlignment::Start);

            let mut rows = Vec::with_capacity(13);
            rows.push(share_row);
            for i in 0..12 {
                rows.push(make_word_row(i));
            }

            Column::new(rows)
                .with_main_axis_alignment(MainAxisAlignment::Center)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
        };

        let mut right_rows = Vec::with_capacity(13);
        for i in 12..25 {
            right_rows.push(make_word_row(i));
        }

        Self::columns(left_column, right_rows)
    }

    /// Words 1-32 of a split card: 1-16 | 17-32, with no key number row.
    pub fn card_half(word_indices: &[u16; CARD_HALF_WORDS]) -> Self {
        let half = CARD_HALF_WORDS / 2;
        let rows = |range: core::ops::Range<usize>| -> Vec<SingleWordRow> {
            range
                .map(|i| Self::word_row(i + 1, word_indices[i]))
                .collect()
        };
        // 📐 16 rows from the top, so the last ones end above the swipe chevron
        let column = |rows: Vec<SingleWordRow>| {
            Column::new(rows)
                .with_main_axis_alignment(MainAxisAlignment::Start)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
        };
        let two_columns = Row::new((column(rows(0..half)), column(rows(half..CARD_HALF_WORDS))))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        Self {
            content: two_columns,
        }
    }

    fn word_row(word_number: usize, word: u16) -> SingleWordRow {
        Row::new((
            Text::new(
                format!("{:2}.", word_number),
                Gray4TextStyle::new(FONT_ALL_WORDS, PALETTE.text_secondary),
            ),
            Text::new(
                format!("{:<8}", BIP39_WORDS[word as usize]),
                Gray4TextStyle::new(FONT_ALL_WORDS, PALETTE.primary),
            ),
        ))
        .with_main_axis_alignment(MainAxisAlignment::Start)
    }

    fn columns(left_column: Column<Vec<SingleWordRow>>, right_rows: Vec<SingleWordRow>) -> Self {
        let right_column = Column::new(right_rows)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Start);

        let two_columns = Row::new((left_column, right_column))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        Self {
            content: two_columns,
        }
    }
}

/// Type alias for the content that can be shown in the confirmation screen
type ConfirmationContent = crate::any_of::AnyOf<(ConfirmContent, SafetyReminder)>;

/// A confirmation screen that shows after backup and fades to a security reminder
pub struct BackupConfirmationScreen {
    hold_confirm: HoldToConfirm<FadeSwitcher<Center<ConfirmationContent>>>,
    fade_triggered: bool,
    copy: BackupCopy,
}

/// The initial confirmation content
#[derive(frostsnap_macros::Widget)]
pub struct ConfirmContent {
    #[widget_delegate]
    column: Column<(
        Column<(
            Text<Gray4TextStyle>,
            Text<Gray4TextStyle>,
            Text<Gray4TextStyle>,
        )>,
        Text<Gray4TextStyle>,
    )>,
}

/// The completion message that fades in after confirmation
#[derive(frostsnap_macros::Widget)]
pub struct SafetyReminder {
    #[widget_delegate]
    content: Column<(
        Text<Gray4TextStyle>,
        Text<Gray4TextStyle>,
        Text<Gray4TextStyle>,
    )>,
}

impl ConfirmContent {
    fn new(copy: BackupCopy) -> Self {
        let [line1, line2, line3] = Self::lines(copy).map(|line| {
            Text::new(
                line.to_string(),
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            )
        });
        let subtitle = Column::builder()
            .push(line1)
            .gap(4)
            .push(line2)
            .push(line3)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let title = Text::new(
            "Hold to Confirm".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let column = Column::builder()
            .push(subtitle)
            .gap(15)
            .push(title)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self { column }
    }

    pub(crate) fn lines(copy: BackupCopy) -> [&'static str; 3] {
        match copy {
            BackupCopy::Master => ["Verify you've recorded:", "- Key number", "- All 25 words"],
            BackupCopy::CardHalf => [
                "Verify you've recorded:",
                "- Words 1-32",
                "33-64 are on your phone",
            ],
        }
    }
}

impl SafetyReminder {
    fn new(copy: BackupCopy) -> Self {
        let [title, line1, line2] = Self::lines(copy);
        let title = Text::new(
            title.to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let line1 = Text::new(
            line1.to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );
        let line2 = Text::new(
            line2.to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );

        let column = Column::builder()
            .push(title)
            .gap(15)
            .push(line1)
            .push(line2)
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self { content: column }
    }

    pub(crate) fn lines(copy: BackupCopy) -> [&'static str; 3] {
        match copy {
            BackupCopy::Master => [
                "Backup Completed",
                "Store it safely in a",
                "secure location",
            ],
            BackupCopy::CardHalf => ["Now let's check them", "Pick each word as", "you wrote it"],
        }
    }
}

impl BackupConfirmationScreen {
    fn new(copy: BackupCopy) -> Self {
        let confirm_content = ConfirmContent::new(copy);
        let initial_content = ConfirmationContent::new(confirm_content);
        let centered_content = Center::new(initial_content);

        let fade_switcher = FadeSwitcher::new(centered_content)
            .with_fade_config(crate::fade_switcher::FadeConfig::new(500));
        let hold_confirm =
            HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, fade_switcher).with_faded_out_button();

        Self {
            hold_confirm,
            fade_triggered: false,
            copy,
        }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_confirmed()
    }
}

impl crate::DynWidget for BackupConfirmationScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.hold_confirm.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.hold_confirm.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.hold_confirm
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_confirm
            .handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.hold_confirm.force_full_redraw();
    }
}

impl crate::Widget for BackupConfirmationScreen {
    type Color = Rgb565;

    fn draw<D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Check if we should trigger the fade
        if !self.fade_triggered && self.hold_confirm.is_confirmed() {
            self.fade_triggered = true;
            // Switch to the safety reminder
            let safety_reminder = SafetyReminder::new(self.copy);
            let safety_content = ConfirmationContent::new(safety_reminder);
            let centered_safety = Center::new(safety_content);
            self.hold_confirm.widget_mut().switch_to(centered_safety);
        }

        self.hold_confirm.draw(target, current_time)
    }
}

/// A type that can be either a ShareIndexPage, WordsPage, AllWordsPage, or BackupConfirmationScreen
type BackupPage = crate::any_of::AnyOf<(
    ShareIndexPage,
    CardHalfLeadPage,
    WordsPage,
    AllWordsPage,
    BackupConfirmationScreen,
)>;

/// The words a backup display shows.
enum BackupWords {
    /// The standard 25 words, after the key number page.
    Share {
        word_indices: [u16; NUM_WORDS],
        share_index: u16,
    },
    /// Words 1-32 of a split card, after a page naming them (the index is in the card).
    CardHalf([u16; CARD_HALF_WORDS]),
}

impl BackupWords {
    fn words(&self) -> &[u16] {
        match self {
            BackupWords::Share { word_indices, .. } => word_indices,
            BackupWords::CardHalf(words) => words,
        }
    }

    /// Pages before the first word page.
    fn lead_pages(&self) -> usize {
        match self {
            BackupWords::Share { .. } | BackupWords::CardHalf(_) => 1,
        }
    }

    fn copy(&self) -> BackupCopy {
        match self {
            BackupWords::Share { .. } => BackupCopy::Master,
            BackupWords::CardHalf(_) => BackupCopy::CardHalf,
        }
    }
}

/// Widget list that generates backup pages
pub struct BackupPageList {
    words: BackupWords,
    total_pages: usize,
}

impl BackupPageList {
    fn new(words: BackupWords) -> Self {
        // the lead page (the key number, or which words), the word pages, all words, then
        // the hold
        let word_pages = words.words().len().div_ceil(WORDS_PER_PAGE);
        let total_pages = words.lead_pages() + word_pages + 1 + 1;

        Self { words, total_pages }
    }
}

impl WidgetList for BackupPageList {
    type Widget = BackupPage;

    fn len(&self) -> usize {
        self.total_pages
    }

    fn get(&self, index: usize) -> Option<WidgetListItem<BackupPage>> {
        if index >= self.total_pages {
            return None;
        }

        if index < self.words.lead_pages() {
            Some(WidgetListItem::new(match self.words {
                BackupWords::Share { share_index, .. } => {
                    BackupPage::new(ShareIndexPage::new(share_index))
                }
                BackupWords::CardHalf(_) => BackupPage::new(CardHalfLeadPage::new()),
            }))
        } else if index == self.total_pages - 1 {
            Some(WidgetListItem::new(BackupPage::new(
                BackupConfirmationScreen::new(self.words.copy()),
            )))
        } else if index == self.total_pages - 2 {
            let all_words = match &self.words {
                BackupWords::Share {
                    word_indices,
                    share_index,
                } => AllWordsPage::new(word_indices, *share_index),
                BackupWords::CardHalf(words) => AllWordsPage::card_half(words),
            };
            let page = BackupPage::new(all_words);
            Some(WidgetListItem::new(page).with_framebuffer_transitions(true))
        } else {
            let all = self.words.words();
            let word_start_index = (index - self.words.lead_pages()) * WORDS_PER_PAGE;
            let mut words = Vec::new();

            for i in 0..WORDS_PER_PAGE {
                let word_index = word_start_index + i;
                if word_index < all.len() {
                    let word_number = word_index + 1;
                    let word = BIP39_WORDS[all[word_index] as usize];
                    words.push((word_number, String::from(word)));
                }
            }

            Some(WidgetListItem::new(BackupPage::new(WordsPage::new(words))))
        }
    }

    fn can_go_prev(&self, from_index: usize, current_widget: &BackupPage) -> bool {
        if from_index == 0 {
            return false;
        }
        // If we're on the last page (confirmation screen)
        if from_index == self.total_pages - 1 {
            // Check if the confirmation screen has been confirmed
            if let Some(confirmation_screen) =
                current_widget.downcast_ref::<BackupConfirmationScreen>()
            {
                // Don't allow going back if confirmed
                return !confirmation_screen.is_confirmed();
            }
        }
        true // Allow navigation for all other cases
    }
}

/// Main widget that displays backup words using PageSlider
#[derive(frostsnap_macros::Widget)]
pub struct BackupDisplay {
    #[widget_delegate]
    page_slider: PageSlider<BackupPageList>,
}

impl BackupDisplay {
    pub fn new(word_indices: [u16; 25], share_index: u16) -> Self {
        Self::with_words(BackupWords::Share {
            word_indices,
            share_index,
        })
    }

    /// Words 1-32 of a split card, numbered 1-32: a page naming them where master shows the
    /// key number, the standard word pages, then all 32 in two columns, then the hold.
    pub fn card_half(words: [u16; CARD_HALF_WORDS]) -> Self {
        Self::with_words(BackupWords::CardHalf(words))
    }

    fn with_words(words: BackupWords) -> Self {
        let page_list = BackupPageList::new(words);
        let mut page_slider = PageSlider::new(page_list);
        page_slider.set_on_page_ready(|page| {
            if let Some(confirmation_screen) = page.downcast_mut::<BackupConfirmationScreen>() {
                confirmation_screen.hold_confirm.fade_in_button();
            }
        });
        page_slider.enable_swipe_up_chevron();

        Self { page_slider }
    }

    /// A page transition is animating.
    pub fn is_sliding(&self) -> bool {
        self.page_slider.is_sliding()
    }

    pub fn is_confirmed(&mut self) -> bool {
        if self.page_slider.current_index() == self.page_slider.total_pages() - 1 {
            let current_widget = self.page_slider.current_widget();
            if let Some(confirmation_screen) =
                current_widget.downcast_ref::<BackupConfirmationScreen>()
            {
                return confirmation_screen.is_confirmed();
            }
        }
        false
    }

    pub fn is_finished(&mut self) -> bool {
        if self.page_slider.current_index() == self.page_slider.total_pages() - 1 {
            let current_widget = self.page_slider.current_widget();
            if let Some(confirmation_screen) =
                current_widget.downcast_ref::<BackupConfirmationScreen>()
            {
                return confirmation_screen.hold_confirm.is_finished();
            }
        }
        false
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::DynWidget;
    use frost_backup::bip39_words::word_to_index;

    #[test]
    fn a_card_half_has_word_pages_then_all_words_then_the_hold() {
        let list = BackupPageList::new(BackupWords::CardHalf([0; CARD_HALF_WORDS]));
        // "Words 1-32 of 64", 11 pages of 3 (the last of 2), all 32, the hold
        assert_eq!(list.total_pages, 14);
        let lead = list.get(0).unwrap();
        assert!(lead.widget.downcast_ref::<CardHalfLeadPage>().is_some());
        let master = BackupPageList::new(BackupWords::Share {
            word_indices: [0; NUM_WORDS],
            share_index: 1,
        });
        assert_eq!(
            master.total_pages,
            1 + 9 + 2,
            "the 25-word pages are unchanged"
        );
    }

    #[test]
    fn all_32_words_fit_the_screen() {
        let widest = word_to_index("WWWWWWWW")
            .or_else(|| word_to_index("ABSTRACT"))
            .unwrap() as u16;
        let mut page = AllWordsPage::card_half(&[widest; CARD_HALF_WORDS]);
        page.set_constraints(Size::new(240, 280));
        let sizing = page.sizing();
        assert!(
            sizing.height <= 280,
            "{} rows of 16 are too tall",
            sizing.height
        );
        assert!(sizing.width <= 240);
    }

    #[test]
    fn the_card_half_copy_fits_the_hold_page() {
        // the hold border takes 5 px a side, and the circle sits under the text
        let width = |text: &str, font| {
            use embedded_graphics::text::{renderer::TextRenderer, Baseline};
            Gray4TextStyle::new(font, PALETTE.on_background)
                .measure_string(text, Point::zero(), Baseline::Top)
                .bounding_box
                .size
                .width
        };
        for line in ConfirmContent::lines(BackupCopy::CardHalf) {
            assert!(width(line, FONT_CONFIRM_TEXT) <= 200, "{line}");
        }
        for line in CardHalfLeadPage::LINES {
            let font = if line == "1-32" {
                FONT_HUGE_MONO
            } else {
                FONT_MED
            };
            assert!(width(line, font) <= 230, "{line}");
        }
        let [title, rest @ ..] = SafetyReminder::lines(BackupCopy::CardHalf);
        assert!(width(title, FONT_CONFIRM_TITLE) <= 200, "{title}");
        for line in rest {
            assert!(width(line, FONT_CONFIRM_TEXT) <= 200, "{line}");
        }
    }
}
