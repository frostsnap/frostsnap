use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use frost_backup::{
    bip39_words::{self, ValidLetters},
    share_backup::ShareBackup,
    NUM_WORDS,
};

#[derive(Debug)]
pub struct BackupModel {
    share_index: String,
    share_index_confirmed: bool,
    words: [Cow<'static, str>; NUM_WORDS],
}

#[derive(Debug)]
pub enum FramebufferMutation {
    SetCharacter { row: usize, pos: usize, char: char },
    DelCharacter { row: usize, pos: usize },
}

#[derive(Debug, Clone)]
pub struct ViewState {
    pub row: usize,               // Which row is being edited (0 = share index)
    pub cursor_pos: usize,        // Position within that row for cursor
    pub completed_rows: usize,    // Number of rows that are fully completed
    pub main_view: MainViewState, // The actual view to show
}

impl ViewState {
    /// Check if we can show the entered words screen
    pub fn can_show_entered_words(&self) -> bool {
        // we don't allow navigating to the entered words page.
        false
        // Can show entered words only when:
        // 1. We're entering a NEW word (row > completed_rows)
        // 2. The current word is empty (cursor_pos == 0)
        // 3. OR all words are complete
        // match &self.main_view {
        //     MainViewState::EnterWord { .. } => {
        //         // Only allow if we're on a new row (not editing existing)
        //         // and the word is empty
        //         self.row >= self.completed_rows && self.cursor_pos == 0
        //     }
        //     MainViewState::AllWordsEntered { .. } => true,
        //     _ => false,
        // }
    }
}

#[derive(Debug, Clone)]
pub enum MainViewState {
    EnterShareIndex {
        current: String,
    },
    EnterWord {
        valid_letters: ValidLetters,
    },
    WordSelect {
        current: String,
        possible_words: &'static [&'static str],
    },
    AllWordsEntered {
        success: Option<ShareBackup>,
    },
}

impl Default for BackupModel {
    fn default() -> Self {
        Self::new()
    }
}

impl BackupModel {
    pub fn new() -> Self {
        Self {
            share_index: String::new(),
            share_index_confirmed: false,
            words: [const { Cow::Borrowed("") }; NUM_WORDS],
        }
    }

    /// Get a mutable reference to the string currently being edited along with its row index
    fn current_string(&mut self) -> (usize, &mut String) {
        if !self.share_index_confirmed {
            (0, &mut self.share_index)
        } else {
            // Find first incomplete word and ensure it's Owned
            let idx = self
                .words
                .iter()
                .position(|w| matches!(w, Cow::Owned(_)) || w.is_empty())
                .unwrap_or(0);

            // Convert to Owned if it's an empty Borrowed
            if self.words[idx].is_empty() {
                self.words[idx] = Cow::Owned(String::new());
            }

            // Now we can safely get a mutable reference to the String
            let string_ref = match &mut self.words[idx] {
                Cow::Owned(s) => s,
                _ => unreachable!("Word should be Owned at this point"),
            };

            (idx + 1, string_ref) // +1 because row 0 is share index
        }
    }

    pub fn add_character(&mut self, c: char) -> Vec<FramebufferMutation> {
        let mut mutations = Vec::new();
        let (row, current) = self.current_string();

        // Limit share index to 7 characters to prevent display overflow
        if row == 0 && current.len() >= 7 {
            return mutations;
        }

        // Add the character
        current.push(c);
        mutations.push(FramebufferMutation::SetCharacter {
            row,
            pos: current.len() - 1,
            char: c,
        });

        // Word-specific logic
        if row > 0 {
            // Special case: if we just typed Q, automatically add U
            if c == 'Q' {
                current.push('U');
                mutations.push(FramebufferMutation::SetCharacter {
                    row,
                    pos: current.len() - 1,
                    char: 'U',
                });
            }

            // No auto-complete - always show word selector for consistent UX
        }

        mutations
    }

    pub fn backspace(&mut self) -> Vec<FramebufferMutation> {
        let mut mutations = Vec::new();
        let (row, current) = self.current_string();

        if current.is_empty() {
            // Current string is empty, go back to previous row
            if row == 0 {
                // Already at share index, nothing to do
                return mutations;
            } else if row == 1 && self.share_index_confirmed {
                // Go back from first word to share index
                self.share_index_confirmed = false;
            } else if row > 1 {
                // Go back to previous word - make it editable
                let prev_word_idx = row - 2; // -1 for 0-based, -1 for share index
                if prev_word_idx < NUM_WORDS {
                    // Convert previous word to Owned (making it editable)
                    let prev_word = self.words[prev_word_idx].to_string();
                    self.words[prev_word_idx] = Cow::Owned(prev_word);
                }
            }
            // Now call backspace again to actually delete from the previous row
            return self.backspace();
        }

        // Delete characters until we have multiple possibilities
        loop {
            if current.pop().is_some() {
                mutations.push(FramebufferMutation::DelCharacter {
                    row,
                    pos: current.len(),
                });

                // For share index (row 0), just delete one character
                if row == 0 {
                    break;
                }

                // For words, check if we now have multiple possibilities
                if current.is_empty() {
                    break;
                }

                let matches = bip39_words::words_with_prefix(current);

                if !matches.is_empty() && matches.len() <= 8 {
                    // We're in word selector range (1-8 matches)
                    // Check if next deletion would give us >8 matches (keyboard state)
                    if current.len() > 1 {
                        let peek_prefix = &current[..current.len() - 1];
                        let peek_matches = bip39_words::words_with_prefix(peek_prefix);
                        if peek_matches.len() > 8 {
                            // Next deletion would show keyboard, stop here at largest word selector
                            break;
                        }
                        // Otherwise continue deleting to find largest word selector
                    } else {
                        // Can't peek further, stop here
                        break;
                    }
                } else {
                    // 0 matches or >8 matches, stop
                    break;
                }
            } else {
                break;
            }
        }

        mutations
    }

    pub fn complete_row(&mut self, completion: &str) -> Vec<FramebufferMutation> {
        let mut mutations = Vec::new();
        let (row, current) = self.current_string();
        let current_len = current.len();

        // Handle the framebuffer mutations the same way for all rows
        if completion.starts_with(current.as_str()) {
            // Add only the remaining characters
            for (i, ch) in completion[current_len..].chars().enumerate() {
                mutations.push(FramebufferMutation::SetCharacter {
                    row,
                    pos: current_len + i,
                    char: ch,
                });
            }
        } else {
            // Replace everything
            // Clear current content
            for i in 0..current_len {
                mutations.push(FramebufferMutation::DelCharacter {
                    row,
                    pos: current_len - 1 - i,
                });
            }

            // Set new content
            for (i, ch) in completion.chars().enumerate() {
                mutations.push(FramebufferMutation::SetCharacter {
                    row,
                    pos: i,
                    char: ch,
                });
            }
        }

        // Update the model based on which row we're completing
        if row == 0 {
            self.share_index = completion.to_string();
            self.share_index_confirmed = true;
        } else {
            // Word - store as Borrowed if it's a valid BIP39 word, otherwise Owned
            let word_idx = row - 1;
            if let Ok(idx) = bip39_words::BIP39_WORDS.binary_search(&completion) {
                self.words[word_idx] = Cow::Borrowed(bip39_words::BIP39_WORDS[idx]);
            } else {
                self.words[word_idx] = Cow::Owned(completion.to_string());
            }
        }

        mutations
    }

    pub fn edit_row(&mut self, row: usize) -> Vec<FramebufferMutation> {
        let mut mutations = Vec::new();

        let to_delete = if row == 0 {
            let to_delete = self.share_index.len();
            self.share_index.clear();
            self.share_index_confirmed = false;
            to_delete
        } else {
            let word_idx = row - 1;
            let to_delete = self.words[word_idx].len();
            self.words[word_idx] = Cow::Owned(String::new());
            to_delete
        };

        for i in 0..to_delete {
            mutations.push(FramebufferMutation::DelCharacter {
                row,
                pos: to_delete - 1 - i,
            });
        }

        mutations
    }

    pub fn view_state(&self) -> ViewState {
        let completed_rows = self.num_completed_rows();

        if !self.share_index_confirmed {
            ViewState {
                row: 0,
                cursor_pos: self.share_index.len(),
                completed_rows,
                main_view: MainViewState::EnterShareIndex {
                    current: self.share_index.clone(),
                },
            }
        } else if let Some(words) = self.get_words_as_static() {
            // All words are entered, try to create ShareBackup
            let share_index = self.share_index.parse::<u32>().expect("must be int");
            let words_array: [&'static str; NUM_WORDS] = *words;

            // Try to create ShareBackup from the entered words
            let success = ShareBackup::from_words(share_index, words_array).ok();

            ViewState {
                row: NUM_WORDS, // Last word row
                cursor_pos: 0,
                completed_rows,
                main_view: MainViewState::AllWordsEntered { success },
            }
        } else {
            // Find first incomplete word
            let idx = self
                .words
                .iter()
                .position(|w| matches!(w, Cow::Owned(_)) || w.is_empty())
                .unwrap_or(0);

            let current_word = match &self.words[idx] {
                Cow::Borrowed("") => "",
                Cow::Borrowed(s) => s,
                Cow::Owned(s) => s.as_str(),
            };

            let row = idx + 1; // +1 because row 0 is share index
            let cursor_pos = current_word.len();
            let current = current_word.to_string();

            let main_view = if current_word.is_empty() {
                // Empty word, show keyboard with all valid starting letters
                let valid_letters = bip39_words::get_valid_next_letters("");
                MainViewState::EnterWord { valid_letters }
            } else {
                // Check how many words match current prefix
                let matches = bip39_words::words_with_prefix(current_word);

                if !matches.is_empty() && matches.len() <= 8 {
                    // Show word selector for 1-8 matches (consistent UX)
                    MainViewState::WordSelect {
                        current,
                        possible_words: matches,
                    }
                } else {
                    // Show keyboard with valid next letters (>8 matches)
                    let valid_letters = bip39_words::get_valid_next_letters(current_word);
                    MainViewState::EnterWord { valid_letters }
                }
            };

            ViewState {
                row,
                cursor_pos,
                completed_rows,
                main_view,
            }
        }
    }

    pub fn num_completed_rows(&self) -> usize {
        // Count share index if confirmed
        let share_rows = if self.share_index_confirmed { 1 } else { 0 };

        // Count only completed words (Borrowed with content, not Owned which means editing)
        let word_rows = self
            .words
            .iter()
            .filter(|w| matches!(w, Cow::Borrowed(s) if !s.is_empty()))
            .count();

        share_rows + word_rows
    }

    pub fn is_complete(&self) -> bool {
        self.share_index_confirmed && self.words.iter().all(|w| !w.is_empty())
    }

    fn get_words_as_static(&self) -> Option<Box<[&'static str; NUM_WORDS]>> {
        // Only return if all words are valid BIP39 words (Borrowed variants)
        if !self.is_complete() {
            return None;
        }

        let mut static_words = [""; NUM_WORDS];
        for (i, word) in self.words.iter().enumerate() {
            match word {
                Cow::Borrowed(s) if !s.is_empty() => static_words[i] = s,
                _ => return None, // Not a valid static BIP39 word
            }
        }

        Some(Box::new(static_words))
    }
}
