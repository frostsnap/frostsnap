//! ESP32-C3 Secure Boot v2 firmware signing and verification.
//!
//! Signing is generic over an RNG supplied by the caller so this crate stays
//! `rand`-free; only `rand_core` is depended on. Verification needs no RNG.

use crc::Crc;
use rand_core::{CryptoRng, RngCore};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::pss::SigningKey;
use rsa::signature::{SignatureEncoding, hazmat::RandomizedPrehashSigner};
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey};
use sha2::{Digest, Sha256};

// Re-export so callers don't have to depend on `rsa` directly to name the
// types returned by [`verify_firmware`] / [`secure_boot_pubkey_from_pem`].
pub use rsa::RsaPublicKey;

const SECTOR_SIZE: usize = 4096;
const SIGNATURE_BLOCK_MAGIC: [u8; 4] = [0xE7, 0x02, 0x00, 0x00];
const CRC: Crc<u32> = Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
const RSA_KEY_BYTES: usize = 384;

pub fn sign_firmware<R: RngCore + CryptoRng>(
    firmware: &[u8],
    pem_key: &[u8],
    rng: &mut R,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let pem_str = std::str::from_utf8(pem_key)?;
    let private_key = RsaPrivateKey::from_pkcs8_pem(pem_str)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem_str))?;

    // Pad firmware to sector boundary with 0xFF
    let padded_len = firmware.len().div_ceil(SECTOR_SIZE) * SECTOR_SIZE;
    let mut signed = Vec::with_capacity(padded_len + SECTOR_SIZE);
    signed.extend_from_slice(firmware);
    signed.resize(padded_len, 0xFF);

    let image_digest: [u8; 32] = Sha256::digest(&signed).into();

    let sig_block = build_signature_block(&private_key, &image_digest, rng)?;
    signed.extend_from_slice(&sig_block);

    Ok(signed)
}

/// Result of a successful Secure Boot v2 verification.
///
/// A `Verified` only proves the image is *self-consistently* signed by
/// `public_key` — it does NOT say *who* `public_key` belongs to. Use
/// [`classify_signer`] on `key_digest` to learn whether the signer is the
/// pinned Frostsnap prod or dev key.
#[derive(Debug)]
pub struct Verified {
    /// The RSA public key embedded in (and validated against) the signature block.
    pub public_key: RsaPublicKey,
    /// SHA-256 over the signature block's public-key fields (`block[36..812]`) —
    /// the ESP32 Secure Boot v2 "key digest" burned into device eFuses.
    /// Identifies *which* key signed the image; pass it to [`classify_signer`].
    pub key_digest: [u8; 32],
    /// SHA-256 of the signed firmware body (everything before the signature block).
    pub image_digest: [u8; 32],
}

/// Verify the Secure Boot v2 signature block on a signed firmware image.
///
/// Checks magic + CRC32 + image SHA-256 + Montgomery constants + RSA-PSS
/// signature, all against the modulus embedded in the signature block. A
/// successful return only proves the image is *self-consistently* signed —
/// pair it with [`classify_signer`] (or a direct `verified.public_key == expected`
/// check) to learn *who* signed it.
pub fn verify_firmware(signed_firmware: &[u8]) -> Result<Verified, VerifyError> {
    if signed_firmware.len() < SECTOR_SIZE * 2 {
        return Err(VerifyError::TooSmall);
    }
    if !signed_firmware.len().is_multiple_of(SECTOR_SIZE) {
        return Err(VerifyError::NotSectorAligned);
    }

    let sig_block_offset = signed_firmware.len() - SECTOR_SIZE;
    let block = &signed_firmware[sig_block_offset..];
    let firmware = &signed_firmware[..sig_block_offset];

    if block[0..4] != SIGNATURE_BLOCK_MAGIC {
        return Err(VerifyError::BadMagic);
    }

    let crc_stored = u32::from_le_bytes(block[1196..1200].try_into().unwrap());
    let crc_calc = CRC.checksum(&block[0..1196]);
    if crc_stored != crc_calc {
        return Err(VerifyError::CrcMismatch);
    }

    let image_digest: [u8; 32] = Sha256::digest(firmware).into();
    if block[4..36] != image_digest {
        return Err(VerifyError::DigestMismatch);
    }

    let modulus = BigUint::from_bytes_le(&block[36..420]);
    let exponent = BigUint::from_bytes_le(&block[420..424]);
    let public_key = RsaPublicKey::new(modulus.clone(), exponent)
        .map_err(|e| VerifyError::InvalidPublicKey(e.to_string()))?;

    // Verify Montgomery R
    let r = BigUint::from_bytes_le(&block[424..808]);
    let two_pow_3072 = BigUint::from(1u32) << (RSA_KEY_BYTES * 8);
    let expected_r = (&two_pow_3072 * &two_pow_3072) % &modulus;
    if r != expected_r {
        return Err(VerifyError::MontgomeryMismatch);
    }

    // Verify Montgomery M'
    let m_prime = u32::from_le_bytes(block[808..812].try_into().unwrap());
    let check =
        (&modulus * BigUint::from(m_prime) + BigUint::from(1u32)) % BigUint::from(1u64 << 32);
    if check != BigUint::from(0u32) {
        return Err(VerifyError::MontgomeryMismatch);
    }

    // Verify RSA-PSS signature
    use rsa::pss::VerifyingKey;
    use rsa::signature::hazmat::PrehashVerifier;

    let mut sig_be = block[812..1196].to_vec();
    sig_be.reverse();
    let signature = rsa::pss::Signature::try_from(sig_be.as_slice())
        .map_err(|e| VerifyError::SignatureInvalid(e.to_string()))?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key.clone());
    verifying_key
        .verify_prehash(&image_digest, &signature)
        .map_err(|e| VerifyError::SignatureInvalid(e.to_string()))?;

    // The eFuse key digest is SHA-256 over the public-key fields as they sit
    // in the block (modulus ‖ exponent ‖ Montgomery R ‖ M'). The checks above
    // already proved these are consistent with `public_key`.
    let key_digest: [u8; 32] = Sha256::digest(&block[36..812]).into();

    Ok(Verified {
        public_key,
        key_digest,
        image_digest,
    })
}

#[derive(Debug)]
pub enum VerifyError {
    TooSmall,
    NotSectorAligned,
    BadMagic,
    CrcMismatch,
    DigestMismatch,
    InvalidPublicKey(String),
    MontgomeryMismatch,
    SignatureInvalid(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::TooSmall => {
                write!(f, "signed firmware too small (need at least 2 sectors)")
            }
            VerifyError::NotSectorAligned => write!(f, "signed firmware size not sector-aligned"),
            VerifyError::BadMagic => write!(f, "signature block magic bytes missing"),
            VerifyError::CrcMismatch => write!(f, "signature block CRC32 mismatch"),
            VerifyError::DigestMismatch => {
                write!(f, "image SHA256 digest does not match signature block")
            }
            VerifyError::InvalidPublicKey(e) => {
                write!(f, "invalid RSA public key in signature block: {e}")
            }
            VerifyError::MontgomeryMismatch => {
                write!(f, "Montgomery constants do not match public key")
            }
            VerifyError::SignatureInvalid(e) => {
                write!(f, "RSA-PSS signature verification failed: {e}")
            }
        }
    }
}

impl std::error::Error for VerifyError {}

impl VerifyError {
    /// Whether this error means the input simply isn't a Secure Boot v2 image
    /// at all (raw/unsigned binary), as opposed to a signed image that is
    /// corrupt or tampered.
    pub fn is_not_signed(&self) -> bool {
        matches!(
            self,
            VerifyError::TooSmall | VerifyError::NotSectorAligned | VerifyError::BadMagic
        )
    }
}

/// Which pinned Frostsnap key signed a verified image, or `Unknown` if none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signer {
    Prod,
    Dev,
    Unknown,
}

/// Classify a verified image's `key_digest` against the prod/dev digests the
/// caller has computed from the committed `bootloader/{env}/secure-boot-key.pem`
/// files (via [`secure_boot_pubkey_from_pem`] + [`compute_key_digest`]).
///
/// The pinned digests are passed in rather than read from disk so this crate
/// imposes no filesystem layout on its callers — each consumer decides how to
/// resolve the bootloader directory.
pub fn classify_signer(
    key_digest: &[u8; 32],
    prod: &[u8; 32],
    dev: &[u8; 32],
) -> Signer {
    if key_digest == prod {
        Signer::Prod
    } else if key_digest == dev {
        Signer::Dev
    } else {
        Signer::Unknown
    }
}

/// Load an RSA public key from a `bootloader/{env}/secure-boot-key.pem` file.
///
/// Accepts either form: a PKCS#1/PKCS#8 *private* key PEM (dev's committed key)
/// returns the derived public key, and a SubjectPublicKeyInfo *public* key PEM
/// (prod's committed key — `openssl rsa -pubout` form) is parsed directly. The
/// caller doesn't need to know which form a given env uses.
pub fn secure_boot_pubkey_from_pem(
    pem: &[u8],
) -> Result<RsaPublicKey, Box<dyn std::error::Error>> {
    let pem_str = std::str::from_utf8(pem)?;
    if let Ok(priv_key) = RsaPrivateKey::from_pkcs8_pem(pem_str)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem_str))
    {
        return Ok(priv_key.to_public_key());
    }
    use rsa::pkcs8::DecodePublicKey;
    Ok(RsaPublicKey::from_public_key_pem(pem_str)?)
}

/// Compute the ESP32 Secure Boot v2 eFuse "key digest" for a public key.
///
/// The digest is SHA-256 over the signature block's public-key fields
/// (`modulus ‖ exponent ‖ Montgomery R ‖ M'`) — the same layout
/// [`verify_firmware`] reads from `block[36..812]`. Used to pin a known signer
/// without storing the digest itself: derive it from a committed PEM at startup.
pub fn compute_key_digest(public_key: &RsaPublicKey) -> [u8; 32] {
    let n = public_key.n();
    let e = public_key.e();
    let modulus_le = to_le_bytes_fixed::<RSA_KEY_BYTES>(n);
    let exponent_le = to_le_bytes_fixed::<4>(e);

    let two_pow_3072 = BigUint::from(1u32) << (RSA_KEY_BYTES * 8);
    let r = (&two_pow_3072 * &two_pow_3072) % n;
    let r_le = to_le_bytes_fixed::<RSA_KEY_BYTES>(&r);

    let m_prime = montgomery_m_prime(n);
    let m_prime_le = m_prime.to_le_bytes();

    let mut buf = [0u8; 776];
    buf[0..384].copy_from_slice(&modulus_le);
    buf[384..388].copy_from_slice(&exponent_le);
    buf[388..772].copy_from_slice(&r_le);
    buf[772..776].copy_from_slice(&m_prime_le);
    Sha256::digest(buf).into()
}

fn build_signature_block<R: RngCore + CryptoRng>(
    private_key: &RsaPrivateKey,
    image_digest: &[u8; 32],
    rng: &mut R,
) -> Result<[u8; SECTOR_SIZE], Box<dyn std::error::Error>> {
    let public_key = private_key.to_public_key();
    let n = public_key.n();
    let e = public_key.e();

    let modulus_le = to_le_bytes_fixed::<RSA_KEY_BYTES>(n);
    let exponent_le = to_le_bytes_fixed::<4>(e);

    let two_pow_3072 = BigUint::from(1u32) << (RSA_KEY_BYTES * 8);
    let r = (&two_pow_3072 * &two_pow_3072) % n;
    let r_le = to_le_bytes_fixed::<RSA_KEY_BYTES>(&r);

    let m_prime = montgomery_m_prime(n);
    let m_prime_le = m_prime.to_le_bytes();

    let signing_key = SigningKey::<Sha256>::new(private_key.clone());
    let signature = signing_key.sign_prehash_with_rng(rng, image_digest)?;
    let sig_bytes = signature.to_vec();

    let mut sig_le = [0u8; RSA_KEY_BYTES];
    sig_le.copy_from_slice(&sig_bytes);
    sig_le.reverse();

    let mut block = [0u8; SECTOR_SIZE];
    block[0..4].copy_from_slice(&SIGNATURE_BLOCK_MAGIC);
    block[4..36].copy_from_slice(image_digest);
    block[36..420].copy_from_slice(&modulus_le);
    block[420..424].copy_from_slice(&exponent_le);
    block[424..808].copy_from_slice(&r_le);
    block[808..812].copy_from_slice(&m_prime_le);
    block[812..1196].copy_from_slice(&sig_le);

    let crc_val = CRC.checksum(&block[0..1196]);
    block[1196..1200].copy_from_slice(&crc_val.to_le_bytes());

    block[1216..].fill(0xFF);

    Ok(block)
}

// Montgomery M' = -N^(-1) mod 2^32. N is odd (product of primes) so the inverse
// exists; Newton's method gives 5 iterations of doubling precision.
fn montgomery_m_prime(n: &BigUint) -> u32 {
    let n_low = n.to_bytes_le();
    let n32 = u32::from_le_bytes([n_low[0], n_low[1], n_low[2], n_low[3]]);
    let mut x: u32 = 1;
    for _ in 0..5 {
        x = x.wrapping_mul(2u32.wrapping_sub(n32.wrapping_mul(x)));
    }
    0u32.wrapping_sub(x)
}

fn to_le_bytes_fixed<const N: usize>(val: &BigUint) -> [u8; N] {
    let bytes = val.to_bytes_le();
    let mut result = [0u8; N];
    let len = bytes.len().min(N);
    result[..len].copy_from_slice(&bytes[..len]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use std::sync::LazyLock;

    static TEST_KEY_PEM: LazyLock<Vec<u8>> = LazyLock::new(|| {
        let mut rng = rand::thread_rng();
        let key = RsaPrivateKey::new(&mut rng, 3072).unwrap();
        key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .unwrap()
            .as_bytes()
            .to_vec()
    });

    #[test]
    fn sign_and_verify_aligned() {
        let firmware = vec![0xABu8; 4096 * 4];
        let signed = sign_firmware(&firmware, &TEST_KEY_PEM, &mut rand::thread_rng()).unwrap();
        assert_eq!(signed.len(), 4096 * 5);
        verify_firmware(&signed).unwrap();
    }

    #[test]
    fn sign_and_verify_unaligned() {
        let firmware = vec![0xCDu8; 5000];
        let signed = sign_firmware(&firmware, &TEST_KEY_PEM, &mut rand::thread_rng()).unwrap();
        assert_eq!(signed.len(), 12288);
        assert!(signed[5000..8192].iter().all(|&b| b == 0xFF));
        verify_firmware(&signed).unwrap();
    }

    /// `secure_boot_pubkey_from_pem` must accept both committed forms — a
    /// private key PEM (dev's `secure-boot-key.pem`) and a public key PEM
    /// (prod's `secure-boot-key.pem`) — and return the same `RsaPublicKey`.
    #[test]
    fn pem_loader_accepts_private_and_public_forms() {
        use rsa::pkcs8::{EncodePublicKey, LineEnding};
        let from_priv = secure_boot_pubkey_from_pem(&TEST_KEY_PEM).unwrap();
        let pub_pem = from_priv.to_public_key_pem(LineEnding::LF).unwrap();
        let from_pub = secure_boot_pubkey_from_pem(pub_pem.as_bytes()).unwrap();
        assert_eq!(from_priv, from_pub);
    }

    /// `compute_key_digest` must agree with the digest the verifier extracts
    /// from a real signature block — that's the whole point of pinning a
    /// signer by hashing the same layout bytes.
    #[test]
    fn compute_key_digest_matches_verify_output() {
        let firmware = vec![0xABu8; 4096 * 4];
        let signed = sign_firmware(&firmware, &TEST_KEY_PEM, &mut rand::thread_rng()).unwrap();
        let verified = verify_firmware(&signed).unwrap();
        let recomputed = compute_key_digest(&verified.public_key);
        assert_eq!(verified.key_digest, recomputed);
    }

    #[test]
    fn classify_signer_routes_correctly() {
        let prod = [0x11u8; 32];
        let dev = [0x22u8; 32];
        assert_eq!(classify_signer(&prod, &prod, &dev), Signer::Prod);
        assert_eq!(classify_signer(&dev, &prod, &dev), Signer::Dev);
        assert_eq!(classify_signer(&[0xFFu8; 32], &prod, &dev), Signer::Unknown);
    }

    #[test]
    fn matches_espsecure_format() {
        let reference = match std::fs::read("tests/fixtures/test-firmware-signed.bin") {
            Ok(data) => data,
            Err(_) => {
                eprintln!(
                    "Skipping: tests/fixtures/test-firmware-signed.bin not found (generate with espsecure.py)"
                );
                return;
            }
        };
        verify_firmware(&reference).unwrap();
    }

    #[test]
    fn roundtrip_against_espsecure_input() {
        let (firmware, reference, pem) = match (|| -> Option<_> {
            let firmware = std::fs::read("tests/fixtures/test-firmware.bin").ok()?;
            let reference = std::fs::read("tests/fixtures/test-firmware-signed.bin").ok()?;
            let pem = std::fs::read("tests/fixtures/test-signing-key.pem").ok()?;
            Some((firmware, reference, pem))
        })() {
            Some(v) => v,
            None => {
                eprintln!("Skipping: ../tmp/test-firmware*.bin or test-signing-key.pem not found");
                return;
            }
        };

        let signed = sign_firmware(&firmware, &pem, &mut rand::thread_rng()).unwrap();
        assert_eq!(signed.len(), reference.len(), "output size mismatch");

        let our_block = &signed[signed.len() - SECTOR_SIZE..];
        let ref_block = &reference[reference.len() - SECTOR_SIZE..];

        assert_eq!(&our_block[0..4], &ref_block[0..4]);
        assert_eq!(&our_block[4..36], &ref_block[4..36]);
        assert_eq!(&our_block[36..812], &ref_block[36..812]);
        assert_eq!(&our_block[1200..], &ref_block[1200..]);

        verify_firmware(&signed).unwrap();
    }
}
