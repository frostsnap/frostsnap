use frost_backup::{recovery, ShareBackup};
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

    // Generate shares
    let mut rng = rand::thread_rng();
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
    let images: Vec<ShareImage> = shares.iter().map(|s| s.share_image()).collect();

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
    let mut images: Vec<ShareImage> = shares1.iter().map(|s| s.share_image()).collect();

    // Add a share from the second sharing at index 2 (same as shares1[1])
    // This creates a conflict: two different shares both claiming to be at index 2
    // Even though both are valid shares of the same secret, they're from different polynomials
    images.push(shares2[1].share_image());

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
        .filter(|img| shares1.iter().any(|s| s.share_image() == **img))
        .count();
    let from_shares2 = found_shares
        .iter()
        .filter(|img| shares2.iter().any(|s| s.share_image() == **img))
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
        shares1[0].share_image(),
        shares1[1].share_image(),
        shares2[0].share_image(),
        shares2[1].share_image(),
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
        .map(|s| s.share_image())
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
