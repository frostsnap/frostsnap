use crate::ShareBackup;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use schnorr_fun::{
    frost::{Fingerprint, SecretShare, ShareImage, ShareIndex, SharedKey},
    fun::prelude::*,
};

/// Errors that can occur during secret recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    /// No shares were provided
    NoSharesProvided,
    /// The polynomial checksum verification failed
    PolynomialChecksumFailed,
    /// Failed to extract secret from a share
    SecretExtractionFailed,
    /// A `#0` bare secret was provided alongside shares (`#i`, `i > 0`)
    BareSecretMixedWithShares,
    /// Several `#0` backups were provided but they encode different secrets
    BareSecretsDiffer,
}

impl core::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RecoveryError::NoSharesProvided => write!(f, "No shares provided"),
            RecoveryError::PolynomialChecksumFailed => {
                write!(f, "Polynomial checksum verification failed")
            }
            RecoveryError::SecretExtractionFailed => {
                write!(f, "Failed to extract secret from share")
            }
            RecoveryError::BareSecretMixedWithShares => {
                write!(f, "A #0 backup must not be combined with shares")
            }
            RecoveryError::BareSecretsDiffer => {
                write!(f, "The #0 backups encode different secrets")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RecoveryError {}

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
///
/// `#0` backups carry the secret itself: they need no interpolation, must all
/// agree, and must not be combined with shares.
pub fn recover_secret(
    shares: &[ShareBackup],
    fingerprint: Fingerprint,
) -> Result<RecoveredSecret, RecoveryError> {
    if shares.is_empty() {
        return Err(RecoveryError::NoSharesProvided);
    }

    if shares.iter().any(|backup| backup.is_bare_secret()) {
        return recover_bare_secret(shares);
    }

    // Reconstruct the SharedKey from share images
    let share_images: Vec<_> = shares
        .iter()
        .filter_map(|backup| backup.share_image())
        .collect();
    let shared_key = SharedKey::from_share_images(share_images);

    // Verify the fingerprint matches
    if shared_key
        .check_fingerprint::<sha2::Sha256>(fingerprint)
        .is_none()
    {
        return Err(RecoveryError::PolynomialChecksumFailed);
    }

    // Extract and verify the secret shares against the reconstructed key
    let mut secret_shares = Vec::with_capacity(shares.len());
    for share in shares {
        let secret_share = share
            .clone()
            .extract_secret(&shared_key)
            .map_err(|_| RecoveryError::SecretExtractionFailed)?;
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

/// Recovers from `#0` backups only: every backup must be a bare secret and
/// they must all encode the same secret.
fn recover_bare_secret(shares: &[ShareBackup]) -> Result<RecoveredSecret, RecoveryError> {
    if !shares.iter().all(|backup| backup.is_bare_secret()) {
        return Err(RecoveryError::BareSecretMixedWithShares);
    }

    let mut secret = None;
    for backup in shares {
        let this = backup
            .clone()
            .extract_bare_secret()
            .map_err(|_| RecoveryError::SecretExtractionFailed)?;
        match secret {
            None => secret = Some(this),
            Some(prev) if prev == this => {}
            Some(_) => return Err(RecoveryError::BareSecretsDiffer),
        }
    }
    let secret = secret.expect("shares is non-empty");

    Ok(RecoveredSecret {
        secret,
        compatible_shares: shares.to_vec(),
        shared_key: ShareBackup::bare_secret_shared_key(secret),
    })
}

/// Recovers the secret from a collection of shares by automatically discovering compatible subsets.
///
/// This function searches through the provided shares to find a valid subset that can reconstruct
/// a SharedKey matching the given fingerprint. It's useful when you have a collection of shares
/// that may include duplicates, shares from different DKG sessions, or corrupted shares.
///
/// If no multi-share subset is found and the threshold is unknown or `1`, each
/// backup is then tried on its own: a `#0` bare secret, or a share of a
/// threshold-`1` key, verifies its polynomial checksum against its own
/// degree-0 commitment.
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
    // A threshold of 1 needs no subset search: every card carries the secret.
    if known_threshold != Some(1) {
        if let Some(recovered) = recover_from_subset(shares, fingerprint, known_threshold) {
            return Some(recovered);
        }
    }

    if matches!(known_threshold, None | Some(1)) {
        // `#0` backups are self-declaring, so try them before lone shares.
        let candidates = shares
            .iter()
            .filter(|s| s.is_bare_secret())
            .chain(shares.iter().filter(|s| !s.is_bare_secret()));
        for candidate in candidates {
            if let Some(recovered) = recover_from_single(candidate, shares) {
                return Some(recovered);
            }
        }
    }

    None
}

/// The multi-share search: `#0` backups are skipped since they have no image.
fn recover_from_subset(
    shares: &[ShareBackup],
    fingerprint: Fingerprint,
    known_threshold: Option<usize>,
) -> Option<RecoveredSecret> {
    // Get share images from all shares
    let share_images: Vec<ShareImage> = shares.iter().filter_map(|s| s.share_image()).collect();

    // Try to find a valid subset of shares
    let (compatible_images, shared_key) =
        find_valid_subset(&share_images, fingerprint, known_threshold)?;

    // Find the ShareBackups that correspond to the compatible images
    let mut compatible_shares = Vec::new();
    for image in &compatible_images {
        // Find the first share that has this image (guaranteed present — the images came from the shares)
        let share = shares
            .iter()
            .find(|s| s.share_image().as_ref() == Some(image))?;
        compatible_shares.push(share.clone());
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

/// Tries to recover from `candidate` alone as a threshold-`1` backup (a `#0`
/// bare secret or a single share), verified by its polynomial checksum
/// against its own degree-0 commitment. The other `shares` that carry the
/// same secret are reported as compatible.
fn recover_from_single(candidate: &ShareBackup, shares: &[ShareBackup]) -> Option<RecoveredSecret> {
    let (secret, shared_key) = if candidate.is_bare_secret() {
        let secret = candidate.clone().extract_bare_secret().ok()?;
        (secret, ShareBackup::bare_secret_shared_key(secret))
    } else {
        let shared_key = SharedKey::from_share_images([candidate.share_image()?]);
        let secret_share = candidate.clone().extract_secret(&shared_key).ok()?;
        (secret_share.share, shared_key)
    };

    let compatible_shares = shares
        .iter()
        .filter(|s| {
            if s.is_bare_secret() {
                (*s).clone().extract_bare_secret().ok() == Some(secret)
            } else {
                (*s).clone().extract_secret(&shared_key).is_ok()
            }
        })
        .cloned()
        .collect();

    Some(RecoveredSecret {
        secret,
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
/// find shares that on their own were single share wallets unless the
/// threshold is known to be `1`. [`recover_secret_fuzzy`] handles the lone
/// share and `#0` cases since it can check polynomial checksums.
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
    // We need at least 2 images for the fingerprint to actually filter anything
    // -- unless we know explicitly that it's the trivial 1-of-n case then we
    // can just go ahead and choose one.
    let min_shares_needed = known_threshold.unwrap_or(2);
    if images.len() < min_shares_needed {
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
    let mut best_match: Option<(SharedKey<Normal, Zero>, usize)> = None;

    // Try subsets from largest to smallest (but at least 2 shares)
    'outer: for subset_size in sizes {
        // Generate all combinations of indices of the given size
        for index_combo in generate_combinations(&indices, subset_size) {
            // For this combination of indices, try all possible share selections
            for share_combo in generate_share_combinations(&index_combo, &shares_by_index) {
                // Try to reconstruct SharedKey from this combination
                let shared_key = SharedKey::from_share_images(share_combo.clone());

                // If threshold was specified, enforce strict matching. You
                // might think that since we're only generating combinations of
                // the right size that nothing can go wrong -- but it is
                // possible to get a threshold 2 poly from a 3 shares if they
                // lie on the right polynomial.
                if let Some(expected_threshold) = known_threshold {
                    if shared_key.point_polynomial().len() != expected_threshold {
                        continue;
                    }
                }

                // Check how many fingerprint bits matched
                if let Some(bits_matched) =
                    shared_key.check_fingerprint::<sha2::Sha256>(fingerprint)
                {
                    // Only update if this is better than our current best
                    let is_better = match &best_match {
                        None => true,
                        Some((_, prev_bits)) => bits_matched > *prev_bits,
                    };

                    if is_better {
                        best_match = Some((shared_key, bits_matched));
                        // Early exit if we found a complete match
                        if bits_matched >= fingerprint.max_bits_total as usize {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    let shared_key = best_match.map(|(key, _)| key)?;

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
