//! Test improved error handling for invalid BIP39 words

use frost_backup::{ShareBackup, ShareBackupError};

#[test]
fn test_invalid_word_error_message() {
    // Test with an invalid word at position 5
    let mut words = ["ABANDON"; 25];
    words[4] = "INVALIDWORD"; // 5th word (0-indexed)

    let result = ShareBackup::from_words(1, words);
    assert!(result.is_err());

    match result {
        Err(ShareBackupError::InvalidBip39Word { word_index, word }) => {
            assert_eq!(word_index, 4);
            assert_eq!(word, "INVALIDWORD");
        }
        _ => panic!("Expected InvalidBip39Word error"),
    }
}

#[test]
fn test_multiple_invalid_words() {
    // Test that we get the error for the first invalid word
    let mut words = ["ABANDON"; 25];
    words[2] = "BADWORD1"; // 3rd word
    words[7] = "BADWORD2"; // 8th word

    let result = ShareBackup::from_words(1, words);
    assert!(result.is_err());

    match result {
        Err(ShareBackupError::InvalidBip39Word { word_index, word }) => {
            assert_eq!(word_index, 2); // Should report first invalid word
            assert_eq!(word, "BADWORD1");
        }
        _ => panic!("Expected InvalidBip39Word error"),
    }
}

#[test]
fn test_fromstr_error_handling() {
    use std::str::FromStr;

    // Test with invalid word
    let invalid_share = "#1 ABANDON ABANDON ABANDON ABANDON INVALIDWORD ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON";

    let result = ShareBackup::from_str(invalid_share);
    assert!(matches!(
        result,
        Err(ShareBackupError::InvalidBip39Word { .. })
    ));

    // Test missing words
    let not_enough = "#1 ABANDON ABANDON";
    let result = ShareBackup::from_str(not_enough);
    assert!(matches!(result, Err(ShareBackupError::NotEnoughWords)));

    // Test too many words
    let too_many = "#1 ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON ABANDON EXTRA";
    let result = ShareBackup::from_str(too_many);
    assert!(matches!(result, Err(ShareBackupError::TooManyWords)));
}
