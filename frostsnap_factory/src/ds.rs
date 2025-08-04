use aes::cipher::BlockEncryptMut;
use aes::cipher::{block_padding::NoPadding, KeyIvInit};
use aes::Aes256;
use cbc::Encryptor;
use frostsnap_comms::factory::{pad_message_for_rsa, Esp32DsKey, DS_KEY_SIZE_BITS};
use hmac::{Hmac, Mac};
use num_traits::ToPrimitive;
use num_traits::{One, Zero};
use rand::{CryptoRng, RngCore};
use rsa::traits::PublicKeyParts as _;
use rsa::BigUint;
use rsa::{traits::PrivateKeyParts as _, RsaPrivateKey};
use sha2::{Digest, Sha256};
use std::error::Error;

use std::fmt;

const DS_NUM_WORDS: usize = DS_KEY_SIZE_BITS / 32;

pub fn standard_rsa_sign(priv_key: &RsaPrivateKey, message: &[u8]) -> Vec<u8> {
    let message_digest: [u8; 32] = sha2::Sha256::digest(message).into();
    let padded_message = pad_message_for_rsa(&message_digest);

    raw_exponent_rsa_sign(padded_message.into(), priv_key)
}

fn raw_exponent_rsa_sign(padded_int: Vec<u8>, private_key: &RsaPrivateKey) -> Vec<u8> {
    let d = BigUint::from_bytes_be(&private_key.d().to_bytes_be());
    let n = BigUint::from_bytes_be(&private_key.n().to_bytes_be());
    let challenge_uint = BigUint::from_bytes_be(&padded_int);
    let signature_int = challenge_uint.modpow(&d, &n);

    signature_int.to_bytes_be()
}

pub fn esp32_ds_key_from_keys(priv_key: &RsaPrivateKey, hmac_key: [u8; 32]) -> Esp32DsKey {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(&hmac_key[..]).expect("HMAC can take key of any size");
    mac.update([0xffu8; 32].as_slice());
    let aes_key: [u8; 32] = mac.finalize().into_bytes().into();
    let iv = [
        0xb8, 0xb4, 0x69, 0x18, 0x28, 0xa3, 0x91, 0xd9, 0xd6, 0x62, 0x85, 0x8c, 0xc9, 0x79, 0x48,
        0x86,
    ];

    let plaintext_data = EspDsPData::new(priv_key).unwrap();
    let encrypted_params =
        encrypt_private_key_material(&plaintext_data, &aes_key[..], &iv[..]).unwrap();

    Esp32DsKey {
        encrypted_params,
        ds_hmac_key: hmac_key,
    }
}

pub fn generate(rng: &mut (impl RngCore + CryptoRng)) -> (RsaPrivateKey, [u8; 32]) {
    let priv_key = RsaPrivateKey::new(rng, DS_KEY_SIZE_BITS).unwrap();

    let mut hmac_key = [42u8; 32];
    rng.fill_bytes(&mut hmac_key);

    (priv_key, hmac_key)
}

#[repr(C)]
pub struct EspDsPData {
    pub y: [u32; DS_NUM_WORDS],  // RSA exponent (private exponent)
    pub m: [u32; DS_NUM_WORDS],  // RSA modulus
    pub rb: [u32; DS_NUM_WORDS], // Montgomery R inverse operand: (1 << (DS_KEY_SIZE_BITS*2)) % M
    pub m_prime: u32,            // - modinv(M mod 2^32, 2^32) mod 2^32
    pub length: u32,             // effective length: (DS_KEY_SIZE_BITS/32) - 1
}

impl EspDsPData {
    /// Constructs EspDsPData from an RSA private key.
    ///
    /// Uses the following Python formulas:
    ///
    ///   rr = 1 << (key_size * 2)
    ///   rinv = rr % pub_numbers.n
    ///   mprime = - modinv(M, 1 << 32) & 0xFFFFFFFF
    ///   length = key_size // 32 - 1
    ///
    /// In this implementation we assume DS_KEY_SIZE_BITS is the intended bit size.
    /// Y is taken as the private exponent and M as the modulus.
    pub fn new(rsa_private: &RsaPrivateKey) -> Result<Self, Box<dyn Error>> {
        // Get the private exponent (d) and modulus (n) as BigUint.
        let y_big = rsa_private.d();
        let m_big = rsa_private.n();

        // Convert Y and M into vectors of u32 words (little-endian).
        let y_vec = big_number_to_words(y_big);
        let m_vec = big_number_to_words(m_big);

        // Use the fixed DS_KEY_SIZE_BITS to compute the effective length.
        // For example, if DS_KEY_SIZE_BITS is 3072 then length = 3072/32 - 1 = 96 - 1 = 95.
        let length = (DS_KEY_SIZE_BITS / 32 - 1) as u32;

        // Convert the vectors into fixed-length arrays.
        let y_arr = vec_to_fixed(&y_vec, DS_NUM_WORDS);
        let m_arr = vec_to_fixed(&m_vec, DS_NUM_WORDS);

        // Compute m_prime = - modinv(M mod 2^32, 2^32) & 0xFFFFFFFF.
        let n0 = (m_big & BigUint::from(0xffffffffu32))
            .to_u32()
            .ok_or("Failed to convert modulus remainder to u32")?;
        let inv_n0 = modinv_u32(n0).ok_or("Failed to compute modular inverse for m_prime")?;
        let m_prime = (!inv_n0).wrapping_add(1);

        // Compute Montgomery value as per Python:
        // rr = 1 << (DS_KEY_SIZE_BITS * 2)
        // rb = rr % M
        let rr = BigUint::one() << (DS_KEY_SIZE_BITS * 2);
        let rb_big = &rr % m_big;
        let rb_vec = big_number_to_words(&rb_big);
        let rb_arr = vec_to_fixed(&rb_vec, DS_NUM_WORDS);

        Ok(EspDsPData {
            y: y_arr,
            m: m_arr,
            rb: rb_arr,
            m_prime,
            length,
        })
    }
}

/// Converts a BigUint into a Vec<u32> in little-endian order,
/// stopping when the number becomes zero.
fn big_number_to_words(num: &BigUint) -> Vec<u32> {
    let mut vec = Vec::new();
    let mut n = num.clone();
    let mask = BigUint::from(0xffffffffu32);
    while n > BigUint::zero() {
        let word = (&n & &mask).to_u32().unwrap();
        vec.push(word);
        n >>= 32;
    }
    if vec.is_empty() {
        vec.push(0);
    }
    vec
}

/// Copies a vector of u32 into a fixed-length array, padding with zeros.
fn vec_to_fixed(vec: &[u32], fixed_len: usize) -> [u32; DS_NUM_WORDS] {
    let mut arr = [0u32; DS_NUM_WORDS];
    for (i, &word) in vec.iter().enumerate().take(fixed_len) {
        arr[i] = word;
    }
    arr
}

/// Computes the modular inverse of a modulo 2^32, assuming a is odd.
fn modinv_u32(a: u32) -> Option<u32> {
    let modulus: i64 = 1i64 << 32;
    let mut r: i64 = modulus;
    let mut new_r: i64 = a as i64;
    let mut t: i64 = 0;
    let mut new_t: i64 = 1;

    while new_r != 0 {
        let quotient = r / new_r;
        let temp_t = t - quotient * new_t;
        t = new_t;
        new_t = temp_t;
        let temp_r = r - quotient * new_r;
        r = new_r;
        new_r = temp_r;
    }
    if r > 1 {
        return None;
    }
    if t < 0 {
        t += modulus;
    }
    Some(t as u32)
}

/// Custom Debug implementation that prints u32 arrays in "0x%08x" format.
impl fmt::Debug for EspDsPData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn format_array(arr: &[u32; DS_NUM_WORDS]) -> String {
            let formatted: Vec<String> = arr.iter().map(|word| format!("0x{word:08x}")).collect();
            format!("{{ {} }}", formatted.join(", "))
        }

        writeln!(f, "EspDsPData {{")?;
        writeln!(f, "    y: {}", format_array(&self.y))?;
        writeln!(f, "    m: {}", format_array(&self.m))?;
        writeln!(f, "    rb: {}", format_array(&self.rb))?;
        writeln!(f, "    m_prime: 0x{:08x}", self.m_prime)?;
        writeln!(f, "    length: {}", self.length)?;
        write!(f, "}}")
    }
}

/// Encrypts the private key material following the ESP32-C3 DS scheme without extra padding.
///
/// It constructs:
///
///   md_in = number_as_bytes(Y, max_key_size)
///         || number_as_bytes(M, max_key_size)
///         || number_as_bytes(Rb, max_key_size)
///         || pack::<LittleEndian>(m_prime, length)
///         || iv
///
///   md = SHA256(md_in)
///
///   p = number_as_bytes(Y, max_key_size)
///         || number_as_bytes(M, max_key_size)
///         || number_as_bytes(Rb, max_key_size)
///         || md
///         || pack::<LittleEndian>(m_prime, length)
///         || [0x08; 8]
///
/// where max_key_size = DS_KEY_SIZE_BITS/8. Then p is encrypted using AES-256 in CBC mode with no padding.
/// (Note: p must be block-aligned; for example, for a 3072-bit key, p ends up being 1200 bytes, which is
/// a multiple of 16.)
pub fn encrypt_private_key_material(
    ds_data: &EspDsPData,
    aes_key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    // For a fixed RSA key size (e.g., 3072 bits), max_key_size is:
    let max_key_size = DS_KEY_SIZE_BITS / 8; // e.g., 3072/8 = 384 bytes

    // Convert each of Y, M, and Rb into fixed-length little-endian byte arrays.
    let y_bytes = number_as_bytes(&ds_data.y, max_key_size);
    let m_bytes = number_as_bytes(&ds_data.m, max_key_size);
    let rb_bytes = number_as_bytes(&ds_data.rb, max_key_size);

    // Pack m_prime and length as little-endian u32 values.
    let mut mprime_length = Vec::new();
    mprime_length.extend_from_slice(&ds_data.m_prime.to_le_bytes());
    mprime_length.extend_from_slice(&ds_data.length.to_le_bytes());

    // Construct md_in = Y || M || Rb || (m_prime||length) || IV.
    let mut md_in = Vec::new();
    md_in.extend_from_slice(&y_bytes);
    md_in.extend_from_slice(&m_bytes);
    md_in.extend_from_slice(&rb_bytes);
    md_in.extend_from_slice(&mprime_length);
    md_in.extend_from_slice(iv);

    // Compute SHA256 digest of md_in.
    let md = Sha256::digest(&md_in); // 32 bytes

    // Construct p = Y || M || Rb || md || (m_prime||length) || 8 bytes of 0x08.
    let mut p = Vec::new();
    p.extend_from_slice(&y_bytes);
    p.extend_from_slice(&m_bytes);
    p.extend_from_slice(&rb_bytes);
    p.extend_from_slice(&md);
    p.extend_from_slice(&mprime_length);
    p.extend_from_slice(&[0x08u8; 8]);

    // Verify that p is the expected length:
    // expected_len = (max_key_size * 3) + 32 + 8 + 8.
    let expected_len = (max_key_size * 3) + 32 + 8 + 8;
    assert_eq!(
        p.len(),
        expected_len,
        "P length mismatch: got {}, expected {}",
        p.len(),
        expected_len
    );

    // Allocate an output buffer exactly the same size as p.
    let mut out_buf = vec![0u8; p.len()];

    // Encrypt p using AES-256 in CBC mode with no padding.
    type Aes256CbcEnc = Encryptor<Aes256>;
    let ct = Aes256CbcEnc::new(aes_key.into(), iv.into())
        .encrypt_padded_b2b_mut::<NoPadding>(&p, &mut out_buf)
        .map_err(|e| format!("Encryption error: {e:?}"))?;

    let iv_and_ct = [iv, ct].concat();
    Ok(iv_and_ct)
}

/// Converts a fixed-length u32 array (representing a big number in little-endian order)
/// into a byte vector of exactly `max_bytes` length. Each u32 is converted using `to_le_bytes()`,
/// then the vector is truncated or padded with zeros to exactly max_bytes.
fn number_as_bytes(arr: &[u32; DS_NUM_WORDS], max_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DS_NUM_WORDS * 4);
    for &word in arr.iter() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    if bytes.len() > max_bytes {
        bytes.truncate(max_bytes);
    } else {
        while bytes.len() < max_bytes {
            bytes.push(0);
        }
    }
    bytes
}
