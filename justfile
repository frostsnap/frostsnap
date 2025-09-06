import 'fetch.just'

default_board := "dev"
ordinary_crates := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p rust_lib_frostsnapp -p frostsnap_embedded -p frostsnap_macros -p frostsnap_factory -p frostsnap_widgets -p frost_backup"
device_crates := "-p frostsnap_device -p frostsnap_cst816s --target riscv32imc-unknown-none-elf"

alias erase := erase-device
alias demo := simulate

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --bin {{BOARD}} {{ARGS}} -- --erase-parts otadata,ota_0

# Flash firmware to device already configured for secure boot
flash-secure:
    just build-secure
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x12000 device/blank-otadata.bin
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x20000 target/riscv32imc-unknown-none-elf/release/firmware.bin

# Initial secure boot setup: bootloader + partitions + firmware + monitor
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
    cd device && cargo build --release --bin {{BOARD}} {{ARGS}}
    just save-image {{BOARD}} "firmware.bin"

build-secure BOARD=default_board +ARGS="":
    just build-device {{BOARD}} {{ARGS}}
    just save-image {{BOARD}} "unsigned-firmware.bin"
    espsecure.py sign_data -v 2 -k device/secure_boot_signing_key.pem -o target/riscv32imc-unknown-none-elf/release/firmware.bin target/riscv32imc-unknown-none-elf/release/unsigned-firmware.bin

build-deterministic:
    cd device && ./deterministic-build.sh

build +ARGS="":
   (cd frostsnapp; just build {{ARGS}})

# Regardless of the specified BOARD, we tend to save the firmware as `firmware.bin` for streamlined bundling within the app.
save-image BOARD=default_board OUTNAME="firmware.bin" +ARGS="":
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/{{BOARD}} target/riscv32imc-unknown-none-elf/release/{{OUTNAME}} {{ARGS}}

test-ordinary +ARGS="":
    cargo test {{ARGS}} {{ordinary_crates}}

test: test-ordinary

check-ordinary +ARGS="":
    cargo check {{ordinary_crates}} {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cargo check {{device_crates}} {{ARGS}} --all-features

lint-ordinary +ARGS="":
    cargo fmt {{ordinary_crates}} -- --check
    cargo clippy {{ordinary_crates}} {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cargo clippy {{device_crates}} {{ARGS}} --all-features -- -Dwarnings

dart-format-check-app:
    ( cd frostsnapp; dart format --set-exit-if-changed --output=none  $(find ./lib -type f -name "*.dart" -not -path "./lib/src/rust/*") )

lint-app +ARGS="": dart-format-check-app
    ( cd frostsnapp; flutter analyze {{ARGS}} )

fix-dart: maybe-gen
    ( cd frostsnapp && dart format . && dart fix --apply && flutter analyze )

maybe-gen:
    just "frostsnapp/maybe-gen"

gen:
    just "frostsnapp/gen"

fix: fix-dart fix-rust

fix-rust:
    cargo clippy --fix --allow-dirty --allow-staged {{ordinary_crates}} --all-features --tests --bins
    cargo clippy --fix --allow-dirty --allow-staged {{device_crates}} --all-features
    cargo fmt --all

gen-firmware: build-device save-image

run +ARGS="":
    just frostsnapp/run {{ARGS}}

# Run the app with bundled firmware
run-secure +ARGS="":
    just frostsnapp/run-secure

fetch-riscv VERSION="2024.09.03-nightly":
    #!/bin/sh
    version="riscv32-elf-ubuntu-22.04-gcc-nightly-{{VERSION}}.tar.gz"
    just fetch https://github.com/riscv-collab/riscv-gnu-toolchain/releases/download/2024.09.03/$version "$version"
    tar -zxf "$version" && rm "$version"

check: check-ordinary check-device
lint: lint-ordinary lint-device lint-app

install-cargo-bins:
    just frostsnapp/install-cargo-bins

backup +ARGS="":
    cargo run --release --bin frost_backup -- {{ARGS}}

simulate +ARGS="":
    (cd widget_simulator && cargo run -- {{ARGS}}; )

widget_dev +args="":
    cd device && cargo run --bin widget_dev --release {{args}}
