fn main() {
    use std::env;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    if env::var("BUILD_FIRMWARE").is_ok() {
        let out_dir = env::var("OUT_DIR").unwrap();
        let output_path = PathBuf::from(out_dir).join("firmware.bin");

        // // Ensure the output directory exists
        // let out_dir = env::var("OUT_DIR").unwrap();
        // let binary_path = Path::new(&out_dir).join("crate_a_binary");
        let target_binary_path = Path::new("../../target/riscv32imc-unknown-none-elf/release/v2");
        println!("cargo:rerun-if-changed={}", target_binary_path.display());

        if !target_binary_path.exists() {
            eprintln!(
                "device firmware elf file doesn't exist at {}. Build before trying to build app.",
                target_binary_path.display()
            );
        }
        // println!("cargo:rerun-if-changed=../crate_a/src/*");
        let status = Command::new("espflash")
            .args(["save-image", "--chip=esp32c3"])
            .arg(target_binary_path)
            .arg(&output_path)
            .status()
            .expect("Failed to create binary firmware file from elf file");

        assert!(
            status.success(),
            "Unsuccesful exit of command to create binary firmware file from elf file"
        );
    } else {
        println!("cargo:rustc-cfg=no_build_firmware")
    }
}
