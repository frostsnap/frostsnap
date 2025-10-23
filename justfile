import 'fetch.just'

default_board := "dev"
ordinary_crates := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p rust_lib_frostsnapp -p frostsnap_embedded -p frostsnap_macros -p frostsnap_factory -p frostsnap_widgets -p frost_backup -p frostsnap_macros -p frostsnap_fonts"
device_crates := "-p frostsnap_device -p frostsnap_cst816s"

alias erase := erase-device
alias demo := simulate

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --bin {{BOARD}} {{ARGS}} -- --erase-parts otadata,ota_0

# Flash firmware to device already configured for secure boot
flash-secure BOARD=default_board:
    just build-secure {{BOARD}}
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x12000 device/blank-otadata.bin
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x20000 target/riscv32imc-unknown-none-elf/release/firmware.bin

# Initial secure boot setup: bootloader + partitions + firmware + monitor
flash-secure-new BOARD=default_board +ARGS="":
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x0 device/bootloader-dev-sb.bin {{ARGS}}
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0xD000 device/partitions.bin {{ARGS}}
    ## TMP: Instead of building again, we want to flash with pre-signed firmware!
    # just flash-secure {{BOARD}}
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x20000 target/riscv32imc-unknown-none-elf/release/firmware.bin
    just monitor

# Initial secure boot setup: bootloader + partitions + firmware
flash-frontier-new BOARD="frontier" +ARGS="":
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x0 device/bootloader-frontier.bin {{ARGS}}
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0xD000 device/partitions.bin {{ARGS}}
    espflash write-bin --chip esp32c3 --port /dev/ttyACM0 --baud 921600 --no-stub 0x20000 target/riscv32imc-unknown-none-elf/release/firmware.bin

monitor +ARGS="":
    espflash monitor --no-stub

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device BOARD=default_board +ARGS="":
    cd device && cargo build --release --bin {{BOARD}} ${DEVICE_BUILD_ARGS:-} {{ARGS}}
    just save-image {{BOARD}} "firmware.bin"

build-secure BOARD=default_board +ARGS="":
    just build-device {{BOARD}} {{ARGS}}
    just save-image {{BOARD}} "unsigned-firmware.bin"
    just sign-firmware

build-deterministic:
    cd device && ./deterministic-build.sh

build +ARGS="":
   (cd frostsnapp; just build {{ARGS}})

# Regardless of the specified BOARD, we tend to save the firmware as `firmware.bin` for streamlined bundling within the app.
save-image BOARD=default_board OUTNAME="firmware.bin" +ARGS="":
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/{{BOARD}} target/riscv32imc-unknown-none-elf/release/{{OUTNAME}} {{ARGS}}

# Sign a firmware binary for secure boot
sign-firmware INPUT="target/riscv32imc-unknown-none-elf/release/unsigned-firmware.bin" OUTPUT="target/riscv32imc-unknown-none-elf/release/firmware.bin":
    espsecure.py sign_data -v 2 -k device/secure_boot_signing_key.pem -o {{OUTPUT}} {{INPUT}}

test-ordinary +ARGS="":
    cargo test {{ARGS}} {{ordinary_crates}}

test: test-ordinary

check-ordinary +ARGS="":
    cargo check {{ordinary_crates}} {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cargo check --target riscv32imc-unknown-none-elf {{device_crates}} {{ARGS}} --all-features

lint-ordinary +ARGS="":
    cargo fmt {{ordinary_crates}} -- --check
    cargo clippy {{ordinary_crates}} {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cargo fmt {{device_crates}} -- --check
    cargo clippy {{device_crates}} --target riscv32imc-unknown-none-elf  {{ARGS}} --all-features -- -Dwarnings

dart-format-check-app:
    ( cd frostsnapp; dart format --set-exit-if-changed --output=none  $(find ./lib -type f -name "*.dart" -not -path "./lib/src/rust/*" -not -name "*.freezed.dart") )

lint-app +ARGS="": maybe-gen dart-format-check-app
    ( cd frostsnapp; flutter analyze {{ARGS}} )

fix-dart: maybe-gen
    ( cd frostsnapp && dart format $(find ./lib -type f -name "*.dart" -not -path "./lib/src/rust/*" -not -name "*.freezed.dart") && dart fix --apply && flutter analyze )

maybe-gen:
    just "frostsnapp/maybe-gen"

gen:
    just "frostsnapp/gen"

fix: fix-dart fix-rust

fix-rust:
    cargo clippy --fix --allow-dirty --allow-staged {{ordinary_crates}} --all-features --tests --bins
    cargo clippy --fix --allow-dirty --target riscv32imc-unknown-none-elf --allow-staged {{device_crates}} --all-features
    cargo fmt --all

gen-firmware: build-device save-image

run +ARGS="":
    just frostsnapp/run {{ARGS}}

# Run the app with bundled firmware
run-secure +ARGS="":
    just frostsnapp/run-secure

# Build macOS DMG
build-dmg:
    just frostsnapp/build-dmg

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
