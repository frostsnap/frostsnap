use std::{collections::HashSet, env};

use clap::Parser;
use frostsnap_comms::{genuine_certificate::CaseColor, Sha256Digest};
use frostsnap_core::{
    hex,
    schnorr_fun::fun::{marker::EvenY, KeyPair, Point, Scalar},
};
pub mod cli;
pub mod db;
pub mod ds;
pub mod genuine_check;
pub mod process;
pub mod secure_boot;

const BOARD_REVISION: &str = "2.7-1625";

pub const USB_VID: u16 = 12346;
pub const USB_PID: u16 = 4097;

pub struct FactoryState<D: db::FactoryDatabase> {
    pub target_color: CaseColor,
    pub target_quantity: usize,
    pub operator: String,
    pub devices_flashed: HashSet<String>, // serial numbers
    pub genuine_checks: HashSet<String>,  //serial numbers
    pub devices_failed: usize,
    pub revision: String,
    pub genuine_keypair: KeyPair<EvenY>,
    pub db: D,
    pub batch_note: Option<String>,
}

impl<D: db::FactoryDatabase> FactoryState<D> {
    pub fn new(
        color: CaseColor,
        quantity: usize,
        operator: String,
        revision: String,
        genuine_keypair: KeyPair<EvenY>,
        db: D,
        batch_note: Option<String>,
    ) -> Self {
        FactoryState {
            target_color: color,
            target_quantity: quantity,
            operator,
            devices_flashed: Default::default(),
            genuine_checks: Default::default(),
            devices_failed: 0,
            revision,
            genuine_keypair,
            db,
            batch_note,
        }
    }

    fn device_record<'a>(&'a self, serial_number: &'a str) -> db::DeviceRecord<'a> {
        db::DeviceRecord {
            serial_number,
            color: self.target_color,
            operator: &self.operator,
            board_revision: &self.revision,
            batch_note: self.batch_note.as_deref(),
        }
    }

    pub fn record_success(
        &mut self,
        serial_number: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db
            .mark_factory_complete(&self.device_record(serial_number))?;
        self.devices_flashed.insert(serial_number.to_string());
        Ok(())
    }

    pub fn record_genuine_verified(
        &mut self,
        serial_number: &str,
        firmware_digest: Sha256Digest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db
            .mark_genuine_verified(serial_number, firmware_digest)?;
        self.genuine_checks.insert(serial_number.to_string());
        Ok(())
    }

    pub fn record_failure(
        &mut self,
        serial_number: &str,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db
            .mark_failed(&self.device_record(serial_number), reason)?;
        self.devices_failed += 1;
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.devices_complete() >= self.target_quantity
    }

    pub fn devices_complete(&self) -> usize {
        self.genuine_checks
            .intersection(&self.devices_flashed)
            .count()
    }

    pub fn print_progress(&self) {
        let devices_complete = self.devices_complete();
        let percentage = if self.target_quantity > 0 {
            (devices_complete as f32 / self.target_quantity as f32) * 100.0
        } else {
            0.0
        };

        println!(
            "Factory Tool - {} devices (Operator: {})",
            self.target_color, self.operator
        );
        println!(
            "Progress: {}/{} ({:.1}%)",
            devices_complete, self.target_quantity, percentage
        );
        println!(
            "Success: {} | Failed: {}",
            devices_complete, self.devices_failed
        );
    }
}

fn load_genuine_keypair(
    path: &std::path::Path,
) -> Result<KeyPair<EvenY>, Box<dyn std::error::Error>> {
    let hex_content = std::fs::read_to_string(path)?;
    let hex_content = hex_content.trim();
    let hex_content = hex_content.strip_prefix("0x").unwrap_or(hex_content);

    let bytes = hex::decode(hex_content)?;
    if bytes.len() != 32 {
        return Err(format!("Expected 32 bytes, got {}", bytes.len()).into());
    }

    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);

    let genuine_secret = Scalar::from_bytes_mod_order(array)
        .non_zero()
        .ok_or("Invalid secret key: resulted in zero scalar")?;
    let genuine_keypair = KeyPair::new_xonly(genuine_secret);

    let public_key_path = path.with_file_name("public_key.hex");
    if public_key_path.exists() {
        let public_hex = std::fs::read_to_string(&public_key_path)?;
        let public_bytes = hex::decode(public_hex.trim())?;
        let expected: [u8; 32] = public_bytes
            .try_into()
            .map_err(|_| "public_key.hex must be 32 bytes")?;
        assert_eq!(
            genuine_keypair.public_key().to_xonly_bytes(),
            expected,
            "secret_key.hex does not match public_key.hex — wrong key file?"
        );
    }

    eprintln!(
        "Loaded genuine certificate public key: {}",
        hex::encode(&genuine_keypair.public_key().to_xonly_bytes())
    );

    Ok(genuine_keypair)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    match args.command {
        cli::Commands::Batch {
            color,
            quantity,
            operator,
            env: env_name,
            db_connection_url,
            batch_note,
        } => {
            let secret_path = format!("frostsnap_factory/genuine/{env_name}/secret_key.hex");
            let genuine_keypair = load_genuine_keypair(std::path::Path::new(&secret_path))?;

            eprintln!(
                "WARNING: Confirm this is the correct production key for batch provisioning."
            );
            eprintln!("Continue? [y/N]");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                return Err("Aborted by user".into());
            }

            let db_connection_url = db_connection_url
                .or_else(|| env::var("DATABASE_URL").ok())
                .ok_or("No database URL provided via --db-connection-url or DATABASE_URL")?;

            let db = db::MysqlDatabase::new(db_connection_url)?;
            let mut factory_state = FactoryState::new(
                color,
                quantity,
                operator.clone(),
                BOARD_REVISION.to_string(),
                genuine_keypair,
                db,
                batch_note,
            );

            println!("Starting factory batch:");
            println!("Color: {color}, Quantity: {quantity}, Operator: {operator}");

            process::run_with_state(&mut factory_state);
        }
        cli::Commands::GenSecureBootKey { output } => {
            if output.exists() {
                return Err(format!(
                    "{} already exists — refusing to overwrite",
                    output.display()
                )
                .into());
            }
            use rsa::pkcs1::EncodeRsaPrivateKey;
            let mut rng = rand::thread_rng();
            let key = rsa::RsaPrivateKey::new(&mut rng, 3072)?;
            key.write_pkcs1_pem_file(&output, rsa::pkcs1::LineEnding::LF)?;
            println!("Wrote RSA-3072 secure boot key to {}", output.display());
        }
        cli::Commands::GenGenuineCertKey { output_dir } => {
            let secret_path = output_dir.join("secret_key.hex");
            let public_path = output_dir.join("public_key.hex");

            for path in [&secret_path, &public_path] {
                if path.exists() {
                    return Err(format!(
                        "{} already exists — refusing to overwrite",
                        path.display()
                    )
                    .into());
                }
            }

            std::fs::create_dir_all(&output_dir)?;
            let mut rng = rand::thread_rng();
            let secret = Scalar::random(&mut rng);
            let keypair = KeyPair::<EvenY>::new_xonly(secret);

            std::fs::write(
                &secret_path,
                format!("{}\n", hex::encode(&keypair.secret_key().to_bytes())),
            )?;
            std::fs::write(
                &public_path,
                format!("{}\n", hex::encode(&keypair.public_key().to_xonly_bytes())),
            )?;

            println!("Wrote secret to {}", secret_path.display());
            println!("Wrote public key to {}", public_path.display());
            println!(
                "Public key (hex): {}",
                hex::encode(&keypair.public_key().to_xonly_bytes())
            );
        }
        cli::Commands::SignFirmware { input, output, key } => {
            let pem = std::fs::read(&key)?;
            let firmware = std::fs::read(&input)?;
            let signed = secure_boot::sign_firmware(&firmware, &pem)?;
            std::fs::write(&output, &signed)?;
            println!(
                "Signed {} bytes -> {} bytes written to {}",
                firmware.len(),
                signed.len(),
                output.display()
            );
        }
        cli::Commands::Provision {
            color,
            env: env_name,
        } => {
            let secret_path = format!("frostsnap_factory/genuine/{env_name}/secret_key.hex");
            let genuine_keypair = load_genuine_keypair(std::path::Path::new(&secret_path))?;
            let mut factory_state = FactoryState::new(
                color,
                1,
                "dev".into(),
                BOARD_REVISION.to_string(),
                genuine_keypair,
                db::DevDatabase::new(99999),
                None,
            );
            println!("Provisioning single device (color: {color})");
            process::run_with_state(&mut factory_state);
        }
        cli::Commands::VerifyFirmware { input } => {
            let signed = std::fs::read(&input)?;
            let public_key = secure_boot::verify_firmware(&signed)?;
            use rsa::traits::PublicKeyParts;
            use sha2::{Digest, Sha256};
            let modulus_le = public_key.n().to_bytes_le();
            let exponent_le = public_key.e().to_bytes_le();
            let mut key_material = Vec::new();
            key_material.extend_from_slice(&modulus_le);
            key_material.resize(384, 0);
            key_material.extend_from_slice(&exponent_le);
            key_material.resize(384 + 4, 0);
            let key_digest = Sha256::digest(&key_material);
            println!("Verified: {}", input.display());
            println!(
                "  Size: {} bytes ({} firmware + 4096 signature block)",
                signed.len(),
                signed.len() - 4096
            );
            println!("  Public key digest: {}", hex::encode(&key_digest));
        }
        cli::Commands::GenuineCheck => {
            let known_keys = load_known_genuine_keys();
            if known_keys.is_empty() {
                return Err("No genuine public keys found in frostsnap_factory/genuine/".into());
            }
            let result = genuine_check::run_genuine_check(&known_keys)?;
            if result.env == "dev" {
                println!("⚠️  DEV DEVICE — signed with DEV key (not production!) ⚠️");
            } else {
                println!("✅ VERIFIED ({})", result.env);
            }
            println!("  Serial:    {}", result.serial);
            println!("  Color:     {}", result.color);
            println!("  Revision:  {}", result.revision);
            println!("  Timestamp: {}", result.timestamp);
            println!("  Firmware:  {}", result.firmware_digest);
        }
    }

    Ok(())
}

fn load_known_genuine_keys() -> Vec<(&'static str, Point<EvenY>)> {
    let mut keys = Vec::new();
    for env in ["dev", "prod"] {
        let path = format!("frostsnap_factory/genuine/{env}/public_key.hex");
        if let Ok(content) = std::fs::read_to_string(&path) {
            let content = content.trim();
            if let Ok(bytes) = hex::decode(content) {
                if let Ok(array) = <[u8; 32]>::try_from(bytes.as_slice()) {
                    if let Some(point) = Point::<EvenY>::from_xonly_bytes(array) {
                        keys.push(if env == "dev" {
                            ("dev", point)
                        } else {
                            ("prod", point)
                        });
                    }
                }
            }
        }
    }
    keys
}
