use crate::{
    any_of::AnyOf, palette::PALETTE, prelude::*, touch_listener::TouchListener,
    DefaultTextStyle, Key, SizedBox, FONT_MED,
};
use alloc::{string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use frostsnap_macros::Widget;

// Type aliases to simplify the complex type
/// A button widget that displays a BIP39 word with prefix highlighting
#[derive(Widget)]
pub struct WordButton {
    word: &'static str,
    #[widget_delegate]
    inner: Container<Padding<Row<(Text, Text)>>>,
}

impl WordButton {
    fn new(word: &'static str, prefix: &str) -> TouchListener<Self> {
        // Split the word into prefix and suffix
        let suffix = &word[prefix.len()..];

        // Create two text widgets - prefix in secondary color, suffix in primary
        let prefix_text = Text::new(
            String::from(prefix),
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let suffix_text = Text::new(
            String::from(suffix),
            DefaultTextStyle::new(FONT_MED, PALETTE.primary),
        );

        // Create a row with the text elements, centered vertically
        let word_row = Row::new((prefix_text, suffix_text))
            .with_main_axis_alignment(MainAxisAlignment::Center);

        let word_row = Padding::only(word_row).top(15).bottom(8).build();
        // Wrap in a Container with fixed width, rounded corners, and button styling
        // Width of 110px should fit most BIP39 words comfortably
        // Using surface_variant as the button background color (Material Design elevated button)
        let container = Container::new(word_row)
            .with_width(110)
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(8, 8));

        let word_button = Self {
            word,
            inner: container,
        };

        // Return a TouchListener that can inspect the child to get the word
        TouchListener::new(word_button, |_, _, is_release, child| {
            if is_release {
                None
            } else {
                Some(Key::WordSelector(child.word))
            }
        })
    }
}

// Type alias for a word button or empty placeholder
type WordOrEmpty = AnyOf<(TouchListener<WordButton>, SizedBox)>;
type WordColumn = Column<Vec<WordOrEmpty>>;

/// A widget that displays BIP39 words in two columns for selection
#[derive(Widget)]
pub struct WordSelector {
    words: &'static [&'static str],
    // Two columns of words
    #[widget_delegate]
    columns: Row<(WordColumn, WordColumn)>,
}

impl WordSelector {
    pub fn new(words: &'static [&'static str], prefix: &str) -> Self {
        // Always create 8 positions (4 per column) for consistent spacing
        const MAX_WORDS: usize = 8;
        const WORDS_PER_COLUMN: usize = MAX_WORDS / 2;

        let mut left_words = Vec::new();
        let mut right_words = Vec::new();

        // Fill positions with actual word buttons
        for (i, &word) in words.iter().enumerate() {
            let word_button = WordButton::new(word, prefix);
            let word_any: WordOrEmpty = AnyOf::new(word_button);

            if i % 2 == 0 {
                left_words.push(word_any);
            } else {
                right_words.push(word_any);
            }
        }

        // Fill remaining positions with invisible placeholders
        // WordButton height: FONT_MED line_height (19px) + padding top(15) + bottom(8) = 42px
        let placeholder_height = 42;
        while left_words.len() < WORDS_PER_COLUMN {
            let placeholder: WordOrEmpty = AnyOf::new(SizedBox::<Rgb565>::new(Size::new(110, placeholder_height)));
            left_words.push(placeholder);
        }
        while right_words.len() < WORDS_PER_COLUMN {
            let placeholder: WordOrEmpty = AnyOf::new(SizedBox::<Rgb565>::new(Size::new(110, placeholder_height)));
            right_words.push(placeholder);
        }

        // Create columns with SpaceEvenly for consistent flex distribution
        // With always 4 items per column at exactly 42px each, spacing is now consistent
        let left_column =
            Column::new(left_words).with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        let right_column =
            Column::new(right_words).with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        // Create a row with the two columns, evenly spaced horizontally
        let columns = Row::new((left_column, right_column))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

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
