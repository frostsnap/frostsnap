default_board := "frostypede"
non_device_packages := "-p native -p frostsnap_core -p coordinator-cli -p frostsnap_coordinator"

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}}

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

check: check-non-device check-device
lint: lint-non-device lint-device
