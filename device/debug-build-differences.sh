#!/bin/bash
set -euo pipefail

echo "🔍 Debugging build differences..."

FIRMWARE_BIN="target/riscv32imc-unknown-none-elf/release/firmware.bin"

if [[ ! -f "$FIRMWARE_BIN" ]]; then
    echo "❌ Firmware binary not found: $FIRMWARE_BIN"
    exit 1
fi

echo "📊 Binary Analysis:"
echo "Size: $(stat -c%s "$FIRMWARE_BIN") bytes"
echo "SHA256: $(sha256sum "$FIRMWARE_BIN" | cut -d' ' -f1)"
echo ""

echo "🦀 Rust Environment:"
echo "Rustc: $(rustc --version)"
echo "Cargo: $(cargo --version)"
echo "Target installed: $(rustup target list --installed | grep riscv32imc || echo 'NOT FOUND')"
echo ""

echo "🏗️  Build Environment:"
echo "SOURCE_DATE_EPOCH: ${SOURCE_DATE_EPOCH:-'NOT SET'}"
echo "CARGO_BUILD_INCREMENTAL: ${CARGO_BUILD_INCREMENTAL:-'NOT SET'}"
echo "CARGO_TARGET_DIR: ${CARGO_TARGET_DIR:-'NOT SET'}"
echo ""

echo "🔍 Checking for non-deterministic content:"

echo "Timestamps in binary:"
strings "$FIRMWARE_BIN" | grep -E '20[0-9]{2}' | head -5 || echo "None found"

echo ""
echo "Nix store paths in binary:"
strings "$FIRMWARE_BIN" | grep '/nix/store' | head -3 || echo "None found"

echo ""
echo "Build-related strings:"
strings "$FIRMWARE_BIN" | grep -E '(debug|release|target|rustc)' | head -5 || echo "None found"

echo ""
echo "🔄 Rebuilding with maximum determinism..."

# Clean everything
cargo clean
rm -rf target/

# Set additional deterministic flags
export CARGO_BUILD_INCREMENTAL=false
export SOURCE_DATE_EPOCH=1704067200
export TZ=UTC

# Rebuild
echo "Building with strict deterministic settings..."
cargo build --release --features v2 --bin v2 --locked
just save-image v2

# Check if the build succeeded
if [[ -f "$FIRMWARE_BIN" ]]; then
    echo ""
    echo "📊 New build results:"
    echo "Size: $(stat -c%s "$FIRMWARE_BIN") bytes"
    echo "SHA256: $(sha256sum "$FIRMWARE_BIN" | cut -d' ' -f1)"
else
    echo "❌ Build failed or firmware binary not found"
fi
