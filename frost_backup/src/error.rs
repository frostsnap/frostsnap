//! Error types for frost_backup

use alloc::string::String;
use core::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ShareBackupError {
    /// A word at a specific position is not in the BIP39 word list
    InvalidBip39Word {
        /// The word index (0 = share index, 1-25 = words)
        word_index: usize,
        /// The invalid word that was provided
        word: String,
    },
    /// The encoded scalar is not less than the secp256k1 group order
    InvalidScalar,
    /// The share index could not be parsed as a number
    ShareIndexParseError,
    /// The words checksum verification failed
    WordsChecksumFailed,
    /// The polynomial checksum verification failed
    PolyChecksumFailed,
    /// Not enough words were provided (expected 25)
    NotEnoughWords,
    /// Too many words were provided (expected 25)
    TooManyWords,
    /// A `#0` backup encodes the secret itself, not a share
    BareSecret,
    /// The backup is a share (`#i`, `i > 0`), not a bare secret (`#0`)
    NotBareSecret,
}

impl fmt::Display for ShareBackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShareBackupError::InvalidBip39Word { word_index, word } => {
                write!(
                    f,
                    "Word at position {} '{}' is not in BIP39 word list",
                    word_index, word
                )
            }
            ShareBackupError::InvalidScalar => {
                write!(f, "Scalar is not less than the secp256k1 group order")
            }
            ShareBackupError::ShareIndexParseError => {
                write!(f, "Invalid share index format")
            }
            ShareBackupError::WordsChecksumFailed => {
                write!(f, "Words checksum verification failed")
            }
            ShareBackupError::PolyChecksumFailed => {
                write!(f, "Polynomial checksum verification failed")
            }
            ShareBackupError::NotEnoughWords => {
                write!(f, "Not enough words in share")
            }
            ShareBackupError::TooManyWords => {
                write!(f, "Too many words in share")
            }
            ShareBackupError::BareSecret => {
                write!(f, "Backup #0 encodes the secret itself, not a share")
            }
            ShareBackupError::NotBareSecret => {
                write!(f, "Backup is a share, not a bare secret (#0)")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ShareBackupError {}
