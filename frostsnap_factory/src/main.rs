use clap::Parser;

pub mod ds;
pub mod genuine_certificate;
pub mod process;
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

pub const FACTORY_KEY: [u8; 32] = [
    0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];
pub const DS_CHALLENGE: &str = "354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b";

fn main() {
    process::run()
}
