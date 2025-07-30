#!/usr/bin/env bash
set -euo pipefail

echo "🔧 Building deterministically in /tmp/frostsnap..."

# Clean and prepare
rm -rf /tmp/frostsnap
mkdir -p /tmp/frostsnap

# Copy source to identical location
cp -r . /tmp/frostsnap/
cd /tmp/frostsnap

nix develop --command just build-device v2 --locked
nix develop --command just save-image
