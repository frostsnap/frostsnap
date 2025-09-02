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
        /// Path to factory secret key file (.hex format)
        #[arg(short, long)]
        factory_secret: PathBuf,
        /// Connection URL to factory database
        #[arg(short, long)]
        db_connection_url: Option<String>,
        /// Optional batch note (e.g., "testing devices", "for Company X")
        #[arg(short = 'n', long)]
        batch_note: Option<String>,
    },
}
