use crate::{
    alignment::{Align, HorizontalAlignment, VerticalAlignment}, center::Center, container::Container, 
    icons::IconWidget, palette::PALETTE, prelude::*, text::Text, touch_listener::TouchListener, 
    Column, CrossAxisAlignment, DynWidget, Instant, Key, KeyTouch, MainAxisAlignment, Padding, Row, 
    Sizing, Widget, FONT_LARGE,
};
use alloc::{string::String, vec::Vec};
use embedded_graphics::{geometry::Point, pixelcolor::Rgb565, prelude::*};
use embedded_iconoir::{
    prelude::IconoirNewIcon, size32px::navigation::NavArrowLeft, Icon as IconoirIcon,
};
use frostsnap_macros::Widget;
use u8g2_fonts::U8g2TextStyle;

// Type aliases to simplify the complex type
type StyledText = Text<U8g2TextStyle<Rgb565>>;
type WordRow = Padding<Align<Row<(StyledText, StyledText)>>>;

/// A word widget that displays a BIP39 word with prefix highlighting
#[derive(Widget)]
pub struct WordTouch {
    word: &'static str,
    index: usize,
    #[widget_delegate]
    inner: WordRow,
}

impl WordTouch {
    fn new(word: &'static str, index: usize, prefix: &str) -> TouchListener<Self> {
        // Split the word into prefix and suffix
        let suffix = &word[prefix.len()..];

        // Create two text widgets - prefix in primary color, suffix in tertiary
        let prefix_text = Text::new(
            String::from(prefix),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.on_background),
        );

        let suffix_text = Text::new(
            String::from(suffix),
            U8g2TextStyle::new(FONT_LARGE, PALETTE.tertiary),
        );

        // Put them in a row with padding
        let word_row = Padding::all(
            5,
            Align::new(Row::new((prefix_text, suffix_text)).with_main_axis_size(MainAxisSize::Min))
                .vertical(VerticalAlignment::Center),
        );

        let word_touch = Self {
            word,
            index,
            inner: word_row,
        };

        TouchListener::new(word_touch, |_, _, is_release, child| {
            if !is_release {
                Some(Key::WordSelector(child.word))
            } else {
                None
            }
        })
    }
}

type WordColumn = Column<Vec<TouchListener<WordTouch>>>;
type BackspaceButton = TouchListener<Container<Align<IconWidget<IconoirIcon<Rgb565, NavArrowLeft>>>>>;
type SecondColumn = Column<(BackspaceButton, WordColumn)>;

/// A widget that displays BIP39 words in two columns for selection
#[derive(Widget)]
pub struct WordSelector {
    words: &'static [&'static str],
    // Two columns: left with words, right with backspace and words
    #[widget_delegate]
    columns: Row<(WordColumn, SecondColumn)>,
}

impl WordSelector {
    pub fn new(words: &'static [&'static str], prefix: &str) -> Self {
        // Split words into two columns
        let mut left_words = Vec::new();
        let mut right_words = Vec::new();

        for (i, &word) in words.iter().enumerate() {
            // Create a WordTouch widget for each word
            let word_touch = WordTouch::new(word, i, prefix);

            if i % 2 == 0 {
                left_words.push(word_touch);
            } else {
                right_words.push(word_touch);
            }
        }

        // Create left column with flex for each word
        let mut left_column = Column::new(left_words).with_main_axis_size(MainAxisSize::Max);

        // Create right words column with flex for each word
        let mut right_words_column =
            Column::new(right_words).with_main_axis_size(MainAxisSize::Max);

        // Make each word flex(1) in its column
        for i in 0..left_column.children.len() {
            left_column.flex_scores[i] = 1;
        }
        for i in 0..right_words_column.children.len() {
            right_words_column.flex_scores[i] = 1;
        }

        // Create backspace button sized to match the input preview backspace
        // The input preview has height 60px, backspace takes 60-4=56px height
        // and width is 1/4 of total width
        let backspace_icon = IconWidget::new(NavArrowLeft::new(PALETTE.error));
        
        // Align the icon to the left center within the container
        let aligned_icon = Align::new(backspace_icon)
            .horizontal(HorizontalAlignment::Left)
            .vertical(VerticalAlignment::Center);

        // Wrap in Container to control size
        // Container will be 60x56 to match the input preview proportions
        let backspace_container = Container::with_size(aligned_icon, Size::new(60, 56));

        let backspace_button = TouchListener::new(backspace_container, |_, _, is_release, _| {
            if !is_release {
                Some(Key::Keyboard('⌫'))
            } else {
                None
            }
        });

        // Create the second column with backspace button at top and words below
        let mut second_column = Column::new((backspace_button, right_words_column))
            .with_cross_axis_alignment(CrossAxisAlignment::End);
        // Make the words take up the remaining space after the backspace button
        second_column.flex_scores[1] = 1;

        // Create a row with the two columns
        let columns = Row::new((left_column, second_column))
            .with_main_axis_alignment(MainAxisAlignment::SpaceBetween);

        Self { words, columns }
    }
}

impl core::fmt::Debug for WordSelector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WordSelector")
            .field("words", &self.words)
            .finish()
    }
}
