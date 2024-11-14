default_board := "v2"
non_device_packages := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p native"

alias erase := erase-device

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}} -- --erase-parts otadata,factory {{ARGS}}

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device BOARD=default_board +ARGS="":
    cd device && cargo build --release --features {{BOARD}} --bin {{BOARD}} {{ARGS}}

test +ARGS="":
    cargo test {{ARGS}} {{non_device_packages}}

check-non-device +ARGS="":
    cargo check {{non_device_packages}} {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cd device && cargo check {{ARGS}} --all-features --bins

lint-non-device +ARGS="":
    cargo fmt {{non_device_packages}} -- --check
    cargo clippy {{non_device_packages}} {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cd device && cargo clippy {{ARGS}} --all-features --bins -- -Dwarnings

lint-app +ARGS="":
    ( cd frostsnapp && flutter analyze {{ARGS}}; )

fix-dart:
    ( cd frostsnapp && dart format . && dart fix --apply )

gen:
    just "frostsnapp/gen"

fix: fix-dart
    cargo clippy --fix --allow-dirty --allow-staged {{non_device_packages}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    cargo fmt --all


run +ARGS="":
    just build-device
    just frostsnapp/run {{ARGS}}

check: check-non-device check-device
lint: lint-non-device lint-device lint-app
