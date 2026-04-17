import 'fetch.just'

default_board := "frontier"
device_crates := "-p frostsnap_device -p frostsnap_cst816s"

# Override with: just env=prod <recipe>
env := "dev"
bootloader_dir := "frostsnap_factory/bootloader"
genuine_dir := "frostsnap_factory/genuine"
secure_boot_key := bootloader_dir / env / "secure-boot-key.pem"
bootloader_bin := bootloader_dir / env / "signed-bootloader.bin"
firmware_bin := "target/riscv32imc-unknown-none-elf/release/" + env + "-frontier.bin"
partitions_csv := "device/partitions.csv"
partitions_csv_esp32s3 := "device/partitions-esp32s3.csv"
app_api_dir := "frostsnap/rust/src/api"
app_canary := "frostsnapp/binding-rerun.sha256"

alias erase := erase-device
alias demo := simulate

# Build and flash signed firmware only (no bootloader)
flash BOARD=default_board +ARGS="":
    just env={{env}} build-firmware-signed {{BOARD}}
    just env={{env}} flash-firmware {{ARGS}}

# Flash signed firmware to device (firmware + otadata only)
flash-firmware +ARGS="":
    #!/bin/sh
    set -e
    ADDR_OTADATA=$(awk -F, '$1 == "otadata" { gsub(/ /, "", $4); print $4 }' {{partitions_csv}})
    ADDR_APP=$(awk -F, '$1 == "ota_0" { gsub(/ /, "", $4); print $4 }' {{partitions_csv}})
    [ -n "$ADDR_OTADATA" ] || { echo "Failed to find otadata offset in {{partitions_csv}}" >&2; exit 1; }
    [ -n "$ADDR_APP" ] || { echo "Failed to find ota_0 offset in {{partitions_csv}}" >&2; exit 1; }
    for f in device/blank-otadata.bin "{{firmware_bin}}"; do
        [ -f "$f" ] || { echo "Missing: $f" >&2; exit 1; }
    done
    flash() { espflash write-bin --chip esp32c3 --baud 921600 --no-stub "$@"; }
    if [ "{{ARGS}}" = "--table" ]; then
        printf "%-12s  %s\n" "Address" "Component"
        printf "%-12s  %s\n" "$ADDR_OTADATA" "device/blank-otadata.bin"
        printf "%-12s  %s\n" "$ADDR_APP" "{{firmware_bin}}"
        exit 0
    fi
    flash $ADDR_OTADATA    device/blank-otadata.bin   {{ARGS}}
    flash $ADDR_APP        "{{firmware_bin}}"         {{ARGS}}

# Flash bootloader + partitions + firmware (for initial secure boot setup)
flash-bootloader CHIP="esp32c3" +ARGS="":
    #!/bin/sh
    set -e
    BOOT_DIR="{{bootloader_dir}}/{{env}}-{{CHIP}}"
    UNSIGNED_BOOTLOADER="$BOOT_DIR/bootloader.bin"
    SIGNED_BOOTLOADER="$BOOT_DIR/signed-bootloader.bin"
    SDKCONFIG="$BOOT_DIR/sdkconfig"
    [ -f "$SDKCONFIG" ] || { echo "Missing: $SDKCONFIG. Run 'just env={{env}} build-bootloader {{CHIP}}' first." >&2; exit 1; }
    ADDR_PARTITIONS=$(awk -F= '$1 == "CONFIG_PARTITION_TABLE_OFFSET" { gsub(/"/, "", $2); print $2 }' "$SDKCONFIG")
    [ -n "$ADDR_PARTITIONS" ] || { echo "Failed to find CONFIG_PARTITION_TABLE_OFFSET in $SDKCONFIG" >&2; exit 1; }
    if [ "{{env}}" = "prod" ]; then
        BOOTLOADER_IMAGE="$SIGNED_BOOTLOADER"
    else
        BOOTLOADER_IMAGE="$UNSIGNED_BOOTLOADER"
    fi
    for f in "$BOOTLOADER_IMAGE" device/partitions.bin; do
        [ -f "$f" ] || { echo "Missing: $f" >&2; exit 1; }
    done
    flash() { espflash write-bin --chip {{CHIP}} --baud 921600 --no-stub "$@"; }
    if [ "{{ARGS}}" = "--table" ]; then
        printf "%-12s  %s\n" "Address" "Component"
        printf "%-12s  %s\n" "0x0" "$BOOTLOADER_IMAGE"
        printf "%-12s  %s\n" "$ADDR_PARTITIONS" "device/partitions.bin"
        exit 0
    fi
    flash 0x0              "$BOOTLOADER_IMAGE"        {{ARGS}}
    flash $ADDR_PARTITIONS device/partitions.bin      {{ARGS}}

# Full provision: build + flash bootloader + firmware, then provision device
full-provision COLOR BOARD=default_board CHIP="esp32c3" +ARGS="":
    just env={{env}} build-bootloader {{CHIP}}
    just env={{env}} sign-bootloader {{CHIP}}
    just env={{env}} build-firmware-signed {{BOARD}}
    just env={{env}} flash-bootloader {{CHIP}} {{ARGS}}
    just env={{env}} flash-firmware {{ARGS}}
    just env={{env}} provision {{COLOR}}

# Flash unsigned firmware to a legacy device (no secure boot)
legacy-flash BOARD="legacy" +ARGS="":
    cd device && cargo run --release --bin {{BOARD}} {{ARGS}} -- --erase-parts otadata,ota_0

monitor +ARGS="":
    espflash monitor --no-stub

erase-device CHIP="esp32c3" +ARGS="nvs":
    #!/bin/sh
    set -e
    case "{{CHIP}}" in
      esp32c3) PARTITIONS="{{partitions_csv}}" ;;
      esp32s3) PARTITIONS="{{partitions_csv_esp32s3}}" ;;
      *) echo "Unknown CHIP='{{CHIP}}'. Expected esp32c3 or esp32s3." >&2; exit 1 ;;
    esac
    cd device && espflash erase-parts --partition-table "$PARTITIONS" {{ARGS}}

build-firmware BOARD=default_board +ARGS="":
    cd device && cargo build --release --bin {{BOARD}} ${DEVICE_BUILD_ARGS:-} {{ARGS}}

build-firmware-signed BOARD=default_board OUTPUT=firmware_bin +ARGS="":
    just build-firmware {{BOARD}} {{ARGS}}
    just save-image {{BOARD}}
    just env={{env}} sign-firmware target/riscv32imc-unknown-none-elf/release/{{BOARD}}.bin {{OUTPUT}}

# Build unsigned bootloader via Nix (no signing key needed)
build-bootloader CHIP="esp32c3":
    cd {{bootloader_dir}} && nix build .#{{env}}-{{CHIP}}
    mkdir -p {{bootloader_dir}}/{{env}}-{{CHIP}}
    cp -f {{bootloader_dir}}/result/bootloader.bin {{bootloader_dir}}/{{env}}-{{CHIP}}/bootloader.bin
    cp -f {{bootloader_dir}}/result/sdkconfig {{bootloader_dir}}/{{env}}-{{CHIP}}/sdkconfig

# Sign the unsigned bootloader with the secure boot key
sign-bootloader CHIP="esp32c3":
    #!/bin/sh
    set -e
    INPUT="{{bootloader_dir}}/{{env}}-{{CHIP}}/bootloader.bin"
    OUTPUT="{{bootloader_dir}}/{{env}}-{{CHIP}}/signed-bootloader.bin"
    [ -f "$INPUT" ] || { echo "Missing: $INPUT. Run 'just env={{env}} build-bootloader {{CHIP}}' first." >&2; exit 1; }
    if [ "{{env}}" = "dev" ]; then
        echo "Skipping bootloader signing for dev environment by design."
        exit 0
    fi
    just env={{env}} sign-firmware "$INPUT" "$OUTPUT"

# Generate all keys for an environment
gen-keys:
    mkdir -p {{bootloader_dir}}/{{env}}
    mkdir -p {{genuine_dir}}/{{env}}
    cargo run -p frostsnap_factory -- gen-secure-boot-key -o {{secure_boot_key}}
    cargo run -p frostsnap_factory -- gen-genuine-cert-key -o {{genuine_dir}}/{{env}}

# Provision a single device (no database)
provision COLOR:
    cargo run -p frostsnap_factory -- provision {{COLOR}} --env {{env}}

# Verify a connected device's genuine certificate
genuine-check:
    cargo run -p frostsnap_factory -- genuine-check

build-deterministic:
    cd device && ./deterministic-build.sh

save-image BOARD=default_board +ARGS="":
    espflash save-image --chip=esp32c3 target/riscv32imc-unknown-none-elf/release/{{BOARD}} target/riscv32imc-unknown-none-elf/release/{{BOARD}}.bin {{ARGS}}

# Sign a firmware binary for secure boot
sign-firmware INPUT="target/riscv32imc-unknown-none-elf/release/frontier.bin" OUTPUT=firmware_bin:
    cargo run -p frostsnap_factory -- sign-firmware -i {{INPUT}} -o {{OUTPUT}} -k {{secure_boot_key}}

# --- App build ---

get-build-commit:
    #!/bin/sh
    BUILD_COMMIT=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
    if [ "$BUILD_COMMIT" != "unknown" ] && ! git diff-index --quiet HEAD --; then
        BUILD_COMMIT="${BUILD_COMMIT}-modified"
    fi
    echo "$BUILD_COMMIT"

get-build-version:
    #!/bin/sh
    BUILD_VERSION=$(git describe --tags --always 2>/dev/null || echo "unknown")
    if [ "$BUILD_VERSION" != "unknown" ] && ! git diff-index --quiet HEAD --; then
        BUILD_VERSION="${BUILD_VERSION}-modified"
    fi
    echo "$BUILD_VERSION"

gen +ARGS="":
    cd frostsnapp && flutter_rust_bridge_codegen generate {{ARGS}}
    find frostsnapp/rust/src/api -type f -exec sha256sum {} + > {{app_canary}}

build-runner:
    cd frostsnapp && dart run build_runner build --delete-conflicting-outputs

maybe-gen:
    #!/bin/sh
    if ! sha256sum --check {{app_canary}} >/dev/null 2>&1 ; then
       echo "{{app_canary}} changed so re-running bindgen">&2;
       just gen
    fi
    just build-runner

build TARGET="linux" +ARGS="": maybe-gen
    #!/bin/sh
    BUILD_COMMIT=$(just get-build-commit)
    BUILD_VERSION=$(just get-build-version)
    cd frostsnapp && FROSTSNAP_ENV={{env}} BUNDLE_FIRMWARE=1 \
      flutter build {{TARGET}} --dart-define=BUILD_COMMIT="$BUILD_COMMIT" --dart-define=BUILD_VERSION="$BUILD_VERSION" {{ARGS}}

run +ARGS="": maybe-gen
    #!/bin/sh
    set -e
    just env={{env}} build-firmware-signed
    BUILD_COMMIT=$(just get-build-commit)
    BUILD_VERSION=$(just get-build-version)
    FLAVOR_FLAG=""
    if [ "$(uname)" != "Darwin" ]; then
        FLAVOR_FLAG="--flavor direct"
    fi
    cd frostsnapp && FROSTSNAP_ENV={{env}} BUNDLE_FIRMWARE=1 \
      flutter run $FLAVOR_FLAG --dart-define=BUILD_COMMIT="$BUILD_COMMIT" --dart-define=BUILD_VERSION="$BUILD_VERSION" {{ARGS}}

# Run the app with unsigned legacy firmware (for legacy devices)
legacy-run +ARGS="": maybe-gen
    #!/bin/sh
    set -e
    just build-firmware legacy
    just save-image legacy
    BUILD_COMMIT=$(just get-build-commit)
    BUILD_VERSION=$(just get-build-version)
    FLAVOR_FLAG=""
    if [ "$(uname)" != "Darwin" ]; then
        FLAVOR_FLAG="--flavor direct"
    fi
    cd frostsnapp && BUNDLE_FIRMWARE=../target/riscv32imc-unknown-none-elf/release/legacy.bin \
      flutter run $FLAVOR_FLAG --dart-define=BUILD_COMMIT="$BUILD_COMMIT" --dart-define=BUILD_VERSION="$BUILD_VERSION" {{ARGS}}

build-appimage +ARGS="":
    #!/bin/sh
    if [ ! -d "frostsnapp/build/linux/x64/release/bundle" ]; then
        echo "ERROR: Flutter build not found at frostsnapp/build/linux/x64/release/bundle" >&2
        echo "Run 'just build linux --release' first" >&2
        exit 1
    fi
    dir="frostsnapp/appimage/AppDir"
    executable="$dir/usr/Frostsnap"
    rm -rf "$dir"
    mkdir -p "$dir/usr"
    cp -r frostsnapp/appimage/assets/* "$dir/"
    cp -rp frostsnapp/build/linux/x64/release/bundle/* $dir/usr
    chmod u+x "$executable" || exit 1;
    echo "final bundled libs:" >&2;
    ls -l $dir/usr/* >&2
    just fetch https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20250213-2/linuxdeploy-x86_64.AppImage frostsnapp/appimage/linuxdeploy
    chmod u+x frostsnapp/appimage/linuxdeploy
    export LD_LIBRARY_PATH="$dir/usr/lib:${LD_LIBRARY_PATH:-}"
    frostsnapp/appimage/linuxdeploy \
    --appdir $dir \
    --desktop-file "$dir/com.frostsnapp.app.desktop" \
    --icon-file "$dir/Frostsnap.png" \
    --executable "$executable" \
    --output appimage

# Create DMG from an existing .app bundle
package-dmg APP_PATH="frostsnapp/build/macos/Build/Products/Release/Frostsnap.app":
    #!/bin/sh
    set -e
    APP_PATH="{{APP_PATH}}"
    APP_NAME="Frostsnap"
    DMG_NAME="${APP_NAME}.dmg"
    if [ ! -d "$APP_PATH" ]; then
        echo "ERROR: App not found at $APP_PATH" >&2
        exit 1
    fi
    rm -f "$DMG_NAME"
    if ! command -v create-dmg &> /dev/null; then
        echo "ERROR: create-dmg is required but not installed" >&2
        echo "Install with: brew install create-dmg" >&2
        exit 1
    fi
    create-dmg \
      --volname "${APP_NAME}" \
      --volicon "frostsnapp/macos/Runner/Assets.xcassets/AppIcon.appiconset/app_icon_1024.png" \
      --background "frostsnapp/macos/dmg-background.png" \
      --window-pos 200 120 \
      --window-size 600 400 \
      --icon-size 100 \
      --icon "${APP_NAME}.app" 150 185 \
      --hide-extension "${APP_NAME}.app" \
      --app-drop-link 450 185 \
      --no-internet-enable \
      --hdiutil-quiet \
      "$DMG_NAME" \
      "$APP_PATH"
    echo "Created DMG: $DMG_NAME" >&2
    ls -lh "$DMG_NAME" >&2

# Build macOS app and package into DMG
build-dmg:
    just build macos --release
    just package-dmg

# --- Testing & linting ---

test-ordinary +ARGS="":
    cargo test {{ARGS}}

test: test-ordinary

check-ordinary +ARGS="":
    cargo check {{ARGS}} --all-features --tests --bins

check-device +ARGS="":
    cargo check --target riscv32imc-unknown-none-elf {{device_crates}} {{ARGS}} --all-features

lint-ordinary +ARGS="":
    cargo fmt -- --check
    cargo clippy {{ARGS}} --all-features --tests --bins -- -Dwarnings

lint-device +ARGS="":
    cargo fmt {{device_crates}} -- --check
    cargo clippy {{device_crates}} --target riscv32imc-unknown-none-elf  {{ARGS}} --all-features -- -Dwarnings

dart-format-check-app:
    ( cd frostsnapp; dart format --set-exit-if-changed --output=none  $(find ./lib -type f -name "*.dart" -not -path "./lib/src/rust/*" -not -name "*.freezed.dart") )

lint-app +ARGS="": maybe-gen dart-format-check-app
    ( cd frostsnapp; flutter analyze {{ARGS}} )

fix-dart: maybe-gen
    ( cd frostsnapp && dart format $(find ./lib -type f -name "*.dart" -not -path "./lib/src/rust/*" -not -name "*.freezed.dart") && dart fix --apply && flutter analyze )

fix: fix-dart fix-rust

fix-rust:
    cargo clippy --fix --allow-dirty --allow-staged --all-features --tests --bins
    cargo clippy --fix --allow-dirty --target riscv32imc-unknown-none-elf --allow-staged {{device_crates}} --all-features
    cargo fmt --all

check: check-ordinary check-device
lint: lint-ordinary lint-device lint-app

# --- Misc ---

flutter-rust-bridge-version:
    #!/bin/sh
    cd frostsnapp && flutter_rust_bridge_version=$(cargo tree --prefix none | sed -n 's/flutter_rust_bridge v//p')
    echo $flutter_rust_bridge_version

install-cargo-bins:
    #!/bin/sh
    flutter_rust_bridge_version=$(just flutter-rust-bridge-version)
    if ! test "$flutter_rust_bridge_version"; then
        echo "couldn't determine version for flutter_rust_bridge" >&2;
        exit 1
    fi
    cargo install cargo-ndk@3.5.4 espflash@3.2.0 cargo-expand@1.0.95 "flutter_rust_bridge_codegen@$flutter_rust_bridge_version"

fetch-riscv VERSION="2024.09.03-nightly":
    #!/bin/sh
    version="riscv32-elf-ubuntu-22.04-gcc-nightly-{{VERSION}}.tar.gz"
    just fetch https://github.com/riscv-collab/riscv-gnu-toolchain/releases/download/2024.09.03/$version "$version"
    tar -zxf "$version" && rm "$version"

backup +ARGS="":
    cargo run --release --bin frost_backup -- {{ARGS}}

simulate +ARGS="":
    (cd widget_simulator && cargo run -- {{ARGS}}; )

widget_dev DEMO="screen_test" CHIP="esp32c3" +ARGS="":
    #!/bin/sh
    set -e
    case "{{CHIP}}" in
      esp32c3)
        TARGET="riscv32imc-unknown-none-elf"
        CHIP_FEATURES="--features chip-esp32c3"
        PARTITIONS_CSV="device/partitions.csv"
        PT_OFFSET="0xD000"
        TOOLCHAIN=""
        ;;
      esp32s3)
        TARGET="xtensa-esp32s3-none-elf"
        CHIP_FEATURES="--no-default-features --features chip-esp32s3"
        PARTITIONS_CSV="device/partitions-esp32s3.csv"
        PT_OFFSET="0x8000"
        TOOLCHAIN="+esp"
        ;;
      *)
        echo "Unknown CHIP='{{CHIP}}'. Expected esp32c3 or esp32s3." >&2
        exit 1
        ;;
    esac
    (cd device && ESP_BOOTLOADER_ESP_IDF_CONFIG_PARTITION_TABLE_OFFSET="$PT_OFFSET" DEMO={{DEMO}} cargo ${TOOLCHAIN} build --bin widget_dev --release --target "$TARGET" $CHIP_FEATURES ${DEVICE_BUILD_ARGS:-} {{ARGS}})
    espflash save-image --chip={{CHIP}} target/"$TARGET"/release/widget_dev target/"$TARGET"/release/widget_dev.bin
    just env={{env}} sign-firmware target/"$TARGET"/release/widget_dev.bin target/"$TARGET"/release/{{env}}-widget_dev.bin
    ADDR_OTADATA=$(awk -F, '$1 == "otadata" { gsub(/ /, "", $4); print $4 }' "$PARTITIONS_CSV")
    ADDR_APP=$(awk -F, '$1 == "ota_0" { gsub(/ /, "", $4); print $4 }' "$PARTITIONS_CSV")
    espflash write-bin --chip {{CHIP}} --baud 921600 --no-stub $ADDR_OTADATA device/blank-otadata.bin {{ARGS}}
    espflash write-bin --chip {{CHIP}} --baud 921600 --no-stub $ADDR_APP target/"$TARGET"/release/{{env}}-widget_dev.bin {{ARGS}}

clean:
    cd frostsnapp && flutter clean
    cd frostsnapp/rust && cargo clean
    cargo clean
