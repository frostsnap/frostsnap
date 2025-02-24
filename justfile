default_board := "v2"
coordinator_packages := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p native"

alias erase := erase-device

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}} -- --erase-parts otadata,factory {{ARGS}}

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device BOARD=default_board +ARGS="":
    cd device && cargo build --release --features {{BOARD}} --bin {{BOARD}} {{ARGS}}

test-coordinator +ARGS="":
    cargo test {{ARGS}} {{coordinator_packages}}

test +ARGS="":
   just test-coordinator {{ARGS}}

check-coordinator +ARGS="":
    cargo check {{coordinator_packages}} {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cd device && cargo check {{ARGS}} --all-features --bins

lint-coordinator +ARGS="":
    cargo fmt {{coordinator_packages}} -- --check
    cargo clippy {{coordinator_packages}} {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cd device && cargo clippy {{ARGS}} --all-features --bins -- -Dwarnings

dart-format-check-app:
    ( cd frostsnapp; dart format --set-exit-if-changed --output=none $(find ./lib -type f -name "*.dart" -not -name "bridge_*") )

lint-app +ARGS="": dart-format-check-app
    ( cd frostsnapp; flutter analyze {{ARGS}} )

fix-dart:
    ( cd frostsnapp && dart format . && dart fix --apply )

gen:
    just "frostsnapp/gen"

fix: fix-dart
    cargo clippy --fix --allow-dirty --allow-staged {{coordinator_packages}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    cargo fmt --all


run +ARGS="":
    just build-device
    just frostsnapp/run {{ARGS}}

check: check-coordinator check-device
lint: lint-coordinator lint-device lint-app
