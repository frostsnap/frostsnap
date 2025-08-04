use clap::Parser;
pub mod cli;
pub mod db;
pub mod ds;
pub mod genuine_certificate;
pub mod process;
pub mod serial_number;

pub const USB_VID: u16 = 12346;
pub const USB_PID: u16 = 4097;
pub const FACTORY_KEY: [u8; 32] = [
    0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];
pub const DS_CHALLENGE: &str = "354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b";

pub struct FactoryState {
    pub target_color: String,
    pub target_quantity: u32,
    pub operator: String,
    pub devices_flashed: u32,
    pub devices_failed: u32,
    pub db: db::Database,
}

impl FactoryState {
    pub fn new(
        color: String,
        quantity: u32,
        operator: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let database = db::Database::new()?;
        let devices_flashed = database.get_device_count(Some(&color))?;

        Ok(FactoryState {
            target_color: color,
            target_quantity: quantity,
            operator,
            devices_flashed,
            devices_failed: 0,
            db: database,
        })
    }

    pub fn record_success(
        &mut self,
        serial_number: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db.insert_device(
            serial_number,
            &self.target_color,
            &self.operator,
            "flashed",
            None,
        )?;
        self.devices_flashed += 1;
        Ok(())
    }

    pub fn record_failure(
        &mut self,
        serial_number: &str,
        reason: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.db.insert_device(
            serial_number,
            &self.target_color,
            &self.operator,
            "failed",
            Some(reason),
        )?;
        self.devices_failed += 1;
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.devices_flashed >= self.target_quantity
    }

    pub fn print_progress(&self) {
        let percentage = if self.target_quantity > 0 {
            (self.devices_flashed as f32 / self.target_quantity as f32) * 100.0
        } else {
            0.0
        };

        println!(
            "Factory Tool - {} devices (Operator: {})",
            self.target_color, self.operator
        );
        println!(
            "Progress: {}/{} ({:.1}%)",
            self.devices_flashed, self.target_quantity, percentage
        );
        println!(
            "Success: {} | Failed: {}",
            self.devices_flashed, self.devices_failed
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
            println!(
                "Color: {color}, Quantity: {quantity}, Operator: {operator}"
            );

            let mut factory_state = FactoryState::new(color, quantity, operator)?;

            process::run_with_state(&mut factory_state);
        }
        cli::Commands::Status => {
            cli::handle_status_command()?;
        }
    }

    Ok(())
}
