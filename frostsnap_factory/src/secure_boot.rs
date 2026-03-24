use crc::Crc;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::pss::SigningKey;
use rsa::signature::{hazmat::RandomizedPrehashSigner, SignatureEncoding};
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey};
use sha2::{Digest, Sha256};

const SECTOR_SIZE: usize = 4096;
const SIGNATURE_BLOCK_MAGIC: [u8; 4] = [0xE7, 0x02, 0x00, 0x00];
const CRC: Crc<u32> = Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
const RSA_KEY_BYTES: usize = 384;

pub fn sign_firmware(
    firmware: &[u8],
    pem_key: &[u8],
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

    let sig_block = build_signature_block(&private_key, &image_digest)?;
    signed.extend_from_slice(&sig_block);

    Ok(signed)
}

/// Verify that a signed firmware binary has a valid Secure Boot v2 signature.
///
/// Checks the signature block magic, CRC32, image digest, Montgomery constants,
/// and RSA-PSS signature. Returns the embedded RSA public key on success.
pub fn verify_firmware(signed_firmware: &[u8]) -> Result<rsa::RsaPublicKey, VerifyError> {
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

    let expected_digest: [u8; 32] = Sha256::digest(firmware).into();
    if block[4..36] != expected_digest {
        return Err(VerifyError::DigestMismatch);
    }

    let modulus = BigUint::from_bytes_le(&block[36..420]);
    let exponent = BigUint::from_bytes_le(&block[420..424]);
    let public_key = rsa::RsaPublicKey::new(modulus.clone(), exponent)
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
        .verify_prehash(&expected_digest, &signature)
        .map_err(|e| VerifyError::SignatureInvalid(e.to_string()))?;

    Ok(public_key)
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

fn build_signature_block(
    private_key: &RsaPrivateKey,
    image_digest: &[u8; 32],
) -> Result<[u8; SECTOR_SIZE], Box<dyn std::error::Error>> {
    let public_key = private_key.to_public_key();
    let n = public_key.n();
    let e = public_key.e();

    let modulus_le = to_le_bytes_fixed::<RSA_KEY_BYTES>(n);
    let exponent_le = to_le_bytes_fixed::<4>(e);

    // Montgomery R = (2^3072)^2 mod N
    let two_pow_3072 = BigUint::from(1u32) << (RSA_KEY_BYTES * 8);
    let r = (&two_pow_3072 * &two_pow_3072) % n;
    let r_le = to_le_bytes_fixed::<RSA_KEY_BYTES>(&r);

    // Montgomery M' = -N^(-1) mod 2^32
    // N is odd (product of primes), so N^(-1) mod 2^32 always exists.
    // Compute via Newton's method: x = x * (2 - n*x) mod 2^32
    let n_low = n.to_bytes_le();
    let n32 = u32::from_le_bytes([n_low[0], n_low[1], n_low[2], n_low[3]]);
    let mut x: u32 = 1;
    for _ in 0..5 {
        x = x.wrapping_mul(2u32.wrapping_sub(n32.wrapping_mul(x)));
    }
    let m_prime = (0u32).wrapping_sub(x); // -N^(-1) mod 2^32
    let m_prime_le = m_prime.to_le_bytes();

    let signing_key = SigningKey::<Sha256>::new(private_key.clone());
    let mut rng = rand::thread_rng();
    let signature = signing_key.sign_prehash_with_rng(&mut rng, image_digest)?;
    let sig_bytes = signature.to_vec();

    let mut sig_le = [0u8; RSA_KEY_BYTES];
    sig_le.copy_from_slice(&sig_bytes);
    sig_le.reverse();

    // Build the block
    let mut block = [0u8; SECTOR_SIZE];
    block[0..4].copy_from_slice(&SIGNATURE_BLOCK_MAGIC);
    block[4..36].copy_from_slice(image_digest);
    block[36..420].copy_from_slice(&modulus_le);
    block[420..424].copy_from_slice(&exponent_le);
    block[424..808].copy_from_slice(&r_le);
    block[808..812].copy_from_slice(&m_prime_le);
    block[812..1196].copy_from_slice(&sig_le);

    // CRC32 over bytes 0..1196
    let crc_val = CRC.checksum(&block[0..1196]);
    block[1196..1200].copy_from_slice(&crc_val.to_le_bytes());

    // bytes 1200..1216 stay 0x00, rest is 0xFF
    block[1216..].fill(0xFF);

    Ok(block)
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
        let signed = sign_firmware(&firmware, &TEST_KEY_PEM).unwrap();
        assert_eq!(signed.len(), 4096 * 5);
        verify_firmware(&signed).unwrap();
    }

    #[test]
    fn sign_and_verify_unaligned() {
        let firmware = vec![0xCDu8; 5000];
        let signed = sign_firmware(&firmware, &TEST_KEY_PEM).unwrap();
        assert_eq!(signed.len(), 12288);
        assert!(signed[5000..8192].iter().all(|&b| b == 0xFF));
        verify_firmware(&signed).unwrap();
    }

    #[test]
    fn matches_espsecure_format() {
        let reference = match std::fs::read("tests/fixtures/test-firmware-signed.bin") {
            Ok(data) => data,
            Err(_) => {
                eprintln!("Skipping: tests/fixtures/test-firmware-signed.bin not found (generate with espsecure.py)");
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

        let signed = sign_firmware(&firmware, &pem).unwrap();
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
