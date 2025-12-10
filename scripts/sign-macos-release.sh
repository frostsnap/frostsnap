#!/bin/bash
set -e

TAG="${1:-}"
if [ -z "$TAG" ]; then
    echo "Usage: $0 <tag>"
    echo "Example: $0 v0.2.0"
    exit 1
fi

WORK_DIR=$(mktemp -d)
trap "rm -rf $WORK_DIR" EXIT
cd "$WORK_DIR"

echo "==> Downloading Frostsnap-mac.app.zip from $TAG..."
gh release download "$TAG" --pattern "Frostsnap-mac.app.zip"
unzip -q Frostsnap-mac.app.zip

echo ""
echo "==> Available signing identities:"
security find-identity -v -p codesigning | grep "Developer ID Application" || true
echo ""
read -p "Enter signing identity (name or hash): " IDENTITY

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENTITLEMENTS="$SCRIPT_DIR/frostsnapp/macos/Runner/Release.entitlements"

echo ""
echo "==> Signing with Hardened Runtime..."
codesign --sign "$IDENTITY" \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    --force --deep Frostsnap.app

echo ""
echo "==> Verifying signature..."
codesign -dvv Frostsnap.app 2>&1 | head -10

echo ""
echo "==> Zipping for notarization..."
zip -ryq Frostsnap-notarize.zip Frostsnap.app

echo ""
read -p "Apple ID email: " APPLE_ID
read -p "Team ID [RGC8S78Z5K]: " TEAM_ID
TEAM_ID="${TEAM_ID:-RGC8S78Z5K}"
read -sp "App-specific password: " APP_PASSWORD
echo ""

echo ""
echo "==> Submitting for notarization (this may take a few minutes)..."
xcrun notarytool submit Frostsnap-notarize.zip \
    --apple-id "$APPLE_ID" \
    --team-id "$TEAM_ID" \
    --password "$APP_PASSWORD" \
    --wait

echo ""
echo "==> Stapling notarization ticket..."
xcrun stapler staple Frostsnap.app

echo ""
echo "==> Creating signed zip..."
zip -ryq Frostsnap-mac-signed.app.zip Frostsnap.app

echo ""
echo "==> Uploading to release $TAG..."
gh release upload "$TAG" Frostsnap-mac-signed.app.zip --clobber

echo ""
echo "Done! Frostsnap-mac-signed.app.zip uploaded to $TAG"
