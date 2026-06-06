use clap::{Parser, Subcommand};
use frostsnap_comms::genuine_certificate::CaseColor;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start a factory batch
    Batch {
        /// Case color
        #[arg(short, long)]
        color: CaseColor,
        /// Number of devices to flash
        #[arg(short, long)]
        quantity: usize,
        /// Operator name
        #[arg(short, long)]
        operator: String,
        /// Environment (dev or prod) — determines key paths
        #[arg(long)]
        env: String,
        /// Connection URL to factory database
        #[arg(short, long)]
        db_connection_url: Option<String>,
        /// Optional batch note (e.g., "testing devices", "for Company X")
        #[arg(short = 'n', long)]
        batch_note: Option<String>,
    },
    /// Flash blank ESP32-C3 devices with production bootloader, partitions, and firmware
    BatchFlash {
        /// Maximum number of devices to flash at the same time
        #[arg(short, long, default_value_t = 2)]
        concurrency: usize,
        /// Environment (dev or prod) — determines artifact paths
        #[arg(long, default_value = "prod")]
        env: String,
        /// Output path for the merged flash image
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Generate an RSA-3072 signing key for ESP32 Secure Boot v2
    GenSecureBootKey {
        /// Output path for the PEM key file
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Generate a Schnorr keypair for genuine certificate signing
    GenGenuineCertKey {
        /// Output directory for key files (writes secret_key.hex and public_key.hex)
        #[arg(short, long)]
        output_dir: PathBuf,
    },
    /// Sign firmware for ESP32 Secure Boot v2
    SignFirmware {
        /// Path to unsigned firmware binary
        #[arg(short, long)]
        input: PathBuf,
        /// Output path for signed firmware binary
        #[arg(short, long)]
        output: PathBuf,
        /// Path to RSA-3072 secure boot key (PEM)
        #[arg(short, long)]
        key: PathBuf,
    },
    /// Verify a signed firmware or bootloader binary
    VerifyFirmware {
        /// Path to signed binary
        #[arg(short, long)]
        input: PathBuf,
        /// Require the firmware digest to be in KNOWN_FIRMWARE_VERSIONS. Use in release CI
        /// to block shipping a release whose firmware version hasn't been registered.
        #[arg(long)]
        require_known_version: bool,
    },
    /// Provision a single device (no database required)
    Provision {
        /// Case color
        color: CaseColor,
        /// Environment (dev or prod) — determines key paths
        #[arg(long)]
        env: String,
    },
    /// Verify a connected device's genuine certificate
    GenuineCheck,
}
