import 'fetch.just'

default_board := "v2"
ordinary_crates := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p rust_lib_frostsnapp -p frostsnap_embedded -p frostsnap_macros"

alias erase := erase-device

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}} -- --erase-parts otadata,ota_0 {{ARGS}}

flash-secure:
    just build-device
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x12000 device/blank-otadata.bin
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x20000 target/riscv32imc-unknown-none-elf/release/firmware.bin

flash-secure-new +ARGS="":
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x0 device/bootloader.bin {{ARGS}}
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0xD000 device/partitions.bin {{ARGS}}
    just flash-secure
    just monitor

monitor +ARGS="":
    espflash monitor --no-stub

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device BOARD=default_board +ARGS="":
    cd device && cargo build --release --features {{BOARD}} --bin {{BOARD}} {{ARGS}}
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/{{BOARD}} target/riscv32imc-unknown-none-elf/release/unsigned-firmware.bin {{ARGS}}
    espsecure.py sign_data -v 2 -k device/secure_boot_signing_key.pem -o target/riscv32imc-unknown-none-elf/release/firmware.bin target/riscv32imc-unknown-none-elf/release/unsigned-firmware.bin

build +ARGS="":
   (cd frostsnapp; just build {{ARGS}})

test-secure-boot BOARD=default_board +ARGS="":
    cd device && cargo build --release --features {{BOARD}} --bin {{BOARD}} {{ARGS}}
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/{{BOARD}} target/riscv32imc-unknown-none-elf/release/unsigned-firmware.bin {{ARGS}}
    espsecure.py sign_data -v 2 -k device/evil_secure_boot_signing_key.pem -o target/riscv32imc-unknown-none-elf/release/firmware.bin target/riscv32imc-unknown-none-elf/release/unsigned-firmware.bin
    (cd frostsnapp; BUNDLE_FIRMWARE=1 flutter run {{ARGS}})
    
test-ordinary +ARGS="":
    cargo test {{ARGS}} {{ordinary_crates}}

test: test-ordinary

check-ordinary +ARGS="":
    cargo check {{ordinary_crates}} {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cd device && cargo check {{ARGS}} --all-features --bins

lint-ordinary +ARGS="":
    cargo fmt {{ordinary_crates}} -- --check
    cargo clippy {{ordinary_crates}} {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cd device && cargo clippy {{ARGS}} --all-features --bins -- -Dwarnings

dart-format-check-app:
    ( cd frostsnapp; dart format --set-exit-if-changed --output=none  $(find ./lib -type f -name "*.dart" -not -path "./lib/src/rust/*") )

lint-app +ARGS="": dart-format-check-app
    ( cd frostsnapp; flutter analyze {{ARGS}} )

fix-dart:
    ( cd frostsnapp && dart format . && dart fix --apply && flutter analyze )

gen:
    just "frostsnapp/gen"

fix: fix-dart fix-rust

fix-rust:
    cargo clippy --fix --allow-dirty --allow-staged {{ordinary_crates}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    cargo fmt --all


run +ARGS="": build-device
    (cd frostsnapp; BUNDLE_FIRMWARE=1 flutter run {{ARGS}})

fetch-riscv VERSION="2024.09.03-nightly":
    #!/bin/sh
    version="riscv32-elf-ubuntu-22.04-gcc-nightly-{{VERSION}}.tar.gz"
    just fetch https://github.com/riscv-collab/riscv-gnu-toolchain/releases/download/2024.09.03/$version "$version"
    tar -zxf "$version" && rm "$version"

check: check-ordinary check-device
lint: lint-ordinary lint-device lint-app

install-cargo-bins:
    just frostsnapp/install-cargo-bins
