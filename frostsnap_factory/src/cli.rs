use clap::{Parser, Subcommand};
use frostsnap_comms::CaseColor;

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
    },
}
