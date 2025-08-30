use core::convert::TryInto;
use frost_backup::*;
use proptest::prelude::*;
use rand::seq::SliceRandom;
use schnorr_fun::frost::ShareImage;
use secp256kfun::{
    marker::*,
    proptest::{
        arbitrary::any,
        strategy::{Just, Strategy},
        test_runner::{RngAlgorithm, TestRng},
    },
    Point, Scalar,
};

mod common;
use common::TEST_SHARES_3_OF_5;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn share_backup_end_to_end(
        secret in any::<Scalar<Secret, NonZero>>(),
        (n_parties, threshold) in (1usize..=10).prop_flat_map(|n| (Just(n), 1usize..=n)),
    ) {
        // Use deterministic RNG for reproducibility
        let mut rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);

        // Generate shares using our backup scheme (use small fingerprint for faster tests)
        let fingerprint = schnorr_fun::frost::Fingerprint {
            bit_length: 7,
            tag: "test",
        };
        let (shares, _poly_commitment) = ShareBackup::generate_shares(
            secret,
            threshold,
            n_parties,
            fingerprint,
            &mut rng
        );

        // Verify we got the right number of shares
        prop_assert_eq!(shares.len(), n_parties);

        // Verify share indices are sequential from 1
        for (i, share) in shares.iter().enumerate() {
            prop_assert_eq!(TryInto::<u32>::try_into(share.index()).unwrap(), (i + 1) as u32);
        }

        // Test encoding and decoding of each share
        for share in &shares {
            let words = share.to_words();
            prop_assert_eq!(words.len(), 25);

            // Test roundtrip through words
            let index_u32: u32 = share.index().try_into().unwrap();
            let decoded = ShareBackup::from_words(index_u32, words)
                .expect("Should decode valid share");

            // Test Display/FromStr roundtrip
            let formatted = share.to_string();
            let parsed: ShareBackup = formatted.parse()
                .expect("Should parse formatted share");
            let parsed_idx: u32 = parsed.index().try_into().unwrap();
            let share_idx: u32 = share.index().try_into().unwrap();
            prop_assert_eq!(parsed_idx, share_idx);
            prop_assert_eq!(parsed.to_words(), share.to_words());

            // Verify the decoded share matches
            let decoded_idx: u32 = decoded.index().try_into().unwrap();
            let share_idx2: u32 = share.index().try_into().unwrap();
            prop_assert_eq!(decoded_idx, share_idx2);
            prop_assert_eq!(decoded.to_words(), share.to_words());
        }

        // Test reconstruction with random threshold-sized subsets using recovery module
        // Use a boolean mask to select which shares to use (like in frost_prop.rs)
        let mut signer_mask = vec![true; threshold];
        signer_mask.extend(vec![false; n_parties - threshold]);

        // Test a few random combinations
        for _ in 0..3.min(n_parties) { // Test up to 3 random combinations
            signer_mask.shuffle(&mut rng);

            let selected_shares: Vec<ShareBackup> = signer_mask
                .iter()
                .zip(shares.iter())
                .filter(|(is_selected, _)| **is_selected)
                .map(|(_, share)| share.clone())
                .collect();

            prop_assert_eq!(selected_shares.len(), threshold, "Should have exactly threshold shares");

            let recovered = recovery::recover_secret(&selected_shares, fingerprint)
                .expect("Recovery should succeed");
            prop_assert_eq!(
                recovered.secret.public(),
                secret.public(),
                "Failed to reconstruct secret with random selection of {} shares",
                threshold
            );
        }

        // Test that threshold-1 shares cannot reconstruct the correct secret
        if threshold > 1 && shares.len() >= threshold {
            let insufficient = &shares[0..threshold-1];
            // This should fail because we don't have enough shares
            let result = recovery::recover_secret(insufficient, fingerprint);
            // We expect an error, but if it somehow succeeds, verify it's not the correct secret
            if let Ok(recovered) = result {
                prop_assert_ne!(
                    recovered.secret.public(),
                    secret.public(),
                    "Should not reconstruct correct secret with insufficient shares"
                );
            }
        }
    }

    #[test]
    fn find_valid_subset_with_noise(
        (bogus_indices, bogus_points) in (0usize..10)
            .prop_flat_map(|n| (
                prop::collection::vec(1u32..6, n),
                prop::collection::vec(any::<Point<Normal, Public, Zero>>(), n)
            ))
    ) {
        // Parse all 5 shares
        let all_valid_shares: Vec<ShareBackup> = TEST_SHARES_3_OF_5
            .iter()
            .map(|s| s.parse().expect("Valid share string"))
            .collect();

        // Randomly select 3 shares using TestRng
        let mut rng = TestRng::deterministic_rng(RngAlgorithm::ChaCha);

        // Get the selected shares
        let valid_shares: Vec<ShareBackup> = all_valid_shares.choose_multiple(&mut rng, 3).cloned().collect();

        // Get share images from valid shares
        let valid_images: Vec<ShareImage> = valid_shares
            .iter()
            .map(|s| s.share_image())
            .collect();

        // Generate bogus share images
        let mut all_images = valid_images.clone();

        // Add bogus shares at random indices
        for (idx, point) in bogus_indices.iter().zip(bogus_points.iter()) {
            // Generate a bogus share using a pre-generated random point
            let bogus_index = Scalar::<Public, _>::from(*idx).non_zero().expect("non-zero");
            let bogus_image = ShareImage {
                index: bogus_index,
                image: *point,
            };
            all_images.push(bogus_image);
        }

        all_images.shuffle(&mut rng);

        // Try to discover the valid shares
        let result = recovery::find_valid_subset(&all_images, Fingerprint::default(), None);

        // Should always find exactly the 3 valid shares
        prop_assert!(result.is_some(), "Should find valid shares among noise");

        let (found_shares, _) = result.unwrap();
        prop_assert_eq!(found_shares.len(), 3, "Should find exactly 3 shares");

        // Verify all found shares are from the valid set
        for found in &found_shares {
            prop_assert!(
                valid_images.contains(found),
                "Found share should be one of the valid shares"
            );
        }
    }
}
