use clap::Parser;
use device_simulator::SimDevice;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "device_simulator",
    about = "Frostsnap desktop device simulator"
)]
struct Cli {
    #[arg(long, help = "Path to the state file")]
    state: PathBuf,

    #[arg(long, help = "Optional name for the device")]
    name: Option<String>,

    #[arg(long, help = "Run in headless mode without graphical window")]
    headless: bool,

    #[arg(
        long,
        default_value_t = frostsnap_widgets::DEFAULT_LIST_REVIEW_THRESHOLD,
        help = "Review a send as a summary and list when it has more recipients than this; 0 always uses the list, 1 uses it for more than one recipient"
    )]
    list_threshold: usize,

    #[arg(
        long,
        help = "Firmware image the app bundles; the simulator announces its digest so the app treats it as up to date"
    )]
    firmware: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let firmware_digest = match args.firmware {
        Some(path) => {
            let bytes: &'static [u8] = Box::leak(std::fs::read(path)?.into_boxed_slice());
            frostsnap_coordinator::FirmwareBin::new(bytes)
                .validate()
                .map_err(|e| format!("invalid firmware image: {e:?}"))?
                .digest()
        }
        None => frostsnap_comms::Sha256Digest([0u8; 32]),
    };
    let mut sim = SimDevice::new(
        args.state,
        args.name,
        args.headless,
        firmware_digest,
        args.list_threshold,
    )?;
    sim.run_loop()?;
    Ok(())
}
