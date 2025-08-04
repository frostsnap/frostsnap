use crate::db::Database;
use clap::{Parser, Subcommand};

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
        color: String,
        /// Number of devices to flash
        #[arg(short, long)]
        quantity: u32,
        /// Operator name
        #[arg(short, long)]
        operator: String,
    },
    /// Show current status
    Status,
}

pub fn handle_status_command() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::new()?;
    let color_counts = db.get_color_counts()?;

    println!("Factory Status:");
    println!("===============");

    if color_counts.is_empty() {
        println!("No devices flashed yet.");
    } else {
        let mut total = 0;
        for (color, count) in &color_counts {
            println!("{color}: {count} devices");
            total += count;
        }
        println!("---------------");
        println!("Total: {total} devices");
    }

    Ok(())
}
