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
    let bytes = fs::read(source).unwrap_or_else(|e| panic!("read firmware {source}: {e}"));
    let expected_pk = expected_secure_boot_pk(env_name);
    frostsnap_secure_boot::verify_firmware(&bytes, &expected_pk).unwrap_or_else(|e| {
        panic!(
            "firmware at {source} failed secure-boot verification for env={env_name}: {e}. \
             You may have run the build with the wrong FROSTSNAP_ENV for the firmware on disk."
        )
    });
}

fn expected_secure_boot_pk(env_name: &str) -> frostsnap_secure_boot::RsaPublicKey {
    match env_name {
        // Prod: the prod secure-boot private key is not in the repo, but the
        // signed prod bootloader is — its signature block embeds the prod
        // public key, which we recover by running the same self-consistency
        // check we run on firmware.
        "prod" => {
            let path = "../../frostsnap_factory/bootloader/prod/signed-bootloader.bin";
            println!("cargo:rerun-if-changed={path}");
            let bytes =
                fs::read(path).unwrap_or_else(|e| panic!("read prod bootloader {path}: {e}"));
            frostsnap_secure_boot::verify_and_extract_pk(&bytes).unwrap_or_else(|e| {
                panic!("prod bootloader at {path} failed secure-boot verification: {e}")
            })
        }
        // Dev: the dev secure-boot private key IS in the repo (development
        // material), so derive the expected public key from it directly.
        "dev" => {
            let path = "../../frostsnap_factory/bootloader/dev/secure-boot-key.pem";
            println!("cargo:rerun-if-changed={path}");
            let pem =
                fs::read(path).unwrap_or_else(|e| panic!("read dev signing key {path}: {e}"));
            frostsnap_secure_boot::public_key_from_private_pem(&pem)
                .unwrap_or_else(|e| panic!("derive dev pubkey from {path}: {e}"))
        }
        _ => panic!(
            "FROSTSNAP_ENV={env_name} is not a recognised secure-boot env (expected `dev` or `prod`)"
        ),
    }
}
