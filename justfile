default_board := "v2"
non_device_packages := "-p native -p frostsnap_core -p frostsnap_coordinator"

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}}

erase-device BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}} -- --erase-data-parts nvs --partition-table partitions.csv

erase:
    just erase-device

build-device BOARD=default_board +ARGS="":
    cd device && cargo build {{ARGS}} --features {{BOARD}} --bin {{BOARD}}

test +ARGS="":
    cargo test {{ARGS}} {{non_device_packages}}

check-non-device +ARGS="":
    cargo check {{non_device_packages}} {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cd device && cargo check {{ARGS}} --all-features --bins

lint-non-device +ARGS="":
    cargo fmt --all -- --check
    cargo clippy {{non_device_packages}} {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cd device && cargo clippy {{ARGS}} --all-features --bins -- -Dwarnings

fix:
    cargo fmt --all
    cargo clippy --fix --allow-dirty --allow-staged {{non_device_packages}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    ( cd frostsnapp && dart format .; )

run:
    just frostsnapp/run

check: check-non-device check-device
lint: lint-non-device lint-device
