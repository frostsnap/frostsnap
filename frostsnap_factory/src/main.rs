use std::{collections::HashSet, env};

use clap::Parser;
use frostsnap_comms::genuine_certificate::CaseColor;
use frostsnap_core::{
    hex,
    schnorr_fun::fun::{marker::EvenY, KeyPair, Scalar},
};
pub mod cli;
pub mod db;
pub mod ds;
pub mod process;

const BOARD_REVISION: &str = "2.7-1625";

pub const USB_VID: u16 = 12346;
pub const USB_PID: u16 = 4097;

pub struct FactoryState {
    pub target_color: CaseColor,
    pub target_quantity: usize,
    pub operator: String,
    pub devices_flashed: HashSet<String>, // serial numbers
    pub genuine_checks: HashSet<String>,  //serial numbers
    pub devices_failed: usize,
    pub revision: String,
    pub factory_keypair: KeyPair<EvenY>,
    pub db: db::Database,
    pub batch_note: Option<String>,
}

impl FactoryState {
    pub fn new(
        color: CaseColor,
        quantity: usize,
        operator: String,
        revision: String,
        factory_keypair: KeyPair<EvenY>,
        db_connection_url: String,
        batch_note: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let db = db::Database::new(db_connection_url)?;

        Ok(FactoryState {
            target_color: color,
            target_quantity: quantity,
            operator,
            devices_flashed: Default::default(),
            genuine_checks: Default::default(),
            devices_failed: 0,
            revision,
            factory_keypair,
            db,
            batch_note,
        })
    }

    pub fn record_success(
        &mut self,
        serial_number: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db.mark_factory_complete(
            serial_number,
            &self.target_color.to_string(),
            &self.operator,
            self.batch_note.as_deref(),
        )?;
        self.devices_flashed.insert(serial_number.to_string());
        Ok(())
    }

    pub fn record_genuine_verified(
        &mut self,
        serial_number: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db.mark_genuine_verified(serial_number)?;
        self.genuine_checks.insert(serial_number.to_string());
        Ok(())
    }

    pub fn record_failure(
        &mut self,
        serial_number: &str,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db.mark_failed(
            serial_number,
            &self.target_color.to_string(),
            &self.operator,
            reason,
            self.batch_note.as_deref(),
        )?;
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

fn load_factory_keypair(
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

    let factory_secret = Scalar::from_bytes_mod_order(array)
        .non_zero()
        .ok_or("Invalid secret key: resulted in zero scalar")?;
    let factory_keypair = KeyPair::new_xonly(factory_secret);

    if factory_keypair.public_key().to_xonly_bytes() != frostsnap_comms::FACTORY_PUBLIC_KEY {
        return Err("Loaded factory secret does not match expected public key".into());
    }

    Ok(factory_keypair)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    match args.command {
        cli::Commands::Batch {
            color,
            quantity,
            operator,
            factory_secret,
            db_connection_url,
            batch_note,
        } => {
            let factory_keypair = load_factory_keypair(&factory_secret)?;

            let db_connection_url = db_connection_url
                .or_else(|| env::var("DATABASE_URL").ok())
                .ok_or("No database URL provided via --db-connection-url or DATABASE_URL")?;

            let mut factory_state = FactoryState::new(
                color,
                quantity,
                operator.clone(),
                BOARD_REVISION.to_string(),
                factory_keypair,
                db_connection_url,
                batch_note,
            )?;

            println!("Starting factory batch:");
            println!("Color: {color}, Quantity: {quantity}, Operator: {operator}");

            process::run_with_state(&mut factory_state);
        }
    }
}
