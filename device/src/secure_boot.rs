extern crate alloc;
use crate::partitions::EspFlashPartition;
use alloc::boxed::Box;
use alloc::{vec, vec::Vec};
use crc::Crc;
use embedded_storage::nor_flash::NorFlashErrorKind;
use esp_hal::efuse::Efuse;
use esp_hal::rsa::{operand_sizes::Op3072, Rsa, RsaModularExponentiation};
use esp_hal::sha::{Sha, Sha256};
use esp_hal::Blocking;

use nb::block;

const SECTOR_SIZE: usize = 4096;

#[derive(Debug)]
pub enum SecureBootError<'a> {
    MissingSignature,
    ChecksumInvalid,
    EfuseError,
    PublicKeyInvalid,
    ReadImageError(u32, NorFlashErrorKind),
    ImageHashInvalid,
    SignatureInvalid,
    SignatureError(&'a str),
}

#[derive(Debug)]
struct SignatureBlock {
    image_digest: [u8; 32],
    rsa_public_modulus: [u8; 384],
    rsa_public_exponent: [u8; 4],
    precalculated_r: [u8; 384],
    precalculated_m_prime: [u8; 4],
    rsa_pss_signature: [u8; 384],
    crc32: [u8; 4],
}

impl SignatureBlock {
    fn from_bytes(data: &[u8; SECTOR_SIZE]) -> Self {
        let mut block = SignatureBlock {
            image_digest: [0; 32],
            rsa_public_modulus: [0; 384],
            rsa_public_exponent: [0; 4],
            precalculated_r: [0; 384],
            precalculated_m_prime: [0; 4],
            rsa_pss_signature: [0; 384],
            crc32: [0; 4],
        };

        block.image_digest.copy_from_slice(&data[4..36]);
        block.rsa_public_modulus.copy_from_slice(&data[36..420]);
        block.rsa_public_exponent.copy_from_slice(&data[420..424]);
        block.precalculated_r.copy_from_slice(&data[424..808]);
        block.precalculated_m_prime.copy_from_slice(&data[808..812]);
        block.rsa_pss_signature.copy_from_slice(&data[812..1196]);
        block.crc32.copy_from_slice(&data[1196..1200]);

        block
    }
}

// Convert bytes to u32 array in little-endian format (ESP32-C3 native format)
fn bytes_to_u32_le_native(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(word)
        })
        .collect()
}

fn verify_rsa_pss_signature(
    rsa: &mut Rsa<'_, Blocking>,
    sig_block: &SignatureBlock,
    message_hash: &[u8; 32],
    sha: &mut Sha,
) -> Result<bool, &'static str> {
    const KEY_SIZE_BYTES: usize = 384;

    // Wait for RSA peripheral to be ready
    block!(rsa.ready()).map_err(|_| "RSA peripheral not ready")?;

    // Convert signature block data to u32 arrays in ESP32-C3 native little-endian format
    // All inputs should be little-endian byte arrays
    let modulus_u32 = bytes_to_u32_le_native(&sig_block.rsa_public_modulus);
    let exponent_u32 = bytes_to_u32_le_native(&sig_block.rsa_public_exponent);
    let r_u32 = bytes_to_u32_le_native(&sig_block.precalculated_r);
    // No byte reversal for signature - use native little-endian format
    let signature_u32 = bytes_to_u32_le_native(&sig_block.rsa_pss_signature);
    let m_prime = u32::from_le_bytes(sig_block.precalculated_m_prime);

    // Convert to fixed-size arrays for Op3072 (96 u32 words = 384 bytes = 3072 bits)
    if modulus_u32.len() != 96 || r_u32.len() != 96 || signature_u32.len() != 96 {
        return Err("Invalid RSA key size - expected 3072 bits");
    }

    let mut modulus = [0u32; 96];
    let mut r_value = [0u32; 96];
    let mut signature = [0u32; 96];
    let mut exponent = [0u32; 96];

    modulus.copy_from_slice(&modulus_u32);
    r_value.copy_from_slice(&r_u32);
    signature.copy_from_slice(&signature_u32);
    exponent[0] = exponent_u32[0]; // Exponent is typically small, just copy first word

    // Create modular exponentiation context
    let mut mod_exp = RsaModularExponentiation::<Op3072, _>::new(rsa, &exponent, &modulus, m_prime);

    // Start the RSA operation: signature^exponent mod modulus
    mod_exp.start_exponentiation(&signature, &r_value);

    // Read results - no async waiting needed
    let mut decrypted = [0u32; 96];
    mod_exp.read_results(&mut decrypted);

    // Convert ESP32-C3 RSA result to PSS verification format
    // ESP32-C3 returns little-endian words, PSS expects big-endian byte order
    let mut decrypted_bytes = vec![0u8; KEY_SIZE_BYTES];
    for (i, &word) in decrypted.iter().enumerate() {
        let bytes = word.to_le_bytes();
        decrypted_bytes[i * 4..(i + 1) * 4].copy_from_slice(&bytes);
    }
    decrypted_bytes.reverse(); // Reverse entire array to get correct PSS format
    let decrypted_bytes: [u8; KEY_SIZE_BYTES] = decrypted_bytes.try_into().unwrap();

    // Verify PSS padding manually
    Ok(verify_pss_padding(&decrypted_bytes, message_hash, sha))
}

fn verify_pss_padding(decrypted: &[u8], message_hash: &[u8], sha: &mut Sha) -> bool {
    const SALT_LEN: usize = 32; // PSS salt length for ESP32 (confirmed from ESP-IDF research)
    const HASH_LEN: usize = 32; // SHA-256 hash length
    const KEY_SIZE_BYTES: usize = 384;

    if decrypted.len() != KEY_SIZE_BYTES {
        return false;
    }

    // Check trailer field (last byte should be 0xBC)
    if decrypted[KEY_SIZE_BYTES - 1] != 0xbc {
        return false;
    }

    // Extract mHash (H) from the end before trailer
    let em_hash = &decrypted[KEY_SIZE_BYTES - HASH_LEN - 1..KEY_SIZE_BYTES - 1];

    // Extract maskedDB
    let masked_db_len = KEY_SIZE_BYTES - HASH_LEN - 1;
    let masked_db = &decrypted[..masked_db_len];

    // Generate mask using MGF1
    let db_mask = mgf1(em_hash, masked_db_len, sha);

    // Unmask DB: DB = maskedDB XOR dbMask
    let mut db = vec![0u8; masked_db_len];
    for i in 0..masked_db_len {
        db[i] = masked_db[i] ^ db_mask[i];
    }

    // Clear the leftmost bits (since emBits might not be a multiple of 8)
    let em_bits = KEY_SIZE_BYTES * 8 - 1; // emLen * 8 - 1 for PSS
    let bits_to_clear = 8 - (em_bits % 8);
    if bits_to_clear < 8 {
        db[0] &= 0xff >> bits_to_clear;
    }

    // Check that DB starts with zeros followed by 0x01
    let ps_len = masked_db_len - SALT_LEN - 1;
    for byte in db.iter().take(ps_len) {
        if *byte != 0x00 {
            return false;
        }
    }

    if db[ps_len] != 0x01 {
        return false;
    }

    // Extract salt
    let salt = &db[ps_len + 1..];
    if salt.len() != SALT_LEN {
        return false;
    }

    // Reconstruct M' = 0x00 00 00 00 00 00 00 00 || mHash || salt
    // Not to be confused with M' from signature block
    let mut m_prime = vec![0u8; 8 + HASH_LEN + SALT_LEN];
    m_prime[8..8 + HASH_LEN].copy_from_slice(message_hash);
    m_prime[8 + HASH_LEN..].copy_from_slice(salt);

    // Compute H' = Hash(M') using hardware SHA peripheral
    let h_prime = compute_sha256_hardware(sha, &m_prime);

    // Verify H == H'
    em_hash == h_prime.as_slice()
}

fn mgf1(seed: &[u8], mask_len: usize, sha: &mut Sha) -> Vec<u8> {
    let mut mask = Vec::new();
    let mut counter = 0u32;

    while mask.len() < mask_len {
        let mut input = Vec::new();
        input.extend_from_slice(seed);
        input.extend_from_slice(&counter.to_be_bytes());
        let hash = compute_sha256_hardware(sha, &input);
        mask.extend_from_slice(&hash);
        counter += 1;
    }

    mask.truncate(mask_len);
    mask
}

// Helper function to compute SHA256 using hardware peripheral
fn compute_sha256_hardware(sha: &mut Sha, data: &[u8]) -> [u8; 32] {
    let mut hasher = sha.start::<Sha256>();

    let mut remaining = data;
    while !remaining.is_empty() {
        remaining = nb::block!(hasher.update(remaining)).unwrap();
    }

    let mut result = [0u8; 32];
    nb::block!(hasher.finish(&mut result)).unwrap();
    result
}

// Find secure boot key digest from eFuse by checking KEY_PURPOSE fields
fn find_secure_boot_key() -> Option<[u8; 32]> {
    use esp_hal::efuse::{
        KEY0, KEY1, KEY2, KEY3, KEY4, KEY5, KEY_PURPOSE_0, KEY_PURPOSE_1, KEY_PURPOSE_2,
        KEY_PURPOSE_3, KEY_PURPOSE_4, KEY_PURPOSE_5, SECURE_BOOT_KEY_REVOKE0,
        SECURE_BOOT_KEY_REVOKE1, SECURE_BOOT_KEY_REVOKE2,
    };
    // Key purpose values and their revoke fields (from ESP32-C3 TRM Table 4.3-1 & 4.3-2)
    let secure_boot_digests = [
        (9, SECURE_BOOT_KEY_REVOKE0),  // DIGEST0
        (10, SECURE_BOOT_KEY_REVOKE1), // DIGEST1
        (11, SECURE_BOOT_KEY_REVOKE2), // DIGEST2
    ];

    let key_purpose_fields = [
        KEY_PURPOSE_0,
        KEY_PURPOSE_1,
        KEY_PURPOSE_2,
        KEY_PURPOSE_3,
        KEY_PURPOSE_4,
        KEY_PURPOSE_5,
    ];
    let key_data_fields = [KEY0, KEY1, KEY2, KEY3, KEY4, KEY5];

    // Search through all key blocks
    for (i, &purpose_field) in key_purpose_fields.iter().enumerate() {
        let purpose: u8 = Efuse::read_field_le(purpose_field);

        // Find matching secure boot digest revoke field
        if let Some((_, revoke_field)) = secure_boot_digests
            .iter()
            .find(|(purpose_val, _)| *purpose_val == purpose)
        {
            // Check if this key is revoked
            let is_revoked = Efuse::read_bit(*revoke_field);
            if !is_revoked {
                // Read the key data (32 bytes)
                let key_data: [u8; 32] = Efuse::read_field_le(key_data_fields[i]);
                return Some(key_data);
            }
        }
    }
    None
}

/// Check if secure boot is enabled by looking for secure boot key digests in eFuse
pub fn is_secure_boot_enabled() -> bool {
    find_secure_boot_key().is_some()
}

pub fn find_signature_sector(partition: &EspFlashPartition) -> Option<u32> {
    // Search backwards from the end - signature blocks are typically at the end of firmware
    for i in (0..partition.n_sectors()).rev() {
        match partition.read_sector(i) {
            Ok(sector_data) => {
                // Check for signature block magic bytes: 0xE7, 0x02, 0x00, 0x00
                if sector_data.len() >= 4 && sector_data[0..4] == [0xE7, 0x02, 0x00, 0x00] {
                    return Some(i);
                }
            }
            Err(_) => {
                // Skip unreadable sectors
                continue;
            }
        }
    }
    None // No signature block found
}

fn read_signature_sector(partition: &EspFlashPartition) -> Option<(u32, Box<[u8; SECTOR_SIZE]>)> {
    let sector_idx = find_signature_sector(partition)?;
    let sector_data = partition.read_sector(sector_idx).ok()?;
    Some((sector_idx, sector_data))
}

pub fn verify_secure_boot<'a>(
    app_partition: &EspFlashPartition,
    rsa: &mut Rsa<'_, Blocking>,
    sha: &mut Sha,
) -> Result<(), SecureBootError<'a>> {
    let (signature_sector_index, signature_block) =
        read_signature_sector(app_partition).ok_or(SecureBootError::MissingSignature)?;

    // Parse signature block structure
    let parsed_block = SignatureBlock::from_bytes(&signature_block);

    // Verify CRC32 checksum
    const CRC: Crc<u32> = Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    // CRC32 is calculated over first 1196 bytes
    let calculated_crc = CRC.checksum(&signature_block[0..1196]);
    let stored_crc = u32::from_le_bytes(parsed_block.crc32);
    if calculated_crc != stored_crc {
        return Err(SecureBootError::ChecksumInvalid);
    }

    // Verify public key digest against eFuse FIRST
    // Find the secure boot key digest from eFuse by checking KEY_PURPOSE fields
    let efuse_key_digest = match find_secure_boot_key() {
        Some(key_digest) => key_digest,
        None => return Err(SecureBootError::EfuseError),
    };

    // Calculate SHA-256 of public key material from signature block (bytes 36-812)
    // This includes: RSA modulus (36-420) + exponent (420-424) + pre-calculated R (424-812)
    let public_key_data = &signature_block[36..812]; // 776 bytes total

    let calculated_key_digest = compute_sha256_hardware(sha, public_key_data);

    if calculated_key_digest != efuse_key_digest {
        return Err(SecureBootError::PublicKeyInvalid);
    }

    // Verify image digest (SHA-256 of application data before signature block)
    // Calculate how many sectors contain application data (before signature block)
    let mut hasher = sha.start::<Sha256>();
    for sector in 0..signature_sector_index {
        match app_partition.read_sector(sector) {
            Ok(sector_data) => {
                let mut remaining = sector_data.as_slice();
                while !remaining.is_empty() {
                    remaining = block!(hasher.update(remaining)).unwrap();
                }
            }
            Err(e) => {
                // "Failed to read flash sector {} for image digest verification: {:?}", sector, e
                return Err(SecureBootError::ReadImageError(sector, e));
            }
        }
    }

    let mut calculated_digest = [0u8; 32];
    block!(hasher.finish(&mut calculated_digest)).unwrap();

    if calculated_digest != parsed_block.image_digest {
        return Err(SecureBootError::ImageHashInvalid);
    }

    // Verify RSA-PSS signature using hardware RSA peripheral
    match verify_rsa_pss_signature(rsa, &parsed_block, &parsed_block.image_digest, sha) {
        Ok(true) => { /* Firmware signature verified successfully */ }
        Ok(false) => return Err(SecureBootError::SignatureInvalid),
        Err(e) => return Err(SecureBootError::SignatureError(e)),
    }

    // If we reach here, ALL security checks have passed
    Ok(())
}
