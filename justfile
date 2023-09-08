default_board := "frostypede"

flash BOARD=default_board:
    cd device && cargo run --release --features {{BOARD}} --bin {{BOARD}}

