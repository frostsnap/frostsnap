use crate::ShareBackup;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use schnorr_fun::{
    frost::{Fingerprint, SecretShare, ShareImage, ShareIndex, SharedKey},
    fun::prelude::*,
};

/// The result of recovering a secret from shares.
#[derive(Debug, Clone)]
pub struct RecoveredSecret {
    /// The recovered secret scalar
    pub secret: Scalar<Secret, Zero>,
    /// The shares that were compatible with the recovered shared_key
    pub compatible_shares: Vec<ShareBackup>,
    /// The shared key reconstructed from the shares
    pub shared_key: SharedKey<Normal, Zero>,
}

/// Recovers the original secret from a threshold number of shares.
///
/// The shares must have been generated with the same fingerprint. Note that all
/// shares must be compatible with each other for this to succeed (or you put in
/// a NONE fingerprint).
pub fn recover_secret(
    shares: &[ShareBackup],
    fingerprint: Fingerprint,
) -> Result<RecoveredSecret, &'static str> {
    if shares.is_empty() {
        return Err("No shares provided");
    }

    // Reconstruct the SharedKey from share images
    let share_images: Vec<_> = shares.iter().map(|backup| backup.share_image()).collect();
    let shared_key = SharedKey::from_share_images(share_images);

    // Verify the fingerprint matches
    if !shared_key.check_fingerprint::<sha2::Sha256>(&fingerprint) {
        return Err("Public key fingerprint does not match expected fingerprint");
    }

    // Extract and verify the secret shares against the reconstructed key
    let mut secret_shares = Vec::with_capacity(shares.len());
    for share in shares {
        let secret_share = share
            .clone()
            .extract_secret(&shared_key)
            .map_err(|_| "Failed to extract secret from share")?;
        secret_shares.push(secret_share);
    }

    // Reconstruct the secret
    let reconstructed = SecretShare::recover_secret(&secret_shares);

    Ok(RecoveredSecret {
        secret: reconstructed,
        compatible_shares: shares.to_vec(),
        shared_key,
    })
}

/// Recovers the secret from a collection of shares by automatically discovering compatible subsets.
///
/// This function searches through the provided shares to find a valid subset that can reconstruct
/// a SharedKey matching the given fingerprint. It's useful when you have a collection of shares
/// that may include duplicates, shares from different DKG sessions, or corrupted shares.
///
/// # Arguments
/// * `shares` - A slice of ShareBackup instances to search through
/// * `fingerprint` - The fingerprint that was used when generating the shares
///
/// # Returns
/// * `Some(RecoveredSecret)` - The recovered secret, shares used, and shared key
/// * `None` - If no valid subset is found
///
/// # Example
/// ```no_run
/// # use frost_backup::{ShareBackup, recovery::recover_secret_fuzzy, Fingerprint};
/// # let mixed_shares: Vec<ShareBackup> = vec![];
/// if let Some(recovered) = recover_secret_fuzzy(&mixed_shares, Fingerprint::default(), None) {
///     println!("Recovered secret using {} shares", recovered.compatible_shares.len());
/// }
/// ```
pub fn recover_secret_fuzzy(
    shares: &[ShareBackup],
    fingerprint: Fingerprint,
    known_threshold: Option<usize>,
) -> Option<RecoveredSecret> {
    // Get share images from all shares
    let share_images: Vec<ShareImage> = shares.iter().map(|s| s.share_image()).collect();

    // Try to find a valid subset of shares
    let (compatible_images, shared_key) =
        find_valid_subset(&share_images, fingerprint, known_threshold)?;

    // Find the ShareBackups that correspond to the compatible images
    let mut compatible_shares = Vec::new();
    for image in &compatible_images {
        // Find the first share that has this image
        if let Some(share) = shares.iter().find(|s| &s.share_image() == image) {
            compatible_shares.push(share.clone());
        } else {
            // This shouldn't happen since we got the images from the shares
            return None;
        }
    }

    // Extract secret shares
    let mut secret_shares = Vec::with_capacity(compatible_shares.len());
    for share in &compatible_shares {
        let secret_share = share.clone().extract_secret(&shared_key).ok()?;
        secret_shares.push(secret_share);
    }

    // Reconstruct the secret
    let reconstructed = SecretShare::recover_secret(&secret_shares);

    Some(RecoveredSecret {
        secret: reconstructed,
        compatible_shares,
        shared_key,
    })
}

/// Finds a valid subset of ShareImages that can reconstruct a SharedKey matching the given fingerprint.
///
/// This function tries different combinations of shares to find a valid subset, starting with all shares
/// and progressively trying smaller subsets. It handles duplicate shares at the same index by trying all
/// alternatives when that index is included.
///
/// Note this finds shares that are compatible with each other -- it doesn't
/// find shares that on their own were single share wallets.
///
/// # Arguments
/// * `images` - A slice of ShareImages to search through
/// * `fingerprint` - The fingerprint that the reconstructed SharedKey must match
///
/// # Returns
/// * `Some((share_subset, shared_key))` - A valid subset of shares and the reconstructed SharedKey
/// * `None` - If no valid subset is found
pub fn find_valid_subset(
    images: &[ShareImage],
    fingerprint: Fingerprint,
    known_threshold: Option<usize>,
) -> Option<(BTreeSet<ShareImage>, SharedKey<Normal, Zero>)> {
    if images.len() < 2 {
        // Can't verify fingerprint with less than 2 shares
        return None;
    }

    // Group shares by index to handle duplicates
    let mut shares_by_index: BTreeMap<ShareIndex, Vec<ShareImage>> = BTreeMap::new();
    for image in images {
        shares_by_index
            .entry(image.index)
            .or_insert_with(Vec::new)
            .push(*image);
    }

    // Get unique indices
    let indices: Vec<ShareIndex> = shares_by_index.keys().copied().collect();
    let n_indices = indices.len();
    let sizes: Vec<_> = match known_threshold {
        Some(known_threshold) => vec![known_threshold],
        None => (2..=n_indices).collect(),
    };
    let mut found = None;

    // Try subsets from largest to smallest (but at least 2 shares)
    for subset_size in sizes {
        // Generate all combinations of indices of the given size
        for index_combo in generate_combinations(&indices, subset_size) {
            // For this combination of indices, try all possible share selections
            for share_combo in generate_share_combinations(&index_combo, &shares_by_index) {
                // Try to reconstruct SharedKey from this combination
                let shared_key = SharedKey::from_share_images(share_combo.clone());

                // The poly must have at least 2 coefficients for us to discover
                // shares that are compatible with each other.
                if shared_key.point_polynomial().len() < 2 {
                    continue;
                }

                // Check if it matches the fingerprint
                if shared_key.check_fingerprint::<sha2::Sha256>(&fingerprint) {
                    found = Some(shared_key);
                    break;
                }
            }
        }
    }

    let shared_key = found?;

    let compatible = images
        .iter()
        .cloned()
        .filter(|image| shared_key.share_image(image.index) == *image)
        .collect();

    Some((compatible, shared_key))
}

/// Generate all combinations of k elements from a slice
fn generate_combinations<T: Clone>(elements: &[T], k: usize) -> impl Iterator<Item = Vec<T>> + '_ {
    let n = elements.len();

    // Use a vector to track which elements are included in the current combination
    let mut indices = (0..k).collect::<Vec<usize>>();
    let mut first = true;

    core::iter::from_fn(move || {
        if k > n || k == 0 {
            return None;
        }

        if first {
            first = false;
            let combination: Vec<T> = indices.iter().map(|&i| elements[i].clone()).collect();
            return Some(combination);
        }

        // Find the rightmost index that can be incremented
        let mut i = k;
        for j in (0..k).rev() {
            if indices[j] != j + n - k {
                i = j;
                break;
            }
        }

        // If no index can be incremented, we're done
        if i == k {
            return None;
        }

        // Increment the found index and reset all indices to its right
        indices[i] += 1;
        for j in (i + 1)..k {
            indices[j] = indices[j - 1] + 1;
        }

        let combination: Vec<T> = indices.iter().map(|&i| elements[i].clone()).collect();
        Some(combination)
    })
}

/// Generate all possible share combinations for a given set of indices,
/// handling multiple shares at the same index
fn generate_share_combinations<'a>(
    indices: &'a [ShareIndex],
    shares_by_index: &'a BTreeMap<ShareIndex, Vec<ShareImage>>,
) -> impl Iterator<Item = Vec<ShareImage>> + 'a {
    // Get the shares at each index (we know all indices exist in the map)
    let shares_per_index: Vec<&Vec<ShareImage>> = indices
        .iter()
        .map(|index| {
            shares_by_index
                .get(index)
                .expect("index should exist in map")
        })
        .collect();

    let n = indices.len();

    // Initialize indices for each position (all start at 0)
    let mut current_indices = vec![0; n];
    let mut first = true;

    core::iter::from_fn(move || {
        if n == 0 {
            return None;
        }

        if first {
            first = false;
            // Build first combination
            let combination: Vec<ShareImage> = current_indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| shares_per_index[i][idx])
                .collect();
            return Some(combination);
        }

        // Increment indices (like counting in mixed base)
        let mut position = n - 1;
        loop {
            current_indices[position] += 1;

            if current_indices[position] < shares_per_index[position].len() {
                // Successfully incremented, build next combination
                let combination: Vec<ShareImage> = current_indices
                    .iter()
                    .enumerate()
                    .map(|(i, &idx)| shares_per_index[i][idx])
                    .collect();
                return Some(combination);
            }

            // Need to carry over
            current_indices[position] = 0;

            if position == 0 {
                // We've generated all combinations
                return None;
            }

            position -= 1;
        }
    })
}
