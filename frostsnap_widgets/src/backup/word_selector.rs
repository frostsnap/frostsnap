use crate::{
    fader::Fader, palette::PALETTE, prelude::*, touch_listener::TouchListener, DefaultTextStyle,
    Key, FONT_MED,
};
use alloc::{string::String, vec::Vec};
use embedded_graphics::prelude::*;
use frostsnap_macros::Widget;

// Type aliases to simplify the complex type
/// A button widget that displays a word with prefix highlighting
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
        // Width of 110px should fit most words comfortably
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

type WordColumn = Column<Vec<Fader<TouchListener<WordButton>>>>;

/// A widget that displays words in two columns for selection
pub struct WordSelector {
    words: &'static [&'static str],
    columns: Row<(WordColumn, WordColumn)>,
    /// Ignore touches briefly after appearing to prevent accidental
    /// word selection when double-tapping a letter.
    grace_ms: u64,
    shown_at: Option<crate::Instant>,
}

impl WordSelector {
    #[inline(never)]
    pub fn new(words: &'static [&'static str], prefix: &str) -> Self {
        const MAX_WORDS: usize = 8;
        const WORDS_PER_COLUMN: usize = MAX_WORDS / 2;

        let mut left_words = Vec::new();
        let mut right_words = Vec::new();

        for (i, &word) in words.iter().enumerate() {
            let button = Fader::new(WordButton::new(word, prefix));
            if i % 2 == 0 {
                left_words.push(button);
            } else {
                right_words.push(button);
            }
        }

        // 🫥 invisible placeholders that match real button size
        while left_words.len() < WORDS_PER_COLUMN {
            left_words.push(Fader::new_faded_out(WordButton::new(words[0], prefix)));
        }
        while right_words.len() < WORDS_PER_COLUMN {
            right_words.push(Fader::new_faded_out(WordButton::new(words[0], prefix)));
        }

        let left_column =
            Column::new(left_words).with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);
        let right_column =
            Column::new(right_words).with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        let columns = Row::new((left_column, right_column))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        let bytes = prefix.as_bytes();
        let has_repeated_letter =
            bytes.len() >= 2 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2];
        let grace_ms = if has_repeated_letter { 400 } else { 200 };

        Self {
            words,
            columns,
            grace_ms,
            shown_at: None,
        }
    }
}

impl crate::DynWidget for WordSelector {
    fn set_constraints(&mut self, max_size: Size) {
        self.columns.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.columns.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let shown_at = self.shown_at?;
        if current_time.saturating_duration_since(shown_at) < self.grace_ms {
            return None;
        }
        self.columns.handle_touch(point, current_time, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.columns.force_full_redraw();
    }
}

impl crate::Widget for WordSelector {
    type Color = embedded_graphics::pixelcolor::Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut crate::SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>,
    {
        self.shown_at.get_or_insert(current_time);
        self.columns.draw(target, current_time)
    }
}

impl core::fmt::Debug for WordSelector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WordSelector")
            .field("words", &self.words)
            .finish()
    }
}
