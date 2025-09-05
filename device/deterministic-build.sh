#!/usr/bin/env bash
set -euo pipefail

# The build directory must be the same as it is frequently written into the binary
BUILD_DIR="/var/tmp/frostsnap"
FIRMWARE_PATH="target/riscv32imc-unknown-none-elf/release/firmware.bin"

echo "Building in ${BUILD_DIR}"

# Clean and prepare
rm -rf "${BUILD_DIR}"
mkdir -p "${BUILD_DIR}"

# Copy source (exclude unnecessary files)
rsync -a --exclude='.git' --exclude='**/target/' --exclude='.github' ../ "${BUILD_DIR}/"
cd "${BUILD_DIR}"

# Build in nix environment
nix develop --command just build-device --locked
nix develop --command just save-image

echo "Build complete - $(stat -c%s "${BUILD_DIR}/${FIRMWARE_PATH}") bytes"
echo "$(sha256sum "${BUILD_DIR}/${FIRMWARE_PATH}")"
