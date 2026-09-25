mod common;

use frost_backup::{recovery, ShareBackup};
use rand::{rngs::StdRng, SeedableRng};
use schnorr_fun::frost::{ShareImage, SharedKey};
use secp256kfun::prelude::*;

const TEST_FINGERPRINT: schnorr_fun::frost::Fingerprint = schnorr_fun::frost::Fingerprint {
    bits_per_coeff: 10,
    max_bits_total: 20,
    tag: "test",
};

#[test]
fn test_recover_secret() {
    // Generate a test secret
    let secret = s!(42);

    // 🎲 deterministic for testing since some checksums only work probabilistically
    let mut rng = StdRng::seed_from_u64(42);
    let (shares, _) = ShareBackup::generate_shares(secret, 2, 3, TEST_FINGERPRINT, &mut rng);

    // Test recovery with exact threshold
    let recovered =
        recovery::recover_secret(&shares[0..2], TEST_FINGERPRINT).expect("Recovery should succeed");
    assert_eq!(recovered.secret.public(), secret.public());

    // Test recovery with more than threshold
    let recovered =
        recovery::recover_secret(&shares, TEST_FINGERPRINT).expect("Recovery should succeed");
    assert_eq!(recovered.secret.public(), secret.public());

    // Test with wrong fingerprint (should fail)
    let wrong_fingerprint = schnorr_fun::frost::Fingerprint {
        bits_per_coeff: 10,
        max_bits_total: 20,
        tag: "wrong",
    };
    assert!(recovery::recover_secret(&shares[0..2], wrong_fingerprint).is_err());

    // Test with single share (should fail)
    assert!(recovery::recover_secret(&shares[0..1], TEST_FINGERPRINT).is_err());

    // Test with no shares
    assert!(recovery::recover_secret(&[], TEST_FINGERPRINT).is_err());
}

#[test]
fn test_find_valid_subset() {
    // Tests the basic functionality of find_valid_subset:
    // - Can find valid subsets when given all shares
    // - Can find valid subsets with exactly threshold shares
    // - Correctly rejects shares with wrong fingerprint
    // - Correctly handles edge cases (single share, empty slice)

    // Generate a test secret
    let secret = s!(42);

    // Generate shares (threshold=2, n=4)
    let mut rng = rand::thread_rng();
    let (shares, shared_key) =
        ShareBackup::generate_shares(secret, 2, 4, TEST_FINGERPRINT, &mut rng);

    // Get share images
    let images: Vec<ShareImage> = shares.iter().map(|s| s.share_image().unwrap()).collect();

    // Test with all shares
    let result = recovery::find_valid_subset(&images, TEST_FINGERPRINT, None);
    assert!(result.is_some());
    let (found_shares, found_key) = result.unwrap();
    assert!(found_shares.len() >= 2);
    assert_eq!(found_key.public_key(), shared_key.public_key());

    // Test with minimum threshold shares
    let result = recovery::find_valid_subset(&images[0..2], TEST_FINGERPRINT, None);
    assert!(result.is_some());

    // Test with wrong fingerprint
    let wrong_fingerprint = schnorr_fun::frost::Fingerprint {
        bits_per_coeff: 10,
        max_bits_total: 20,
        tag: "wrong",
    };
    let result = recovery::find_valid_subset(&images, wrong_fingerprint, None);
    assert!(result.is_none());

    // Test with single share (should fail)
    let result = recovery::find_valid_subset(&images[0..1], TEST_FINGERPRINT, None);
    assert!(result.is_none());

    // Test with empty slice
    let result = recovery::find_valid_subset(&[], TEST_FINGERPRINT, None);
    assert!(result.is_none());
}

#[test]
fn test_find_valid_subset_with_conflicting_indices() {
    // Tests that find_valid_subset can handle shares from two different sharings
    // of the SAME secret. Even though both sharings are valid, shares from different
    // sharings shouldn't be mixed together.
    //
    // This simulates a scenario where someone might generate shares multiple times for
    // the same secret (e.g., to change the threshold or number of shares) and accidentally
    // mix shares from the old and new sharings.

    // Use the same secret for both sharings
    let secret = s!(42);

    // Generate first set of shares (threshold=3, n=5)
    let mut rng = rand::thread_rng();
    let (shares1, _shared_key1) =
        ShareBackup::generate_shares(secret, 3, 5, TEST_FINGERPRINT, &mut rng);

    // Generate second set of shares from the SAME secret (but different polynomial)
    let (shares2, _shared_key2) =
        ShareBackup::generate_shares(secret, 3, 5, TEST_FINGERPRINT, &mut rng);

    // Create share images from the first sharing
    let mut images: Vec<ShareImage> = shares1.iter().map(|s| s.share_image().unwrap()).collect();

    // Add a share from the second sharing at index 2 (same as shares1[1])
    // This creates a conflict: two different shares both claiming to be at index 2
    // Even though both are valid shares of the same secret, they're from different polynomials
    images.push(shares2[1].share_image().unwrap());

    // Test discovery - should find valid subset from one of the sharings
    let result = recovery::find_valid_subset(&images, TEST_FINGERPRINT, None);
    assert!(result.is_some());

    let (found_shares, found_key) = result.unwrap();
    // Should have at least threshold shares
    assert!(found_shares.len() >= 3);

    // The found key should match the same secret (both sharings have the same secret)
    assert_eq!(found_key.public_key(), g!(secret * G).normalize());

    // Verify that the found shares can successfully recover the secret
    // This implicitly verifies they're all from the same sharing (not mixed)
    // because mixed shares wouldn't be able to recover the secret

    // Count how many shares came from each sharing
    let from_shares1 = found_shares
        .iter()
        .filter(|img| shares1.iter().any(|s| s.share_image().unwrap() == **img))
        .count();
    let from_shares2 = found_shares
        .iter()
        .filter(|img| shares2.iter().any(|s| s.share_image().unwrap() == **img))
        .count();

    // All shares should come from one sharing or the other, not mixed
    assert!(
        from_shares1 == 0 || from_shares2 == 0,
        "Found shares should all be from the same sharing, but got {} from first and {} from second",
        from_shares1, from_shares2
    );
}

#[test]
fn test_find_valid_subset_mixed_different_secrets() {
    // Tests that find_valid_subset can handle shares from different secrets
    // mixed together. The algorithm should reject invalid combinations and find only
    // the shares that belong to the same secret sharing.
    //
    // This simulates a scenario where someone might accidentally mix shares from
    // completely different secrets (e.g., mixing up shares from different wallets).

    // Test case where we have shares from different secrets mixed together
    let secret1 = s!(42);
    let secret2 = s!(123);

    let mut rng = rand::thread_rng();
    let (shares1, _) = ShareBackup::generate_shares(secret1, 2, 3, TEST_FINGERPRINT, &mut rng);
    let (shares2, _) = ShareBackup::generate_shares(secret2, 2, 3, TEST_FINGERPRINT, &mut rng);

    // Mix shares from both sharings
    let mixed_images = vec![
        shares1[0].share_image().unwrap(),
        shares1[1].share_image().unwrap(),
        shares2[0].share_image().unwrap(),
        shares2[1].share_image().unwrap(),
    ];

    // Should find a valid subset (from one of the sharings)
    let result = recovery::find_valid_subset(&mixed_images, TEST_FINGERPRINT, None);
    assert!(result.is_some());

    let (found_shares, found_key) = result.unwrap();
    assert!(found_shares.len() >= 2);

    // The found key should correspond to either secret1 or secret2
    let secret1_key = g!(secret1 * G).normalize();
    let secret2_key = g!(secret2 * G).normalize();
    assert!(
        found_key.public_key() == secret1_key || found_key.public_key() == secret2_key,
        "Found key should match one of the original secrets"
    );
}

#[test]
fn test_recover_secret_fuzzy() {
    // Tests that recover_secret_fuzzy can automatically find valid shares from a mixed collection

    // Generate shares from two different secrets
    let secret1 = s!(1337);
    let secret2 = s!(9999);

    let mut rng = rand::thread_rng();
    let (shares1, _) = ShareBackup::generate_shares(secret1, 2, 3, TEST_FINGERPRINT, &mut rng);
    let (shares2, _) = ShareBackup::generate_shares(secret2, 2, 3, TEST_FINGERPRINT, &mut rng);

    // Mix shares from both sharings
    let mut mixed_shares = vec![
        shares1[0].clone(),
        shares1[1].clone(),
        shares2[0].clone(),
        shares2[1].clone(),
    ];

    // Add some duplicates
    mixed_shares.push(shares1[0].clone());
    mixed_shares.push(shares2[1].clone());

    // Try fuzzy recovery - should find one valid set
    let result = recovery::recover_secret_fuzzy(&mixed_shares, TEST_FINGERPRINT, None);
    assert!(result.is_some());

    let recovered = result.unwrap();
    assert_eq!(recovered.compatible_shares.len(), 2); // Should use exactly threshold shares

    // The recovered secret should match one of the originals
    assert!(
        recovered.secret == secret1 || recovered.secret == secret2,
        "Recovered secret should match one of the original secrets"
    );

    // Verify the shared key matches the shares used
    let share_images: Vec<_> = recovered
        .compatible_shares
        .iter()
        .map(|s| s.share_image().unwrap())
        .collect();
    let reconstructed_key = SharedKey::from_share_images(share_images);
    assert_eq!(
        reconstructed_key.public_key(),
        recovered.shared_key.public_key()
    );
}

#[test]
fn test_recover_secret_fuzzy_no_valid_shares() {
    // Test that recover_secret_fuzzy returns None when no valid subset exists

    let secret = s!(42);
    let mut rng = rand::thread_rng();
    let (shares, _) = ShareBackup::generate_shares(secret, 3, 5, TEST_FINGERPRINT, &mut rng);

    // Only provide 2 shares when threshold is 3
    let insufficient_shares = vec![shares[0].clone(), shares[1].clone()];

    let result = recovery::recover_secret_fuzzy(&insufficient_shares, TEST_FINGERPRINT, None);
    assert!(result.is_none());
}

#[test]
fn test_recover_threshold_one() {
    // Threshold-1 shares are emitted as #0 bare secrets and recover on their
    // own, whether or not the threshold is known.
    let secret = s!(42);
    let mut rng = rand::thread_rng();
    let (shares, shared_key) =
        ShareBackup::generate_shares(secret, 1, 3, TEST_FINGERPRINT, &mut rng);

    assert!(shares.iter().all(|s| s.is_bare_secret()));
    assert!(shares.iter().all(|s| s.share_image().is_none()));

    for known_threshold in [Some(1), None] {
        let recovered =
            recovery::recover_secret_fuzzy(&shares[0..1], TEST_FINGERPRINT, known_threshold)
                .expect("a lone #0 backup should recover");
        assert_eq!(recovered.secret.public(), secret.public());
        assert_eq!(recovered.shared_key.public_key(), shared_key.public_key());
        assert_eq!(recovered.compatible_shares.len(), 1);
    }

    // All three carry the same secret, so both strict and fuzzy recovery
    // accept the whole set.
    let recovered = recovery::recover_secret(&shares, TEST_FINGERPRINT).unwrap();
    assert_eq!(recovered.secret.public(), secret.public());
    assert_eq!(recovered.compatible_shares.len(), 3);

    let recovered = recovery::recover_secret_fuzzy(&shares, TEST_FINGERPRINT, None).unwrap();
    assert_eq!(recovered.compatible_shares.len(), 3);
}

#[test]
fn test_recover_lone_share_of_threshold_one_key() {
    // Threshold-1 backups issued before #0 existed carry a non-zero index
    // (the #1 1-of-1 test vector) and must still be discovered from one card.
    use crate::common::TEST_SHARES_1_OF_1;
    use core::str::FromStr;

    let share: ShareBackup = TEST_SHARES_1_OF_1[0].parse().unwrap();
    assert!(!share.is_bare_secret());

    let recovered =
        recovery::recover_secret_fuzzy(&[share.clone()], frost_backup::FINGERPRINT, None)
            .expect("a lone threshold-1 share should recover");
    let expected = Scalar::<Secret, Zero>::from_str(
        "0101010101010101010101010101010101010101010101010101010101010101",
    )
    .unwrap();
    assert_eq!(recovered.secret, expected);
    assert_eq!(recovered.compatible_shares, vec![share]);
}

#[test]
fn test_recover_threshold_one_key_with_mixed_zero_and_nonzero_indices() {
    // A threshold-1 key backed up as #0 on one device and #1 on another
    // (e.g. before and after a firmware update) carries the same scalar on
    // both cards. Fuzzy recovery accepts either card alone and reports both
    // as compatible.
    use crate::common::{TEST_BARE_SECRET, TEST_SHARES_1_OF_1};

    let bare: ShareBackup = TEST_BARE_SECRET.parse().unwrap();
    let share: ShareBackup = TEST_SHARES_1_OF_1[0].parse().unwrap();
    let mixed = vec![bare.clone(), share.clone()];

    for known_threshold in [None, Some(1)] {
        let recovered =
            recovery::recover_secret_fuzzy(&mixed, frost_backup::FINGERPRINT, known_threshold)
                .expect("should recover from the #0 card");
        assert_eq!(
            recovered.secret,
            bare.clone().extract_bare_secret().unwrap()
        );
        assert_eq!(recovered.compatible_shares.len(), 2);
    }
}

#[test]
fn test_bare_secret_not_mixed_with_shares() {
    let secret = s!(42);
    let mut rng = rand::thread_rng();
    let (shares, _) = ShareBackup::generate_shares(secret, 2, 3, TEST_FINGERPRINT, &mut rng);
    let bare = ShareBackup::from_bare_secret(secret.mark_zero());

    let mut mixed = shares[0..2].to_vec();
    mixed.push(bare);

    assert!(matches!(
        recovery::recover_secret(&mixed, TEST_FINGERPRINT),
        Err(recovery::RecoveryError::BareSecretMixedWithShares)
    ));

    // Fuzzy recovery still finds the 2-of-3 subset and leaves the #0 out.
    let recovered = recovery::recover_secret_fuzzy(&mixed, TEST_FINGERPRINT, None).unwrap();
    assert_eq!(recovered.compatible_shares.len(), 2);
    assert!(recovered
        .compatible_shares
        .iter()
        .all(|s| !s.is_bare_secret()));
}

#[test]
fn test_find_valid_subset_threshold_validation() {
    // Test that find_valid_subset strictly validates threshold when specified.
    //
    // Use TEST_SHARES_2_OF_3 which are from a 2-of-3 wallet (threshold=2, poly length=2)
    // but specify known_threshold=3. This should be rejected.

    use crate::common::{TEST_SHARES_1_OF_1, TEST_SHARES_2_OF_3, TEST_SHARES_3_OF_5};

    // Parse all 3 test shares from 2-of-3 wallet
    let shares_2_of_3: Vec<ShareBackup> = TEST_SHARES_2_OF_3
        .iter()
        .map(|s| s.parse().expect("Should parse test share"))
        .collect();

    // Extract share images
    let share_images_2_of_3: Vec<_> = shares_2_of_3
        .iter()
        .map(|s| s.share_image().unwrap())
        .collect();

    // Try to find valid subset with known_threshold=3
    // These shares are from a threshold-2 wallet, so this should return None
    let result =
        recovery::find_valid_subset(&share_images_2_of_3, frost_backup::FINGERPRINT, Some(3));

    assert!(
        result.is_none(),
        "Should reject shares when discovered threshold (2) doesn't match specified threshold (3)"
    );

    // But it should succeed with the correct threshold
    let result =
        recovery::find_valid_subset(&share_images_2_of_3, frost_backup::FINGERPRINT, Some(2));
    assert!(
        result.is_some(),
        "Should accept shares when threshold matches"
    );

    // Now test with threshold-3 shares
    let shares_3_of_5: Vec<ShareBackup> = TEST_SHARES_3_OF_5
        .iter()
        .map(|s| s.parse().expect("Should parse test share"))
        .collect();

    // Test with only 2 shares from threshold-3 wallet - should fail
    let share_images_insufficient: Vec<_> = shares_3_of_5[0..2]
        .iter()
        .map(|s| s.share_image().unwrap())
        .collect();
    let result = recovery::find_valid_subset(
        &share_images_insufficient,
        frost_backup::FINGERPRINT,
        Some(3),
    );
    assert!(
        result.is_none(),
        "Should fail with only 2 shares when threshold is 3"
    );

    // Test with exactly 3 shares from threshold-3 wallet - should succeed
    let share_images_exact: Vec<_> = shares_3_of_5[0..3]
        .iter()
        .map(|s| s.share_image().unwrap())
        .collect();
    let result =
        recovery::find_valid_subset(&share_images_exact, frost_backup::FINGERPRINT, Some(3));
    assert!(
        result.is_some(),
        "Should succeed with exactly 3 shares when threshold is 3"
    );

    // Test with all 5 shares from threshold-3 wallet - should succeed
    let share_images_all: Vec<_> = shares_3_of_5
        .iter()
        .map(|s| s.share_image().unwrap())
        .collect();
    let result = recovery::find_valid_subset(&share_images_all, frost_backup::FINGERPRINT, Some(3));
    assert!(
        result.is_some(),
        "Should succeed with all 5 shares when threshold is 3"
    );

    // Test adding threshold-1 share to the mix - should still succeed (finds threshold-3 subset)
    let share_1_of_1: ShareBackup = TEST_SHARES_1_OF_1[0]
        .parse()
        .expect("Should parse test share");
    let mut share_images_mixed = share_images_all.clone();
    share_images_mixed.push(share_1_of_1.share_image().unwrap());

    let result =
        recovery::find_valid_subset(&share_images_mixed, frost_backup::FINGERPRINT, Some(3));
    assert!(
        result.is_some(),
        "Should succeed finding threshold-3 subset even with threshold-1 share mixed in"
    );
}
