use std::collections::HashSet;

use clap::Parser;
use frostsnap_comms::CaseColor;
pub mod cli;
pub mod db;
pub mod ds;
pub mod genuine_certificate;
pub mod process;
pub mod serial_number;

pub const USB_VID: u16 = 12346;
pub const USB_PID: u16 = 4097;

pub const FACTORY_SECRET_KEY: [u8; 32] = [
    0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

pub struct FactoryState {
    pub target_color: CaseColor,
    pub target_quantity: usize,
    pub operator: String,
    pub devices_flashed: HashSet<String>, // serial numbers
    pub genuine_checks: HashSet<String>,  //serial numbers
    pub devices_failed: usize,
    pub db: db::Database,
}

impl FactoryState {
    pub fn new(
        color: CaseColor,
        quantity: usize,
        operator: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let database = db::Database::new()?;

        Ok(FactoryState {
            target_color: color,
            target_quantity: quantity,
            operator,
            devices_flashed: Default::default(),
            genuine_checks: Default::default(),
            devices_failed: 0,
            db: database,
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
            self.target_color,
            self.operator
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    match args.command {
        cli::Commands::Batch {
            color,
            quantity,
            operator,
        } => {
            println!("Starting factory batch:");
            println!("Color: {color}, Quantity: {quantity}, Operator: {operator}");

            let mut factory_state = FactoryState::new(color, quantity, operator)?;

            process::run_with_state(&mut factory_state);
        }
    }
}
