use core::convert::TryInto;
use frost_backup::*;
use rand::Rng;
use secp256kfun::{
    proptest::test_runner::{RngAlgorithm, TestRng},
    s,
};

#[test]
fn test_checksum_false_positive_rate() {
    // The words checksum uses WORDS_CHECKSUM_BITS bits
    // So we expect about 1/(2^WORDS_CHECKSUM_BITS) false positives
    let expected_false_positive_rate = 1.0 / (1 << share_backup::WORDS_CHECKSUM_BITS) as f64;

    println!("Using {} checksum bits", share_backup::WORDS_CHECKSUM_BITS);
    println!(
        "Expected false positive rate: 1/{} ≈ {:.6}",
        1 << share_backup::WORDS_CHECKSUM_BITS,
        expected_false_positive_rate
    );

    let mut rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);
    let secret = s!(42);

    // Generate a valid share to corrupt (use small fingerprint for faster tests)
    let fingerprint = schnorr_fun::frost::Fingerprint {
        bit_length: 7,
        tag: "test",
    };
    let (shares, _) = ShareBackup::generate_shares(secret, 2, 3, fingerprint, &mut rng);
    let share = &shares[0];

    let mut false_positives = 0;
    let mut total_corruptions = 0;

    // Test corrupting each word position
    for word_index in 0..25 {
        let original_words = share.to_words();

        // Try many different word substitutions
        // BIP39 has 2048 words, so we'll try a good sample
        for _corruption_attempt in 0..500 {
            let mut corrupted_words = original_words;

            // Pick a random different word
            let original_word = original_words[word_index];

            // Keep trying until we get a different word
            let new_word = loop {
                let new_word_index = rng.gen_range(0..2048);
                let candidate = frost_backup::bip39_words::BIP39_WORDS[new_word_index];
                if candidate != original_word {
                    break candidate;
                }
            };

            corrupted_words[word_index] = new_word;
            total_corruptions += 1;

            // Check if the corrupted share passes validation
            let index_u32: u32 = share.index().try_into().unwrap();
            if ShareBackup::from_words(index_u32, corrupted_words).is_ok() {
                false_positives += 1;
            }
        }
    }

    // Calculate the observed false positive rate
    let observed_rate = false_positives as f64 / total_corruptions as f64;
    let expected_rate = expected_false_positive_rate;

    println!("Total corruptions tested: {}", total_corruptions);
    println!("False positives: {}", false_positives);
    println!("Observed false positive rate: {:.6}", observed_rate);
    println!("Expected false positive rate: {:.6}", expected_rate);

    // Allow some statistical variance - we expect the rate to be within reasonable bounds
    // Using binomial distribution, the standard deviation is sqrt(n*p*(1-p))
    // For ~12500 trials with p=1/2048, std dev ≈ 2.5, so 3 sigma ≈ 7.5
    // This gives us a range of about (false_positives ± 7.5) / total_corruptions
    let lower_bound = expected_rate * 0.5; // Very generous bounds to avoid flaky tests
    let upper_bound = expected_rate * 2.0;

    assert!(
        observed_rate >= lower_bound && observed_rate <= upper_bound,
        "False positive rate {:.6} is outside expected range [{:.6}, {:.6}]",
        observed_rate,
        lower_bound,
        upper_bound
    );
}
