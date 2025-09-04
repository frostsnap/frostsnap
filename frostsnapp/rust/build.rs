use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(bundle_firmware)");

    if env::var("BUNDLE_FIRMWARE").is_ok() {
        println!("cargo:rustc-cfg=bundle_firmware");
        let source_path =
            Path::new("../../target/riscv32imc-unknown-none-elf/release/frontier-firmware.bin");
        let out_dir = env::var("OUT_DIR").unwrap();
        let dest_path = Path::new(&out_dir).join("firmware.bin");

        println!("cargo:rerun-if-changed={}", source_path.display());

        if !source_path.exists() {
            eprintln!(
                "device firmware file doesn't exist at {}. Build before trying to build app.",
                source_path.display()
            );
        } else {
            fs::copy(source_path, dest_path).expect("Failed to copy firmware.bin to OUT_DIR");
        }
    }
}
