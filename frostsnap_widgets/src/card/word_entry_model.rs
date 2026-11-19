use alloc::string::String;
use frost_backup::bip39_words::{self, ValidLetters};

/// Once this few words match the letters typed, the word selector takes over.
const SELECT_AT_MOST: usize = 8;

/// The letters typed for one word, with today's entry rules (`BackupModel`, rows > 0): Q
/// brings its U, and the word selector appears once at most 8 words match.
#[derive(Default)]
pub struct WordEntryModel {
    current: String,
}

/// What the entry screen shows under the preview.
#[derive(Clone, Copy)]
pub enum WordEntryView {
    Keyboard {
        valid_letters: ValidLetters,
    },
    Select {
        possible_words: &'static [&'static str],
    },
}

/// 🔐 Both views give away the letters typed, so only the variant and the match count show.
impl core::fmt::Debug for WordEntryView {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WordEntryView::Keyboard { .. } => f.write_str("Keyboard(<redacted>)"),
            WordEntryView::Select { possible_words } => {
                write!(f, "Select({} words)", possible_words.len())
            }
        }
    }
}

impl WordEntryModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    pub fn clear(&mut self) {
        self.current.clear();
    }

    pub fn add_character(&mut self, c: char) {
        self.current.push(c);
        // no BIP39 word has a Q without a U after it
        if c == 'Q' {
            self.current.push('U');
        }
    }

    /// Delete a letter. When that leaves the selector showing, keep deleting back to the
    /// widest selector list, so one tap undoes a whole run of forced letters.
    pub fn backspace(&mut self) {
        self.current.pop();

        if !self.current.is_empty()
            && bip39_words::words_with_prefix(&self.current).len() <= SELECT_AT_MOST
        {
            while let Some(popped) = self.current.pop() {
                if self.current.is_empty() {
                    break;
                }
                let count = bip39_words::words_with_prefix(&self.current).len();
                if count > SELECT_AT_MOST || count == 0 {
                    self.current.push(popped);
                    break;
                }
            }
        }
    }

    /// Fill in the word picked from the selector and return its index.
    pub fn select(&mut self, word: &'static str) -> Option<u16> {
        let index = bip39_words::word_to_index(word)?;
        self.current.clear();
        self.current.push_str(word);
        Some(index as u16)
    }

    pub fn view(&self) -> WordEntryView {
        let matches = bip39_words::words_with_prefix(&self.current);
        if !self.current.is_empty() && !matches.is_empty() && matches.len() <= SELECT_AT_MOST {
            WordEntryView::Select {
                possible_words: matches,
            }
        } else {
            WordEntryView::Keyboard {
                valid_letters: bip39_words::get_valid_next_letters(&self.current),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn type_word(model: &mut WordEntryModel, letters: &str) {
        for c in letters.chars() {
            model.add_character(c);
        }
    }

    #[test]
    fn q_brings_its_u() {
        let mut model = WordEntryModel::new();
        model.add_character('Q');
        assert_eq!(model.current(), "QU");
        assert!(matches!(model.view(), WordEntryView::Select { .. }));
    }

    #[test]
    fn selector_appears_at_eight_matches() {
        let mut model = WordEntryModel::new();
        model.add_character('S');
        assert!(matches!(model.view(), WordEntryView::Keyboard { .. }));
        type_word(&mut model, "IZ");
        match model.view() {
            WordEntryView::Select { possible_words } => {
                assert!(possible_words.len() <= 8);
                assert!(possible_words.contains(&"SIZE"));
            }
            WordEntryView::Keyboard { .. } => panic!("SIZ matches few words"),
        }
        let index = model.select("SIZE").unwrap();
        assert_eq!(bip39_words::BIP39_WORDS[index as usize], "SIZE");
        assert_eq!(model.current(), "SIZE");
    }

    #[test]
    fn selector_shows_exactly_when_one_to_eight_words_match() {
        let (mut saw_eight, mut saw_nine) = (false, false);
        for word in bip39_words::BIP39_WORDS.iter() {
            for typed in 1..=word.len() {
                let prefix = &word[..typed];
                let count = bip39_words::words_with_prefix(prefix).len();
                saw_eight |= count == 8;
                saw_nine |= count == 9;
                let model = WordEntryModel {
                    current: String::from(prefix),
                };
                let selects = matches!(model.view(), WordEntryView::Select { .. });
                assert_eq!(selects, (1..=8).contains(&count), "{prefix}: {count} words");
            }
        }
        assert!(saw_eight && saw_nine, "the boundary is exercised");
    }

    #[test]
    fn keyboard_disables_impossible_letters() {
        let mut model = WordEntryModel::new();
        model.add_character('A');
        let WordEntryView::Keyboard { valid_letters } = model.view() else {
            panic!("A matches many words");
        };
        assert!(valid_letters.is_valid('B'));
        assert!(!valid_letters.is_valid('A'), "no BIP39 word starts with AA");
    }

    #[test]
    fn backspace_goes_back_to_the_widest_list() {
        // SI matches more than 8 words, so deleting the Z of SIZ lands on the keyboard
        let mut model = WordEntryModel::new();
        type_word(&mut model, "SIZ");
        model.backspace();
        assert_eq!(model.current(), "SI");
        assert!(matches!(model.view(), WordEntryView::Keyboard { .. }));

        // every prefix of ZOO has at most 8 matches, so one delete clears the lot
        let mut model = WordEntryModel::new();
        type_word(&mut model, "ZOO");
        model.backspace();
        assert!(model.is_empty());

        // in general a delete stops on the widest selector state, or on the keyboard
        for word in ["ABSURD", "CRUNCH", "MAMMAL", "SHERIFF", "TOMATO", "WRIST"] {
            for typed in 2..=word.len() {
                let mut model = WordEntryModel::new();
                type_word(&mut model, &word[..typed]);
                model.backspace();
                let current = model.current();
                assert!(word.starts_with(current));
                assert!(current.len() < typed);
                if !current.is_empty() && current.len() + 1 < typed {
                    // it deleted a run, so it stopped at the widest list
                    let here = bip39_words::words_with_prefix(current).len();
                    let wider = bip39_words::words_with_prefix(&current[..current.len() - 1]);
                    assert!(here <= 8 && wider.len() > 8, "{word} after {typed}");
                }
            }
        }
    }

    #[test]
    fn backspace_on_qu_clears_it() {
        let mut model = WordEntryModel::new();
        model.add_character('Q');
        model.backspace();
        assert!(model.is_empty());
    }
}
