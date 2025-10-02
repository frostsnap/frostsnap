use crate::HOLD_TO_CONFIRM_TIME_MS;
use crate::{
    fonts::{Gray4TextStyle, NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM, NOTO_SANS_24_BOLD},
    page_slider::PageSlider,
    palette::PALETTE,
    prelude::*,
    share_index::ShareIndexWidget,
    widget_list::WidgetList,
    HoldToConfirm,
};
use alloc::{format, string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, text::Alignment};
use frost_backup::{bip39_words::BIP39_WORDS, NUM_WORDS};

const WORDS_PER_PAGE: usize = 3;

/// A single page showing the share index
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexPage {
    #[widget_delegate]
    center: Center<Column<(Text<Gray4TextStyle<'static>>, ShareIndexWidget)>>,
}

impl ShareIndexPage {
    fn new(share_index: u16) -> Self {
        let label = Text::new(
            "Key index",
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.text_secondary),
        );

        let share_index_widget = ShareIndexWidget::new(share_index);

        let column = Column::builder()
            .push(label)
            .gap(8)
            .push(share_index_widget)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let center = Center::new(column);

        Self { center }
    }
}

/// A row showing a word number and the word itself
#[derive(frostsnap_macros::Widget)]
pub struct WordRow {
    #[widget_delegate]
    row: Row<(
        Text<Gray4TextStyle<'static>>,
        SizedBox<Rgb565>,
        Text<Gray4TextStyle<'static>>,
    )>,
}

impl WordRow {
    fn new(word_number: usize, word: &str) -> Self {
        let number_text = Text::new(
            format!("{}.", word_number),
            Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Left);

        let word_text = Text::new(
            word.to_lowercase(),
            Gray4TextStyle::new(&NOTO_SANS_24_BOLD, PALETTE.primary),
        )
        .with_alignment(Alignment::Left);

        let spacer = SizedBox::width(10); // 10 pixels of space between number and word
        let row = Row::new((number_text, spacer, word_text));

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

/// A confirmation screen that shows after backup
#[derive(frostsnap_macros::Widget)]
pub struct BackupConfirmationScreen {
    #[widget_delegate]
    hold_confirm: HoldToConfirm<
        Center<Column<(Text<Gray4TextStyle<'static>>, Text<Gray4TextStyle<'static>>)>>,
    >,
}

impl BackupConfirmationScreen {
    fn new() -> Self {
        let title = Text::new(
            "Backup recorded?",
            Gray4TextStyle::new(&NOTO_SANS_24_BOLD, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let subtitle = Text::new(
            "I have written down my\nkey index and all 25 words",
            Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let column = Column::builder()
            .push(title)
            .gap(10)
            .push(subtitle)
            .with_main_axis_alignment(crate::MainAxisAlignment::SpaceEvenly);

        let content = Center::new(column);

        let hold_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, content)
            .with_faded_out_button();

        Self { hold_confirm }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_completed()
    }
}

/// A type that can be either a ShareIndexPage, WordsPage, or BackupConfirmationScreen
type BackupPage =
    crate::any_of::AnyOf<(ShareIndexPage, WordsPage, BackupConfirmationScreen)>;

/// Widget list that generates backup pages
pub struct BackupPageList {
    word_indices: [u16; 25],
    share_index: u16,
    total_pages: usize,
}

impl BackupPageList {
    fn new(word_indices: [u16; 25], share_index: u16) -> Self {
        // Calculate total pages: 1 share index page + word pages + 1 hold to confirm page
        let word_pages = NUM_WORDS.div_ceil(WORDS_PER_PAGE);
        let total_pages = 1 + word_pages + 1; // share + word pages + confirm

        Self {
            word_indices,
            share_index,
            total_pages,
        }
    }
}

impl WidgetList<BackupPage> for BackupPageList {
    fn len(&self) -> usize {
        self.total_pages
    }

    fn get(&self, index: usize) -> Option<BackupPage> {
        if index >= self.total_pages {
            return None;
        }

        let page = if index == 0 {
            // Share index page
            BackupPage::new(ShareIndexPage::new(self.share_index))
        } else if index == self.total_pages - 1 {
            // Last page - Backup confirmation screen
            BackupPage::new(BackupConfirmationScreen::new())
        } else {
            // Words page
            let word_start_index = (index - 1) * WORDS_PER_PAGE;
            let mut words = Vec::new();

            for i in 0..WORDS_PER_PAGE {
                let word_index = word_start_index + i;
                if word_index < NUM_WORDS {
                    let word_number = word_index + 1;
                    let word = BIP39_WORDS[self.word_indices[word_index] as usize].to_lowercase();
                    words.push((word_number, word));
                }
            }

            BackupPage::new(WordsPage::new(words))
        };

        Some(page)
    }

    fn can_go_prev(&self, from_index: usize, current_widget: &BackupPage) -> bool {
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
    page_slider: PageSlider<BackupPageList, BackupPage>,
}

impl BackupDisplay {
    pub fn new(word_indices: [u16; 25], share_index: u16) -> Self {
        let page_list = BackupPageList::new(word_indices, share_index);
        let page_slider = PageSlider::new(page_list, 40)
            .with_on_page_ready(|page| {
                // Try to downcast to BackupConfirmationScreen
                if let Some(confirmation_screen) = page.downcast_mut::<BackupConfirmationScreen>() {
                    // Fade in the button when the confirmation page is ready
                    confirmation_screen.hold_confirm.fade_in_button();
                }
            })
            .with_swipe_up_chevron();

        Self { page_slider }
    }

    /// Check if the backup has been confirmed via the hold-to-confirm on the last page
    pub fn is_confirmed(&mut self) -> bool {
        // Check if we're on the last page
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
}
