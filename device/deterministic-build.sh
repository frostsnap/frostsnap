#!/usr/bin/env bash
set -euo pipefail

# The build directory must be the same as it is frequently written into the binary
BUILD_DIR="/var/tmp/frostsnap"
FIRMWARE_PATH="target/riscv32imc-unknown-none-elf/release/frontier.bin"

echo "Building in ${BUILD_DIR}"

# Clean and prepare
rm -rf "${BUILD_DIR}"
mkdir -p "${BUILD_DIR}"

# Copy source (only committed files, automatically excludes gitignored/untracked)
cd ..
git archive HEAD | tar -x -C "${BUILD_DIR}"
cd "${BUILD_DIR}"

# Build in nix environment
nix develop --command just build-firmware frontier --locked
nix develop --command just save-image frontier

# The image embeds CARGO_PKG_VERSION, so the built tree's Cargo.toml names it without compiling a reader.
echo "Version: v$(awk -F'"' '/^version = /{print $2; exit}' device/Cargo.toml)"
echo "Build complete - $(wc -c < "${FIRMWARE_PATH}" | tr -d ' ') bytes"
echo "$(shasum -a 256 "${BUILD_DIR}/${FIRMWARE_PATH}")"
