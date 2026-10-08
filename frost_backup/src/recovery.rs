use crate::ShareBackup;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use schnorr_fun::{
    frost::{Fingerprint, SecretShare, ShareImage, SharedKey},
    fun::{poly, prelude::*},
};

/// Errors that can occur during secret recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    /// No shares were provided
    NoSharesProvided,
    /// The polynomial interpolated from the shares does not carry the fingerprint
    PolynomialChecksumFailed,
    /// Failed to extract secret from a share
    SecretExtractionFailed,
    /// A `#0` backup was provided alongside shares (`#i`, `i > 0`)
    SecretMixedWithShares,
    /// Several `#0` backups were provided but they encode different secrets
    SecretsDiffer,
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
            RecoveryError::SecretMixedWithShares => {
                write!(f, "A #0 backup must not be combined with shares")
            }
            RecoveryError::SecretsDiffer => {
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
/// The shares must have been generated with the same fingerprint (or you put in
/// a NONE fingerprint). Note that all shares must be compatible with each other
/// for this to succeed.
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

    // Reconstruct the SharedKey from share images; only a `#0` has no image
    let Some(share_images): Option<Vec<_>> =
        shares.iter().map(|backup| backup.share_image()).collect()
    else {
        return recover_from_secrets(shares);
    };
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

/// Recovers from `#0` backups only: every backup must be a `#0` and they must
/// all encode the same secret.
fn recover_from_secrets(shares: &[ShareBackup]) -> Result<RecoveredSecret, RecoveryError> {
    let mut secret = None;
    for share in shares {
        if !share.index().is_zero() {
            return Err(RecoveryError::SecretMixedWithShares);
        }
        let this = share
            .extract_whole_secret()
            .ok_or(RecoveryError::SecretExtractionFailed)?;
        match secret {
            None => secret = Some(this),
            Some(prev) if prev == this => {}
            Some(_) => return Err(RecoveryError::SecretsDiffer),
        }
    }
    let secret = secret.expect("shares is non-empty");

    Ok(RecoveredSecret {
        secret,
        compatible_shares: shares.to_vec(),
        shared_key: SharedKey::from_poly(vec![g!(secret * G).normalize()]),
    })
}

/// Recovers the secret from a collection of shares by automatically discovering compatible subsets.
///
/// This function searches through the provided shares to find a valid subset that can reconstruct
/// a SharedKey matching the given fingerprint. It's useful when you have a collection of shares
/// that may include duplicates, shares from different DKG sessions, or corrupted shares.
///
/// A `#0` backup is tried as a subset of its own and never joins a subset of
/// shares. A lone share is only tried when the threshold is known to be `1`.
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
    // Each backup is a point (index, image), a `#0` at index `0`
    let points: Vec<_> = shares.iter().map(ShareBackup::point).collect();

    // Try to find a valid subset of shares, each passing its polynomial checksum
    let shared_key = search_subsets(
        &points,
        fingerprint,
        known_threshold,
        |subset, shared_key| {
            subset
                .iter()
                .all(|&i| shares[i].poly_checksum_verifies(shared_key))
        },
    )?;

    // Find the first share at each index on the polynomial. Every share on it
    // must pass its polynomial checksum. A `#0` lies on every polynomial of its
    // key but belongs only to the constant one.
    let is_constant = shared_key.point_polynomial().len() == 1;
    let mut compatible_shares: Vec<ShareBackup> = Vec::new();
    for (share, (index, image)) in shares.iter().zip(&points) {
        let on_key = poly::point::eval(shared_key.point_polynomial(), *index).normalize() == *image;
        if !on_key || (index.is_zero() && !is_constant) {
            continue;
        }
        if !share.poly_checksum_verifies(&shared_key) {
            return None;
        }
        if compatible_shares.iter().all(|s| s.index() != *index) {
            compatible_shares.push(share.clone());
        }
    }
    compatible_shares.sort_by_key(|share| share.index());

    // Reconstruct the secret: a `#0` holds it outright
    let secret = match compatible_shares
        .iter()
        .find_map(|share| share.extract_whole_secret())
    {
        Some(secret) => secret,
        None => {
            let mut secret_shares = Vec::with_capacity(compatible_shares.len());
            for share in &compatible_shares {
                secret_shares.push(share.clone().extract_secret(&shared_key).ok()?);
            }
            SecretShare::recover_secret(&secret_shares)
        }
    };

    Some(RecoveredSecret {
        secret,
        compatible_shares,
        shared_key,
    })
}

/// Finds a valid subset of ShareImages that can reconstruct a SharedKey matching the given fingerprint.
///
/// This function tries different combinations of shares to find a valid subset, starting with 2 shares
/// and progressively trying larger subsets. It handles duplicate shares at the same index by trying all
/// alternatives when that index is included.
///
/// Note this finds shares that are compatible with each other -- it doesn't
/// find shares that on their own were single share wallets unless the
/// threshold is known to be `1`. [`recover_secret_fuzzy`] handles `#0`
/// backups, which have no image.
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
    let points: Vec<_> = images
        .iter()
        .map(|image| (image.index.mark_zero(), image.image))
        .collect();
    let shared_key = search_subsets(&points, fingerprint, known_threshold, |_, _| true)?;

    let compatible = images
        .iter()
        .cloned()
        .filter(|image| shared_key.share_image(image.index) == *image)
        .collect();

    Some((compatible, shared_key))
}

/// Finds the key matching the most fingerprint bits, interpolated from a
/// subset of `points` that `accept` approves. Shared by [`find_valid_subset`]
/// and [`recover_secret_fuzzy`].
#[allow(clippy::type_complexity)]
fn search_subsets(
    points: &[(Scalar<Public, Zero>, Point<Normal, Public, Zero>)],
    fingerprint: Fingerprint,
    known_threshold: Option<usize>,
    accept: impl Fn(&[usize], &SharedKey<Normal, Zero>) -> bool,
) -> Option<SharedKey<Normal, Zero>> {
    // Group shares by index to handle duplicates
    let mut shares_by_index: BTreeMap<Scalar<Public, Zero>, Vec<usize>> = BTreeMap::new();
    for (i, (index, _)) in points.iter().enumerate() {
        shares_by_index.entry(*index).or_default().push(i);
    }

    // Get unique indices
    let indices: Vec<Scalar<Public, Zero>> = shares_by_index.keys().copied().collect();
    let n_indices = indices.len();
    let sizes: Vec<_> = match known_threshold {
        Some(known_threshold) => vec![known_threshold],
        // A lone `#0` comes last, so any multi-card key found is preferred
        None => (2..=n_indices).chain([1]).collect(),
    };
    let mut best_match: Option<(SharedKey<Normal, Zero>, usize)> = None;

    // Try subsets from 2 shares up, then a lone `#0`, or only the known threshold
    'outer: for subset_size in sizes {
        // Generate all combinations of indices of the given size
        for index_combo in generate_combinations(&indices, subset_size) {
            // A `#0` stands alone: index `0` only forms a subset of one. A lone
            // share has no fingerprint to check, so it is only tried when the
            // threshold is known to be `1`.
            let has_zero = index_combo.iter().any(|index| index.is_zero());
            let allowed = match subset_size {
                1 => has_zero || known_threshold == Some(1),
                _ => !has_zero,
            };
            if !allowed {
                continue;
            }

            // For this combination of indices, try all possible share selections
            for share_combo in generate_share_combinations(&index_combo, &shares_by_index) {
                // Try to reconstruct SharedKey from this combination
                let subset: Vec<_> = share_combo.iter().map(|&i| points[i]).collect();
                let poly = poly::point::interpolate(&subset);
                let shared_key = SharedKey::from_poly(poly::point::normalize(poly).collect());

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

                    if is_better && accept(&share_combo, &shared_key) {
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

    best_match.map(|(key, _)| key)
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
    indices: &'a [Scalar<Public, Zero>],
    shares_by_index: &'a BTreeMap<Scalar<Public, Zero>, Vec<usize>>,
) -> impl Iterator<Item = Vec<usize>> + 'a {
    // Get the shares at each index (we know all indices exist in the map)
    let shares_per_index: Vec<&Vec<usize>> = indices
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
            let combination: Vec<usize> = current_indices
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
                let combination: Vec<usize> = current_indices
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
