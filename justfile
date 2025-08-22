import 'fetch.just'

ordinary_crates := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p rust_lib_frostsnapp -p frostsnap_embedded -p frostsnap_macros -p frostsnap_embedded_widgets -p frostsnap_backup"

alias erase := erase-device

flash +args="":
    cd device && cargo run --release {{args}} --bin v2 -- --erase-parts otadata,factory

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device +args="":
    cd device && cargo build --release {{args}} --bin v2

build +ARGS="":
   (cd frostsnapp; just build {{ARGS}})

save-image +ARGS="":
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/v2 target/riscv32imc-unknown-none-elf/release/firmware.bin {{ARGS}}

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

fix-dart: maybe-gen
    ( cd frostsnapp && dart format . && dart fix --apply && flutter analyze )

maybe-gen:
    just "frostsnapp/maybe-gen"

gen:
    just "frostsnapp/gen"

fix: fix-dart fix-rust

fix-rust:
    cargo clippy --fix --allow-dirty --allow-staged {{ordinary_crates}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    cargo fmt --all

gen-firmware: build-device save-image

run +ARGS="": gen-firmware
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

simulate +ARGS="":
    (cd frostsnap_embedded_widgets && cargo run --bin simulate -- {{ARGS}})

widget_dev +args="":
    cd device && cargo run --bin widget_dev --release {{args}}
