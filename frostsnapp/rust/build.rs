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

    // Firmware bundling.
    //
    // Secure-boot verification only runs on the conventional `BUNDLE_FIRMWARE=1`
    // path (signed `*-frontier.bin`). An explicit-path `BUNDLE_FIRMWARE=<path>`
    // is treated as "caller knows what they're doing" so workflows like
    // `just legacy-run` (unsigned legacy firmware) keep working.
    match env::var("BUNDLE_FIRMWARE").ok().filter(|v| !v.is_empty()) {
        Some(val) if val == "0" || val == "false" => {}
        Some(val) if val == "1" || val == "true" => {
            let env_name = frostsnap_env.as_deref().unwrap_or("dev");
            let path =
                format!("../../target/riscv32imc-unknown-none-elf/release/{env_name}-frontier.bin");
            verify_signed_firmware(&path, env_name);
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

fn verify_signed_firmware(source: &str, env_name: &str) {
    if env_name != "dev" && env_name != "prod" {
        panic!(
            "FROSTSNAP_ENV={env_name} is not a recognised secure-boot env (expected `dev` or `prod`)"
        );
    }

    let pem_path = format!("../../frostsnap_factory/bootloader/{env_name}/secure-boot-pubkey.pem");
    println!("cargo:rerun-if-changed={pem_path}");
    let pem =
        fs::read(&pem_path).unwrap_or_else(|e| panic!("read secure-boot key {pem_path}: {e}"));
    let expected_pk = frostsnap_secure_boot::secure_boot_pubkey_from_pem(&pem)
        .unwrap_or_else(|e| panic!("parse secure-boot key {pem_path}: {e}"));

    let bytes = fs::read(source).unwrap_or_else(|e| panic!("read firmware {source}: {e}"));
    let verified = frostsnap_secure_boot::verify_firmware(&bytes).unwrap_or_else(|e| {
        panic!("firmware at {source} failed secure-boot verification for env={env_name}: {e}")
    });
    if !verified.signed_by(&expected_pk) {
        panic!(
            "firmware at {source} is not signed by the env={env_name} secure-boot key. \
             You may have run the build with the wrong FROSTSNAP_ENV for the firmware on disk."
        );
    }
}
