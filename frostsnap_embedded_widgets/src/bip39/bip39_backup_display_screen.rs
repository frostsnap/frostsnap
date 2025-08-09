use crate::{
    Column, CrossAxisAlignment, page_slider::PageSlider, palette::PALETTE, sized_box::SizedBox, text::Text, widget_list::WidgetList, Row, HoldToConfirm, FONT_LARGE, FONT_MED, FONT_SMALL
};
use alloc::{format, string::String, vec::Vec};
use embedded_graphics::{
    geometry::Size,
    pixelcolor::Rgb565,
    text::Alignment,
};
use frostsnap_backup::bip39_words::{BIP39_WORDS, FROSTSNAP_BACKUP_WORDS};
use u8g2_fonts::{fonts, U8g2TextStyle};

const WORDS_PER_PAGE: usize = 3;

/// A single page showing the share index
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexPage {
    column: Column<(
        Text<U8g2TextStyle<Rgb565>>,
        Text<U8g2TextStyle<Rgb565>>,
        Text<U8g2TextStyle<Rgb565>>,
    )>,
}

impl ShareIndexPage {
    fn new(share_index: u16) -> Self {
        let label = Text::new(
            "Share number",
            U8g2TextStyle::new(FONT_MED, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let share_text = Text::new(
            format!("#{}", share_index),
            U8g2TextStyle::new(fonts::u8g2_font_inr42_mf, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let instruction = Text::new(
            "Write it down",
            U8g2TextStyle::new(FONT_SMALL, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let column = Column::new((label, share_text, instruction)).with_cross_axis_alignment(CrossAxisAlignment::Start);

        Self { column }
    }
}

/// A row showing a word number and the word itself
#[derive(frostsnap_macros::Widget)]
pub struct WordRow {
    column: Row<(
        Text<U8g2TextStyle<Rgb565>>,
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

        let word_text = Text::new(
            String::from(word),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Left);

        let column = Row::new((number_text, word_text));

        Self { column }
    }
}

/// Enum for different word page layouts
type WordsPageLayout = crate::any_of::AnyOf<(
    Column<(WordRow,)>,
    Column<(WordRow, SizedBox<Rgb565>, WordRow)>,
    Column<(WordRow, SizedBox<Rgb565>, WordRow, SizedBox<Rgb565>, WordRow)>,
)>;

/// A page showing up to 3 words
#[derive(frostsnap_macros::Widget)]
pub struct WordsPage {
    layout: WordsPageLayout,
}

impl WordsPage {
    fn new(words: Vec<(usize, String)>) -> Self {
        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 20));
        
        // Build the layout based on how many words we have
        let layout = match words.len() {
            1 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                WordsPageLayout::new(
                    Column::new((row1,))
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                )
            }
            2 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                WordsPageLayout::new(
                    Column::new((row1, spacer, row2))
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                )
            }
            3 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                let row3 = WordRow::new(words[2].0, &words[2].1);
                let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 20));
                WordsPageLayout::new(
                    Column::new((row1, spacer, row2, spacer2, row3))
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                )
            }
            _ => {
                // Should never happen but handle gracefully
                let row1 = WordRow::new(1, "error");
                WordsPageLayout::new(
                    Column::new((row1,))
                        .with_cross_axis_alignment(CrossAxisAlignment::Start)
                )
            }
        };

        Self { layout }
    }
}

/// A type that can be either a ShareIndexPage, WordsPage, or HoldToConfirm page
type BackupPage = crate::any_of::AnyOf<(ShareIndexPage, WordsPage, HoldToConfirm<Text<u8g2_fonts::U8g2TextStyle<Rgb565>>>)>;

/// Widget list that generates backup pages
pub struct BackupPageList {
    word_indices: [u16; 25],
    share_index: u16,
    total_pages: usize,
}

impl BackupPageList {
    fn new(word_indices: [u16; 25], share_index: u16) -> Self {
        // Calculate total pages: 1 share index page + word pages + 1 hold to confirm page
        let total_pages = 1 + (FROSTSNAP_BACKUP_WORDS + WORDS_PER_PAGE - 1) / WORDS_PER_PAGE + 1;
        
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
        use u8g2_fonts::U8g2TextStyle;
        
        if index >= self.total_pages {
            return None;
        }
        
        let page = if index == 0 {
            // Share index page
            BackupPage::new(ShareIndexPage::new(self.share_index))
        } else if index == self.total_pages - 1 {
            // Last page - Hold to confirm
            let confirm_text = Text::new(
                "Backup complete",
                U8g2TextStyle::new(FONT_MED, PALETTE.on_surface)
            ).with_alignment(Alignment::Center);
            
            let hold_confirm = HoldToConfirm::new(
                Size::new(240, 280), // Standard screen size
                1500, // 1.5 seconds hold time
                confirm_text
            );
            
            BackupPage::new(hold_confirm)
        } else {
            // Words page
            let word_start_index = (index - 1) * WORDS_PER_PAGE;
            let mut words = Vec::new();
            
            for i in 0..WORDS_PER_PAGE {
                let word_index = word_start_index + i;
                if word_index < FROSTSNAP_BACKUP_WORDS {
                    let word_number = word_index + 1;
                    let word = BIP39_WORDS[self.word_indices[word_index] as usize];
                    words.push((word_number, String::from(word)));
                }
            }
            
            BackupPage::new(WordsPage::new(words))
        };
        
        Some(page)
    }
}

/// Main widget that displays BIP39 backup words using PageSlider
#[derive(frostsnap_macros::Widget)]
pub struct Bip39BackupDisplay {
    #[widget_delegate]
    page_slider: PageSlider<BackupPageList, BackupPage>,
}

impl Bip39BackupDisplay {
    pub fn new(size: Size, word_indices: [u16; 25], share_index: u16) -> Self {
        let page_list = BackupPageList::new(word_indices, share_index);
        let page_slider = PageSlider::new(page_list, size.height);

        Self { page_slider }
    }
    
    /// Check if the backup has been confirmed via the hold-to-confirm on the last page
    pub fn is_confirmed(&self) -> bool {
        // We're on the last page and it's a HoldToConfirm
        // Unfortunately we can't easily check the confirmation state through the PageSlider
        // This would need a more complex implementation to track state
        false
    }
}

