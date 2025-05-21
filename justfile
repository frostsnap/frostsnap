import 'fetch.just'

default_board := "v2"
ordinary_crates := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p native -p frostsnap_embedded -p frostsnap_macros"

alias erase := erase-device

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}} -- --erase-parts otadata,factory {{ARGS}}

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device BOARD=default_board +ARGS="":
    cd device && cargo build --release --features {{BOARD}} --bin {{BOARD}} {{ARGS}}

save-image BOARD=default_board +ARGS="":
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/{{BOARD}} target/riscv32imc-unknown-none-elf/release/firmware.bin {{ARGS}}

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
    ( cd frostsnapp; dart format --set-exit-if-changed --output=none $(find ./lib -type f -name "*.dart" -not -name "bridge_*") )

lint-app +ARGS="": dart-format-check-app
    ( cd frostsnapp; flutter analyze {{ARGS}} )

fix-dart:
    ( cd frostsnapp && dart format . && dart fix --apply && flutter analyze )

gen:
    just "frostsnapp/gen"

fix: fix-dart
    cargo clippy --fix --allow-dirty --allow-staged {{ordinary_crates}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    cargo fmt --all


run +ARGS="": build-device save-image
    just frostsnapp/run {{ARGS}}

fetch-riscv VERSION="2024.09.03-nightly":
    #!/bin/sh
    version="riscv32-elf-ubuntu-22.04-gcc-nightly-{{VERSION}}.tar.gz"
    just fetch https://github.com/riscv-collab/riscv-gnu-toolchain/releases/download/2024.09.03/$version "$version"
    tar -zxf "$version" && rm "$version"

check: check-ordinary check-device
lint: lint-ordinary lint-device lint-app
