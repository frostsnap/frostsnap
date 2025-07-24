use aes::cipher::BlockEncryptMut as _;
use clap::Parser;
use core::fmt;
use frostsnap_comms::factory::{
    pad_message_for_rsa, Certificate, DeviceFactorySend, Esp32DsKey, FactoryDownstream,
    FactorySend, DS_KEY_SIZE_BITS,
};
use frostsnap_comms::{ReceiveSerial, MAGIC_BYTES_PERIOD};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::marker::{NonZero, Public, Secret};
use frostsnap_core::schnorr_fun::fun::{hex, KeyPair, Scalar};
use frostsnap_core::schnorr_fun::Message;
use hmac::{Hmac, Mac};
use num_bigint::BigUint;
use num_bigint_dig as num_bigint;
use num_traits::identities::{One, Zero};
use num_traits::ToPrimitive;
use rand::{CryptoRng, RngCore};
use rsa::pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey};
use rsa::traits::PublicKeyParts as _;
use rsa::{traits::PrivateKeyParts as _, RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::*;

pub mod serial_number;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // /// Name of the person to greet
    // #[arg(short, long)]
    // name: String,

    // /// Number of times to greet
    // #[arg(short, long, default_value_t = 1)]
    // count: u8,
}

const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;

const DS_NUM_WORDS: usize = DS_KEY_SIZE_BITS / 32;
const DS_CHALLENGE: &str = "354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b";

const FACTORY_KEY: [u8; 32] = [
    0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

fn main() -> ! {
    tracing_subscriber::fmt().pretty().init();

    // let args = Args::parse();

    let serial = DesktopSerial::default();
    let mut rng = rand::thread_rng();

    let mut connection_state = HashMap::new();
    loop {
        for port_desc in serial.available_ports() {
            if !connection_state.contains_key(&port_desc.id) {
                if port_desc.vid == USB_VID && port_desc.pid == USB_PID {
                    let port = serial
                        .open_device_port(&port_desc.id, 14_000 /* doesn't matter*/)
                        .map(FramedSerialPort::<FactoryDownstream>::new);

                    match port {
                        Ok(port) => {
                            connection_state.insert(
                                port_desc.id,
                                Connection {
                                    state: ConnectionState::WaitingForMagic { last_wrote: None },
                                    port,
                                },
                            );
                        }
                        Err(e) => {
                            error!(
                                port = port_desc.id.to_string(),
                                error = e.to_string(),
                                "unable to open port"
                            );
                        }
                    }
                }
            }
        }

        for (port_id, connection) in connection_state.iter_mut() {
            info_span!("polling port", port = port_id.to_string()).in_scope(|| {
                match &connection.state {
                    ConnectionState::WaitingForMagic { last_wrote } => {
                        match connection.port.read_for_magic_bytes() {
                            Ok(supported_features) => match supported_features {
                                Some(_) => {
                                    connection.state = ConnectionState::WaitingForState;
                                }
                                None => {
                                    if last_wrote.is_none()
                                        || last_wrote.as_ref().unwrap().elapsed().as_millis() as u64
                                            > MAGIC_BYTES_PERIOD
                                    {
                                        connection.state = ConnectionState::WaitingForMagic {
                                            last_wrote: Some(std::time::Instant::now()),
                                        };
                                        let _ =
                                            connection.port.write_magic_bytes().inspect_err(|e| {
                                                error!(
                                                    error = e.to_string(),
                                                    "failed to write magic bytes"
                                                );
                                            });
                                    }
                                }
                            },
                            Err(e) => {
                                error!(error = e.to_string(), "failed to read magic bytes")
                            }
                        }
                    }
                    ConnectionState::WaitingForState => {
                        // We leave it up to the device to decide if it is configured looking for
                        // efuses and rsa key, then skip to the genuine check.
                        if let Some(ReceiveSerial::Message(DeviceFactorySend::SendState {
                            rsa_pub_key,
                        })) = connection.port.try_read_message().unwrap()
                        {
                            if let Some(rsa_pub_key_bytes) = rsa_pub_key {
                                println!("Device already has certificate!");
                                let rsa_pub_key =
                                    rsa::RsaPublicKey::from_pkcs1_der(&rsa_pub_key_bytes).unwrap();
                                connection.state =
                                    ConnectionState::SavingGenuineCertificate { rsa_pub_key }
                            } else {
                                connection.state = ConnectionState::BeginInitEntropy;
                            }
                        };
                    }
                    ConnectionState::BeginInitEntropy => {
                        let mut bytes = [0u8; 32];
                        rand::thread_rng().fill_bytes(&mut bytes);
                        connection
                            .port
                            .raw_send(ReceiveSerial::Message(FactorySend::InitEntropy(bytes)))
                            .unwrap();
                        connection.state = ConnectionState::InitEntropy;
                    }
                    ConnectionState::InitEntropy => {
                        if let Some(ReceiveSerial::Message(DeviceFactorySend::InitEntropyOk)) =
                            connection.port.try_read_message().unwrap()
                        {
                            let (rsa_priv_key, hmac_key) = generate_ds_key(&mut rng);
                            let esp32_ds_key = esp32_ds_key_from_keys(&rsa_priv_key, hmac_key);
                            let rsa_pub_key = rsa_priv_key.to_public_key();

                            connection
                                .port
                                .raw_send(ReceiveSerial::Message(FactorySend::SetEsp32DsKey(
                                    esp32_ds_key,
                                )))
                                .unwrap();

                            connection.state = ConnectionState::SettingDsKey { rsa_pub_key };
                        }
                    }
                    ConnectionState::SettingDsKey { rsa_pub_key } => {
                        if let Some(ReceiveSerial::Message(DeviceFactorySend::ReceivedDsKey)) =
                            connection.port.try_read_message().unwrap()
                        {
                            // create genuine certificate
                            let serial_number = serial_number::get_next_serial_number()
                                .expect("serial number file should exist!");
                            let genuine_certificate = generate_genuine_certificate(
                                &rsa_pub_key,
                                serial_number,
                                "BLACK".to_string(),
                            );
                            connection
                                .port
                                .raw_send(ReceiveSerial::Message(
                                    FactorySend::SetGenuineCertificate(genuine_certificate),
                                ))
                                .unwrap();
                            connection.state = ConnectionState::SavingGenuineCertificate {
                                rsa_pub_key: rsa_pub_key.clone(),
                            };
                        }
                    }
                    ConnectionState::SavingGenuineCertificate { rsa_pub_key } => {
                        if let Some(ReceiveSerial::Message(
                            DeviceFactorySend::PresentGenuineCertificate(certificate),
                        )) = connection.port.try_read_message().unwrap()
                        {
                            // Verify certificate signature with factory key
                            if !verify_certificate_signature(&certificate) {
                                panic!("Certificate signature verification failed!");
                            }

                            // Verify the public key matches what we expect
                            let cert_pub_key = RsaPublicKey::from_pkcs1_der(&certificate.rsa_key)
                                .expect("Invalid RSA key in certificate");
                            if cert_pub_key != *rsa_pub_key {
                                panic!("Certificate public key mismatch!");
                            }

                            // Challenge device to prove it has the private key
                            let challenge = hex::decode(DS_CHALLENGE).unwrap();
                            connection
                                .port
                                .raw_send(ReceiveSerial::Message(FactorySend::Challenge(
                                    challenge.clone(),
                                )))
                                .unwrap();

                            connection.state = ConnectionState::SigningChallenge {
                                rsa_pub_key: cert_pub_key,
                                challenge,
                            };
                        }
                    }
                    ConnectionState::SigningChallenge {
                        rsa_pub_key,
                        challenge,
                    } => {
                        if let Some(ReceiveSerial::Message(DeviceFactorySend::SignedChallenge {
                            signature,
                        })) = connection.port.try_read_message().unwrap()
                        {
                            let message_digest: [u8; 32] = sha2::Sha256::digest(challenge).into();
                            let padding = rsa::Pkcs1v15Sign::new::<Sha256>();
                            let _ = rsa_pub_key
                                .verify(padding, &message_digest, &signature)
                                .expect("Signature from device failed to verify!");

                            println!("Device RSA signature verifcation succeeded!");

                            println!("Factory process complete!");
                        }
                    }
                }
            });
        }
    }
}

pub fn standard_rsa_sign(priv_key: &RsaPrivateKey, message: &[u8]) -> Vec<u8> {
    let message_digest: [u8; 32] = sha2::Sha256::digest(message).into();
    let padded_message = pad_message_for_rsa(&message_digest);
    let sig = raw_exponent_rsa_sign(padded_message.into(), &priv_key);
    sig
}

fn raw_exponent_rsa_sign(padded_int: Vec<u8>, private_key: &RsaPrivateKey) -> Vec<u8> {
    let d = BigUint::from_bytes_be(&private_key.d().to_bytes_be());
    let n = BigUint::from_bytes_be(&private_key.n().to_bytes_be());
    let challenge_uint = BigUint::from_bytes_be(&padded_int);
    let signature_int = challenge_uint.modpow(&d, &n);

    signature_int.to_bytes_be()
}

fn esp32_ds_key_from_keys(priv_key: &RsaPrivateKey, hmac_key: [u8; 32]) -> Esp32DsKey {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(&hmac_key[..]).expect("HMAC can take key of any size");
    mac.update([0xffu8; 32].as_slice());
    let aes_key: [u8; 32] = mac.finalize().into_bytes().into();
    let iv = [
        0xb8, 0xb4, 0x69, 0x18, 0x28, 0xa3, 0x91, 0xd9, 0xd6, 0x62, 0x85, 0x8c, 0xc9, 0x79, 0x48,
        0x86,
    ];

    let plaintext_data = EspDsPData::new(&priv_key).unwrap();
    let encrypted_params =
        encrypt_private_key_material(&plaintext_data, &aes_key[..], &iv[..]).unwrap();

    Esp32DsKey {
        encrypted_params,
        ds_hmac_key: hmac_key,
    }
}

fn generate_ds_key(rng: &mut (impl RngCore + CryptoRng)) -> (RsaPrivateKey, [u8; 32]) {
    let priv_key = RsaPrivateKey::new(rng, DS_KEY_SIZE_BITS).unwrap();

    let mut hmac_key = [42u8; 32];
    // rng.fill_bytes(&mut hmac_key); // TODO: FILL!!

    (priv_key, hmac_key)
}

fn generate_genuine_certificate(
    rsa_public_key: &RsaPublicKey,
    serial_number: u32,
    case_color: String,
) -> Certificate {
    let certificate = {
        let pem_bytes = rsa_public_key.to_pkcs1_der().unwrap().to_vec();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let factory_secret = Scalar::<Secret, NonZero>::from_bytes(FACTORY_KEY).unwrap();
        let factory_keypair = KeyPair::new_xonly(factory_secret);
        let schnorr = frostsnap_core::schnorr_fun::new_with_synthetic_nonces::<
            sha2::Sha256,
            rand::rngs::ThreadRng,
        >();

        let message = Message::<Public>::plain("frostsnap-genuine-key", &pem_bytes);
        let signature = schnorr.sign(&factory_keypair, message);

        Certificate {
            rsa_key: pem_bytes,
            serial_number,
            timestamp,
            case_color,
            signature,
            factory_key: factory_keypair.public_key(),
        }
    };

    certificate
}

fn verify_certificate_signature(certificate: &Certificate) -> bool {
    let message = Message::<Public>::plain("frostsnap-genuine-key", &certificate.rsa_key);
    let schnorr = frostsnap_core::schnorr_fun::new_with_synthetic_nonces::<
        sha2::Sha256,
        rand::rngs::ThreadRng,
    >();
    schnorr.verify(&certificate.factory_key, message, &certificate.signature)
}

struct Connection {
    state: ConnectionState,
    port: FramedSerialPort<FactoryDownstream>,
}
enum ConnectionState {
    WaitingForMagic {
        last_wrote: Option<std::time::Instant>,
    },
    WaitingForState,
    BeginInitEntropy,
    InitEntropy,
    SettingDsKey {
        rsa_pub_key: RsaPublicKey,
    },
    SavingGenuineCertificate {
        rsa_pub_key: RsaPublicKey,
    },
    SigningChallenge {
        rsa_pub_key: RsaPublicKey,
        challenge: Vec<u8>,
    },
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
fn vec_to_fixed(vec: &Vec<u32>, fixed_len: usize) -> [u32; DS_NUM_WORDS] {
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
            let formatted: Vec<String> = arr.iter().map(|word| format!("0x{:08x}", word)).collect();
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

use aes::cipher::{block_padding::NoPadding, KeyIvInit};
use aes::Aes256;
use cbc::Encryptor;
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
        .map_err(|e| format!("Encryption error: {:?}", e))?;

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
