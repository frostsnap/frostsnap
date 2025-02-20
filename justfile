default_board := "v2"
ordinary_crates := "-p frostsnap_core -p frostsnap_coordinator -p frostsnap_comms -p native -p frostsnap_embedded"

alias erase := erase-device

flash BOARD=default_board +ARGS="":
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}} -- --erase-parts otadata,factory {{ARGS}}

erase-device +ARGS="nvs":
    cd device && espflash erase-parts --partition-table partitions.csv {{ARGS}}

build-device BOARD=default_board +ARGS="":
    cd device && cargo build --release --features {{BOARD}} --bin {{BOARD}} {{ARGS}}

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

lint-app +ARGS="":
    ( cd frostsnapp && flutter analyze {{ARGS}}; )

fix-dart:
    ( cd frostsnapp && dart format . && dart fix --apply )

gen:
    just "frostsnapp/gen"

fix: fix-dart
    cargo clippy --fix --allow-dirty --allow-staged {{ordinary_crates}} --all-features --tests --bins
    ( cd device && cargo clippy --fix --allow-dirty --allow-staged --all-features --bins; )
    cargo fmt --all


run +ARGS="":
    just build-device
    just frostsnapp/run {{ARGS}}

check: check-ordinary check-device
lint: lint-ordinary lint-device lint-app
