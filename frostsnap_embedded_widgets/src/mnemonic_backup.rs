use crate::{
    super_draw_target::SuperDrawTarget,
    page_slider::PageSlider, palette::PALETTE, sized_box::SizedBox, text::Text, widget_list::WidgetList, Center, Column, CrossAxisAlignment, HoldToConfirm, MainAxisAlignment, Row, FONT_LARGE, FONT_MED, FONT_SMALL
};
use alloc::{format, string::String, vec::Vec};
use embedded_graphics::{
    geometry::Size,
    pixelcolor::Rgb565,
    text::Alignment,
    prelude::*,
};
use frostsnap_backup::bip39_words::{BIP39_WORDS, FROSTSNAP_BACKUP_WORDS};
use u8g2_fonts::{fonts, U8g2TextStyle};

const WORDS_PER_PAGE: usize = 3;
const FONT: fonts::u8g2_font_inr30_mf = fonts::u8g2_font_inr30_mf;
const FONT_TINY: fonts::u8g2_font_profont10_mf = fonts::u8g2_font_profont10_mf;
const FONT_ALL_WORDS: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;

/// A single page showing the wallet name
#[derive(frostsnap_macros::Widget)]
pub struct WalletNamePage {
    #[widget_delegate]
    center: Center<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        Text<U8g2TextStyle<Rgb565>>,
    )>>,
}

impl WalletNamePage {
    fn new(wallet_name: String) -> Self {
        let label = Text::new(
            "Wallet Name",
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let name_text = Text::new(
            wallet_name,
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary),
        );

        let column = Column::builder()
            .push(label)
            .push_with_gap(name_text, 8)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let center = Center::new(column);

        Self { center }
    }
}

/// A single page showing the device name
#[derive(frostsnap_macros::Widget)]
pub struct DeviceNamePage {
    #[widget_delegate]
    center: Center<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        Text<U8g2TextStyle<Rgb565>>,
    )>>,
}

impl DeviceNamePage {
    fn new(device_name: String) -> Self {
        let label = Text::new(
            "Device Name",
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let name_text = Text::new(
            device_name,
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary),
        );

        let column = Column::builder()
            .push(label)
            .push_with_gap(name_text, 8)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let center = Center::new(column);

        Self { center }
    }
}

/// A single page showing the threshold
#[derive(frostsnap_macros::Widget)]
pub struct ThresholdPage {
    #[widget_delegate]
    center: Center<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        Text<U8g2TextStyle<Rgb565>>,
    )>>,
}

impl ThresholdPage {
    fn new(threshold: u16, total: u16) -> Self {
        let label = Text::new(
            "Threshold",
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let threshold_text = Text::new(
            format!("{} of {}", threshold, total),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary),
        );

        let column = Column::builder()
            .push(label)
            .push_with_gap(threshold_text, 8)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let center = Center::new(column);

        Self { center }
    }
}

/// A single page showing the share index
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexPage {
    #[widget_delegate]
    center: Center<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        Row<(Text<U8g2TextStyle<Rgb565>>, Text<U8g2TextStyle<Rgb565>>)>,
    )>>,
}

impl ShareIndexPage {
    fn new(share_index: u16) -> Self {
        let label = Text::new(
            "Key index",
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let hash = Text::new(
            "#",
            U8g2TextStyle::new(FONT_LARGE, PALETTE.text_secondary)
        );

        let share_text = Text::new(
            format!("{}", share_index),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary),
        );

        let row = Row::new((hash, share_text));

        let column = Column::builder()
            .push(label)
            .push_with_gap(row, 8)
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
        Text<U8g2TextStyle<Rgb565>>,
        SizedBox<Rgb565>,
        Text<U8g2TextStyle<Rgb565>>,
    )>,
}

impl WordRow {
    fn new(word_number: usize, word: &str) -> Self {
        let number_text = Text::new(
            format!("{}.", word_number),
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Left);

        // Convert word to lowercase
        let word_text = Text::new(
            word.to_lowercase(),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary),
        )
        .with_alignment(Alignment::Left);

        let spacer = SizedBox::width(10); // 10 pixels of space between number and word
        let row = Row::new((number_text, spacer, word_text));

        Self { row }
    }
}

/// Enum for different word page layouts
type WordsPageLayout = Center<crate::any_of::AnyOf<(
    Column<(WordRow,)>,
    Column<(WordRow, WordRow)>,
    Column<(WordRow, WordRow, WordRow)>,
)>>;

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
                    Column::new((row1,))
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                ))
            }
            2 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::builder()
                        .push(row1)
                        .push_with_gap(row2, 20)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                ))
            }
            3 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                let row3 = WordRow::new(words[2].0, &words[2].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::builder()
                        .push(row1)
                        .push_with_gap(row2, 20)
                        .push_with_gap(row3, 20)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                ))
            }
            _ => {
                // Should never happen but handle gracefully
                let row1 = WordRow::new(1, "error");
                Center::new(crate::any_of::AnyOf::new(
                    Column::new((row1,))
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                ))
            }
        };

        Self { layout }
    }
}

// Helper type for a single word entry (number + word)
type SingleWordRow = Row<(
    Text<U8g2TextStyle<Rgb565>>,
    Text<U8g2TextStyle<Rgb565>>,
)>;

// Type for a column of 13 word entries
type WordColumn = Column<(
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
    SingleWordRow,
)>;

/// A page showing all 25 words in a simple scrollable format
#[derive(frostsnap_macros::Widget)]
pub struct AllWordsPage {
    #[widget_delegate]
    content: Row<(WordColumn, WordColumn)>,
}

impl AllWordsPage {
    pub fn new(word_indices: &[u16; 25], share_index: u16) -> Self {
        // Helper to create a word row (word_idx is 0-based)
        let make_word_row = |word_idx: usize| -> SingleWordRow {
            Row::new((
                Text::new(
                    format!("{:2}.", word_idx + 1),
                    U8g2TextStyle::new(FONT_ALL_WORDS, PALETTE.text_secondary)
                ),
                Text::new(
                    // Convert to lowercase and format
                    format!("{:<8}", BIP39_WORDS[word_indices[word_idx] as usize].to_lowercase()),
                    U8g2TextStyle::new(FONT_ALL_WORDS, PALETTE.primary)
                ),
            )).with_main_axis_alignment(MainAxisAlignment::Start)
        };

        // Create left column: Share index, then words 1-12
        let left_column = {
            // First row: share index
            let share_row = Row::new((
                Text::new(
                    " #.",
                    U8g2TextStyle::new(FONT_ALL_WORDS, PALETTE.text_secondary)
                ),
                Text::new(
                    format!("{}", share_index),
                    U8g2TextStyle::new(FONT_ALL_WORDS, PALETTE.primary)
                ).with_underline(PALETTE.surface),
            )).with_main_axis_alignment(MainAxisAlignment::Start);

            Column::new((
                share_row,
                make_word_row(0),   // Word 1
                make_word_row(1),   // Word 2
                make_word_row(2),   // Word 3
                make_word_row(3),   // Word 4
                make_word_row(4),   // Word 5
                make_word_row(5),   // Word 6
                make_word_row(6),   // Word 7
                make_word_row(7),   // Word 8
                make_word_row(8),   // Word 9
                make_word_row(9),   // Word 10
                make_word_row(10),  // Word 11
                make_word_row(11),  // Word 12
            )).with_main_axis_alignment(MainAxisAlignment::Center)
                .with_cross_axis_alignment(CrossAxisAlignment::Start)
        };

        // Create right column: Words 13-25
        let right_column = Column::new((
            make_word_row(12), // Word 13
            make_word_row(13), // Word 14
            make_word_row(14), // Word 15
            make_word_row(15), // Word 16
            make_word_row(16), // Word 17
            make_word_row(17), // Word 18
            make_word_row(18), // Word 19
            make_word_row(19), // Word 20
            make_word_row(20), // Word 21
            make_word_row(21), // Word 22
            make_word_row(22), // Word 23
            make_word_row(23), // Word 24
            make_word_row(24), // Word 25
        )).with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Start);

        // Combine the two columns
        let two_columns = Row::new((left_column, right_column)).with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        let content = two_columns;

        Self { content }
    }
}

/// A confirmation screen that shows after backup
pub struct BackupConfirmationScreen {
    hold_confirm: HoldToConfirm<Column<(
        SizedBox<Rgb565>,
        Text<U8g2TextStyle<Rgb565>>,
        SizedBox<Rgb565>,
        Text<U8g2TextStyle<Rgb565>>,
        SizedBox<Rgb565>,
    )>>,
}

impl BackupConfirmationScreen {
    fn new() -> Self {
        // Add more breathing room with larger spacers
        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 40)); // Space before Hold to Confirm

        let confirm_text = Text::new("Hold to Confirm", U8g2TextStyle::new(FONT_MED, PALETTE.on_background));

        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 15)); // Space between texts

        // Subtitle text with line breaks
        let subtitle_text = Text::new(
            "I have written down\nmy key index and\nall 25 words",
            U8g2TextStyle::new(FONT_SMALL, PALETTE.text_secondary)
        ).with_alignment(Alignment::Center);

        let spacer3 = SizedBox::<Rgb565>::new(Size::new(1, 40)); // Space after text

        let confirm_content = Column::new((spacer1, confirm_text, spacer2, subtitle_text, spacer3))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let hold_confirm = HoldToConfirm::new(3000, confirm_content)  // 3 seconds to confirm
            .with_faded_out_button();

        Self {
            hold_confirm
        }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_completed()
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
        self.hold_confirm.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_confirm.handle_vertical_drag(prev_y, new_y, is_release);
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
        self.hold_confirm.draw(target, current_time)
    }
}

/// A type that can be either a WalletNamePage, DeviceNamePage, ThresholdPage, ShareIndexPage, WordsPage, AllWordsPage, or BackupConfirmationScreen
type BackupPage = crate::any_of::AnyOf<(
    WalletNamePage,
    DeviceNamePage,
    ThresholdPage,
    ShareIndexPage,
    WordsPage,
    AllWordsPage,
    BackupConfirmationScreen
)>;

/// Widget list that generates backup pages
pub struct BackupPageList {
    word_indices: [u16; 25],
    share_index: u16,
    wallet_name: String,
    device_name: String,
    threshold: u16,
    total_shares: u16,
    total_pages: usize,
}

impl BackupPageList {
    fn new(
        word_indices: [u16; 25],
        share_index: u16,
        wallet_name: String,
        device_name: String,
        threshold: u16,
        total_shares: u16
    ) -> Self {
        // Calculate total pages: 3 info pages (wallet, device, threshold) + 1 share index page + word pages + 1 all words page + 1 hold to confirm page
        let word_pages = (FROSTSNAP_BACKUP_WORDS + WORDS_PER_PAGE - 1) / WORDS_PER_PAGE;
        let total_pages = 3 + 1 + word_pages + 1 + 1; // 3 info + share + word pages + all words + confirm

        Self {
            word_indices,
            share_index,
            wallet_name,
            device_name,
            threshold,
            total_shares,
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

        let word_pages = (FROSTSNAP_BACKUP_WORDS + WORDS_PER_PAGE - 1) / WORDS_PER_PAGE;

        let page = if index == 0 {
            // First page - Wallet Name
            BackupPage::new(WalletNamePage::new(self.wallet_name.clone()))
        } else if index == 1 {
            // Second page - Device Name
            BackupPage::new(DeviceNamePage::new(self.device_name.clone()))
        } else if index == 2 {
            // Third page - Threshold
            BackupPage::new(ThresholdPage::new(self.threshold, self.total_shares))
        } else if index == 3 {
            // Fourth page - Share index
            BackupPage::new(ShareIndexPage::new(self.share_index))
        } else if index == self.total_pages - 1 {
            // Last page - Backup confirmation screen
            BackupPage::new(BackupConfirmationScreen::new())
        } else if index == self.total_pages - 2 {
            // Second to last page - All words summary
            BackupPage::new(AllWordsPage::new(&self.word_indices, self.share_index))
        } else {
            // Words pages (starting from index 4)
            let word_start_index = (index - 4) * WORDS_PER_PAGE;
            let mut words = Vec::new();

            for i in 0..WORDS_PER_PAGE {
                let word_index = word_start_index + i;
                if word_index < FROSTSNAP_BACKUP_WORDS {
                    let word_number = word_index + 1;
                    // Convert to lowercase here too
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
            if let Some(confirmation_screen) = current_widget.downcast_ref::<BackupConfirmationScreen>() {
                // Don't allow going back if confirmed
                return !confirmation_screen.is_confirmed();
            }
        }
        true // Allow navigation for all other cases
    }
}

/// Main widget that displays mnemonic backup words using PageSlider
#[derive(frostsnap_macros::Widget)]
pub struct MnemonicBackupDisplay {
    #[widget_delegate]
    page_slider: PageSlider<BackupPageList, BackupPage>,
}

impl MnemonicBackupDisplay {
    pub fn new(
        word_indices: [u16; 25],
        share_index: u16,
        wallet_name: String,
        device_name: String,
        threshold: u16,
        total_shares: u16
    ) -> Self {
        let page_list = BackupPageList::new(
            word_indices,
            share_index,
            wallet_name,
            device_name,
            threshold,
            total_shares
        );
        let page_slider = PageSlider::new(page_list, 100)
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
            if let Some(confirmation_screen) = current_widget.downcast_ref::<BackupConfirmationScreen>() {
                return confirmation_screen.is_confirmed();
            }
        }
        false
    }
}