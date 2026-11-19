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

# The shell's rustflags are the firmware's reproducibility settings, with GNU linker arguments
# that a host build on macOS can't link with. This tool only reads the finished image.
nix develop --command env -u CARGO_BUILD_RUSTFLAGS cargo run --locked -p frostsnap_factory -- firmware-info -i "${FIRMWARE_PATH}"
echo "Build complete - $(wc -c < "${FIRMWARE_PATH}" | tr -d ' ') bytes"
echo "$(shasum -a 256 "${BUILD_DIR}/${FIRMWARE_PATH}")"
