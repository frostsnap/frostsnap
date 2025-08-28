use crate::bip39_words::{word_to_index, BIP39_WORDS, BITS_PER_WORD};
use crate::error::ShareBackupError;
use alloc::{string::ToString, vec::Vec};
use core::{
    fmt,
    ops::{BitOrAssign, Shl},
    str::FromStr,
};
use schnorr_fun::{
    frost::{Fingerprint, SecretShare, ShareImage, ShareIndex, SharedKey},
    fun::{hash::HashAdd, poly, prelude::*},
};
use sha2::{Digest, Sha256};

/// Number of words in a share backup
pub const NUM_WORDS: usize = 25;

/// Number of bits used for the scalar/secret share (32 bytes)
pub const SCALAR_BITS: usize = 256;

/// Number of bits used for polynomial checksum
pub const POLY_CHECKSUM_BITS: u8 = 8;

/// Number of bits used for words checksum  
pub const WORDS_CHECKSUM_BITS: u8 = 11;

/// Start bit position for polynomial checksum (after scalar)
const POLY_CHECKSUM_START: usize = SCALAR_BITS;

/// Start bit position for words checksum (after scalar and poly checksum)
const WORDS_CHECKSUM_START: usize = SCALAR_BITS + POLY_CHECKSUM_BITS as usize;

/// Total number of bits in the backup (must equal NUM_WORDS * BITS_PER_WORD)
const TOTAL_BITS: usize = SCALAR_BITS + POLY_CHECKSUM_BITS as usize + WORDS_CHECKSUM_BITS as usize;

/// A Shamir secret share with checksum for BIP39 word encoding
///
/// Contains a secret share and its polynomial checksum for FROST compatibility.
/// For safety, you can't access the secret share until you've provided the
/// correct polynomial (i.e. [`SharedKey`]). You can however access the *share
/// image* which can allow you to produce the `SharedKey`. See *polynomial
/// checksum* in the README.
///
/// [`SharedKey`]: schnorr_fun::frost::SharedKey
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct ShareBackup {
    share: SecretShare,
    poly_checksum: u16,
}

fn set_bit<T: BitOrAssign + Shl<u8, Output = T> + From<u8>>(target: &mut T, len: u8, index: u8) {
    *target |= T::from(1) << ((len - 1) - index)
}

impl ShareBackup {
    /// Returns the share index (x-coordinate in Shamir's scheme)
    pub fn index(&self) -> ShareIndex {
        self.share.index
    }

    /// Creates a ShareBackup with polynomial checksum for FROST compatibility
    pub fn from_secret_share_and_shared_key(
        secret_share: SecretShare,
        shared_key: &SharedKey,
    ) -> Self {
        let poly_checksum =
            Self::compute_poly_checksum(secret_share.index, &secret_share.share, shared_key);
        ShareBackup {
            share: secret_share,
            poly_checksum,
        }
    }

    /// Decodes a ShareBackup from index and 25 BIP39 words, validating checksums
    pub fn from_words(index: u32, words: [&str; NUM_WORDS]) -> Result<Self, ShareBackupError> {
        // First, convert each word to its BITS_PER_WORD-bit index
        let mut word_indices = [0u16; NUM_WORDS];
        for (i, word) in words.iter().enumerate() {
            word_indices[i] =
                word_to_index(word).ok_or_else(|| ShareBackupError::InvalidBip39Word {
                    word_index: i,
                    word: word.to_string(),
                })? as u16;
        }

        let mut scalar_bytes = [0u8; 32];
        let mut poly_checksum = 0u16;
        let mut words_checksum = 0u16;

        let mut total_bits_processed = 0;

        for &word_idx in &word_indices {
            for bit_offset in (0..BITS_PER_WORD).rev() {
                let bit = (word_idx >> bit_offset) & 1;

                if bit != 0 {
                    if total_bits_processed < SCALAR_BITS {
                        // Scalar bytes
                        let byte_index = total_bits_processed / 8;
                        let bit_in_byte = total_bits_processed % 8;
                        set_bit(&mut scalar_bytes[byte_index], 8, bit_in_byte as _);
                    } else if total_bits_processed < WORDS_CHECKSUM_START {
                        let checksum_bit = total_bits_processed - POLY_CHECKSUM_START;
                        set_bit(&mut poly_checksum, POLY_CHECKSUM_BITS, checksum_bit as _);
                    } else if total_bits_processed < TOTAL_BITS {
                        let checksum_bit = total_bits_processed - WORDS_CHECKSUM_START;
                        set_bit(&mut words_checksum, WORDS_CHECKSUM_BITS, checksum_bit as _);
                    }
                }

                total_bits_processed += 1;
            }
        }

        let scalar = Scalar::from_bytes_mod_order(scalar_bytes);

        // Verify words checksum before creating the share
        let expected_words_checksum = Self::compute_words_checksum(index, &scalar, poly_checksum);
        if expected_words_checksum != words_checksum {
            return Err(ShareBackupError::WordsChecksumFailed);
        }

        // Convert u32 to ShareIndex (Scalar<Public, NonZero>)
        let index_scalar = Scalar::<Secret, Zero>::from(index)
            .non_zero()
            .ok_or(ShareBackupError::InvalidShareIndex)?
            .public();

        let share = SecretShare {
            index: index_scalar,
            share: scalar,
        };

        Ok(ShareBackup {
            share,
            poly_checksum,
        })
    }

    /// Encodes share as 25 word indices (11-bit each) in constant time
    pub fn to_word_indices(&self) -> [u16; NUM_WORDS] {
        let scalar_bytes = self.share.share.to_bytes();

        // Get index as u32
        let index_u32: u32 = self
            .index()
            .try_into()
            .expect("Share index should fit in u32");

        // Calculate words checksum for encoding
        let words_checksum =
            Self::compute_words_checksum(index_u32, &self.share.share, self.poly_checksum);

        let mut word_indices = [0u16; NUM_WORDS];
        let mut word_idx = 0;
        let mut accumulator: u32 = 0;
        let mut bits_in_acc = 0;

        // Process the 32 scalar bytes
        for &byte in &scalar_bytes {
            accumulator = (accumulator << 8) | (byte as u32);
            bits_in_acc += 8;

            // Extract 11-bit words while we have enough bits
            while bits_in_acc >= BITS_PER_WORD as u32 && word_idx < NUM_WORDS {
                let shift = bits_in_acc - BITS_PER_WORD as u32;
                word_indices[word_idx] = (accumulator >> shift) as u16;
                accumulator &= (1u32 << shift) - 1;
                bits_in_acc = shift;
                word_idx += 1;
            }
        }

        // After scalar bytes, we have some bits left in accumulator
        // Add poly_checksum (8 bits) to complete the current word
        accumulator = (accumulator << 8) | (self.poly_checksum as u32);
        word_indices[23] = accumulator as u16;
        word_indices[24] = words_checksum;

        word_indices
    }

    /// Converts share to 25 BIP39 words for display/backup
    pub fn to_words(&self) -> [&'static str; NUM_WORDS] {
        let word_indices = self.to_word_indices();
        let mut words = [""; NUM_WORDS];

        for (i, &word_idx) in word_indices.iter().enumerate() {
            words[i] = BIP39_WORDS[word_idx as usize];
        }

        words
    }

    /// Extracts the secret share after validating polynomial checksum
    pub fn extract_secret<Z: ZeroChoice>(
        self,
        shared_key: &SharedKey<Normal, Z>,
    ) -> Result<SecretShare, ShareBackupError> {
        // Verify polynomial checksum against the shared key
        let poly_checksum =
            Self::compute_poly_checksum(self.index(), &self.share.share, shared_key);
        if poly_checksum != self.poly_checksum {
            return Err(ShareBackupError::PolyChecksumFailed);
        }

        Ok(self.share)
    }

    /// Returns the public share image (index and commitment)
    pub fn share_image(&self) -> ShareImage {
        self.share.share_image()
    }

    fn compute_poly_checksum<Z: ZeroChoice>(
        index: ShareIndex,
        scalar: &Scalar<impl Secrecy, impl ZeroChoice>,
        shared_key: &SharedKey<Normal, Z>,
    ) -> u16 {
        let hash = Sha256::new()
            .add(index.to_bytes())
            .add(scalar)
            .add(shared_key.point_polynomial())
            .finalize();
        // Take first 2 bytes as u16 and shift right to keep only the top 10 bits
        u16::from_be_bytes([hash[0], hash[1]]) >> (16 - POLY_CHECKSUM_BITS)
    }

    fn compute_words_checksum(
        index: u32,
        scalar: &Scalar<impl Secrecy, impl ZeroChoice>,
        poly_checksum: u16,
    ) -> u16 {
        let hash = Sha256::new()
            .add(index.to_be_bytes())
            .add(scalar)
            .add(poly_checksum.to_be_bytes())
            .finalize();
        // Take first 2 bytes as u16 and shift right to keep only the top 9 bits
        u16::from_be_bytes([hash[0], hash[1]]) >> (16 - WORDS_CHECKSUM_BITS)
    }

    /// Generates threshold shares with fingerprint grinding for FROST
    pub fn generate_shares<R: rand_core::RngCore>(
        secret: Scalar<Secret, NonZero>,
        threshold: usize,
        n_shares: usize,
        fingerprint: Fingerprint,
        rng: &mut R,
    ) -> (Vec<ShareBackup>, SharedKey) {
        let poly = poly::scalar::generate_shamir_sharing_poly(secret, threshold, rng);
        let point_poly = poly::scalar::to_point_poly(&poly);
        let mut shared_key =
            SharedKey::from_non_zero_poly(point_poly[0], point_poly[1..].iter().copied());
        let tweak_poly = shared_key.grind_fingerprint::<sha2::Sha256>(fingerprint);
        let poly = poly::scalar::add(poly, tweak_poly).collect::<Vec<_>>();

        // Generate shares by evaluating the polynomial at indices 1..=n_shares
        let shares: Vec<ShareBackup> = (1u32..=n_shares as _)
            .map(|i| {
                let index = Scalar::<Public, _>::from(i)
                    .non_zero()
                    .expect("starts at 1");
                let share_scalar = poly::scalar::eval(&poly, index);
                let poly_checksum = Self::compute_poly_checksum(index, &share_scalar, &shared_key);

                let share = SecretShare {
                    index,
                    share: share_scalar,
                };

                ShareBackup {
                    share,
                    poly_checksum,
                }
            })
            .collect();

        (shares, shared_key)
    }
}

impl fmt::Display for ShareBackup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = self.to_words();
        let index_u32: u32 = self
            .index()
            .try_into()
            .expect("Share index should fit in u32");
        write!(f, "#{} ", index_u32)?;
        for (i, word) in words.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "{}", word)?;
        }
        Ok(())
    }
}

impl FromStr for ShareBackup {
    type Err = ShareBackupError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split_whitespace();

        // Get the index
        let index_str = parts
            .next()
            .ok_or(ShareBackupError::ShareIndexParseError)?
            .trim_start_matches('#');
        let index = index_str
            .parse::<u32>()
            .map_err(|_| ShareBackupError::ShareIndexParseError)?;

        // Get the NUM_WORDS words
        let mut words = [""; NUM_WORDS];
        for word in &mut words {
            *word = parts.next().ok_or(ShareBackupError::NotEnoughWords)?;
        }

        // Check there are no extra words
        if parts.next().is_some() {
            return Err(ShareBackupError::TooManyWords);
        }

        ShareBackup::from_words(index, words)
    }
}
