use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(bundle_firmware)");
    println!("cargo::rustc-check-cfg=cfg(genuine_cert_key)");
    println!("cargo:rerun-if-env-changed=BUNDLE_FIRMWARE");
    println!("cargo:rerun-if-env-changed=FROSTSNAP_ENV");

    let out_dir = env::var("OUT_DIR").unwrap();
    let frostsnap_env = env::var("FROSTSNAP_ENV").ok().filter(|v| !v.is_empty());

    // Firmware bundling
    match env::var("BUNDLE_FIRMWARE").ok().filter(|v| !v.is_empty()) {
        Some(val) if val == "0" || val == "false" => {}
        Some(val) if val == "1" || val == "true" => {
            let env_name = frostsnap_env.as_deref().unwrap_or("dev");
            let path =
                format!("../../target/riscv32imc-unknown-none-elf/release/{env_name}-frontier.bin");
            copy_to_out(&path, &out_dir, "firmware.bin", "bundle_firmware");
        }
        Some(path) => {
            copy_to_out(&path, &out_dir, "firmware.bin", "bundle_firmware");
        }
        None => {}
    }

    // Genuine certificate key — derived from FROSTSNAP_ENV
    if let Some(env_name) = &frostsnap_env {
        let key_path = format!("../../frostsnap_factory/genuine/{env_name}/public_key.hex");
        copy_to_out(
            &key_path,
            &out_dir,
            "genuine_cert_key.hex",
            "genuine_cert_key",
        );
    }
}

fn copy_to_out(source: &str, out_dir: &str, dest_name: &str, cfg_flag: &str) {
    let source_path = Path::new(source);
    let dest_path = Path::new(out_dir).join(dest_name);

    println!("cargo:rerun-if-changed={}", source_path.display());
    println!("cargo:rustc-cfg={cfg_flag}");

    if !source_path.exists() {
        panic!("{dest_name}: {source} does not exist");
    }
    fs::copy(source_path, &dest_path)
        .unwrap_or_else(|e| panic!("Failed to copy {dest_name} to OUT_DIR: {e}"));
}
