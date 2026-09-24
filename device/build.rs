//! esp-bootloader-esp-idf finds the partition table at a compile-time offset and silently defaults
//! to 0x8000 when it is unset, so a mismatch with the bootloader only shows up as a boot-time
//! panic. Fail the build instead.

use std::{env, fs, path::Path};

const OFFSET_VAR: &str = "ESP_BOOTLOADER_ESP_IDF_CONFIG_PARTITION_TABLE_OFFSET";
const SDKCONFIG: &str = "../frostsnap_factory/bootloader/sdkconfig.defaults";
const SDKCONFIG_KEY: &str = "CONFIG_PARTITION_TABLE_OFFSET";

fn main() {
    let sdkconfig = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join(SDKCONFIG);
    println!("cargo:rerun-if-changed={}", sdkconfig.display());
    println!("cargo:rerun-if-env-changed={OFFSET_VAR}");

    let bootloader_offset = fs::read_to_string(&sdkconfig)
        .unwrap_or_else(|e| panic!("reading {}: {e}", sdkconfig.display()))
        .lines()
        .find_map(|line| {
            line.strip_prefix(SDKCONFIG_KEY)?
                .strip_prefix('=')
                .map(parse_offset)
        })
        .unwrap_or_else(|| panic!("{SDKCONFIG_KEY} not found in {}", sdkconfig.display()));

    let firmware_offset = parse_offset(&env::var(OFFSET_VAR).unwrap_or_else(|_| {
        panic!("{OFFSET_VAR} is not set: it belongs in [env] of the repository's root .cargo/config.toml")
    }));

    if firmware_offset != bootloader_offset {
        panic!(
            "{OFFSET_VAR} ({firmware_offset:#x}, root .cargo/config.toml) must equal \
             {SDKCONFIG_KEY} ({bootloader_offset:#x}, {}), where the bootloader puts the table",
            sdkconfig.display()
        );
    }
}

fn parse_offset(value: &str) -> u32 {
    let value = value.trim().trim_matches('"');
    let parsed = match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => value.parse(),
    };
    parsed.unwrap_or_else(|e| panic!("invalid partition table offset {value:?}: {e}"))
}
