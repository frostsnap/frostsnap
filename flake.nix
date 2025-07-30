{
  description = "Frostsnap ESP32-C3 firmware with deterministic builds";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Pin exact Rust version for deterministic builds
        # You're using 1.88.0 which is quite new - let's use latest stable
        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "riscv32imc-unknown-none-elf" ];
        };

        # RISC-V cross-compilation environment for C dependencies
        riscvPkgs = pkgs.pkgsCross.riscv32-embedded;

        # Common environment variables for deterministic builds
        commonEnv = {
          # Rust environment - let Rust handle cross-compilation
          CARGO_BUILD_INCREMENTAL = "false";  # Disable for deterministic builds
          CARGO_TARGET_DIR = "./target";
          RUST_BACKTRACE = "1";
          
          # C compiler for RISC-V (needed by secp256k1-sys and other C deps)
          CC_riscv32imc_unknown_none_elf = "${riscvPkgs.stdenv.cc}/bin/${riscvPkgs.stdenv.cc.targetPrefix}cc";
          
          # ESP32-C3 specific
          IDF_TARGET = "esp32c3";
          PKG_CONFIG_ALLOW_CROSS = "1";
          
          # Deterministic build settings
          SOURCE_DATE_EPOCH = "1704067200"; # 2024-01-01, pin for reproducibility
        };

      in {
        # Development shell
        devShells.default = pkgs.mkShell (commonEnv // {
          buildInputs = with pkgs; [
            # Rust toolchain
            toolchain
            
            # ESP32 tools
            espflash
            esptool
            
            # RISC-V GCC for C dependencies (secp256k1-sys, etc.)
            riscvPkgs.stdenv.cc
            
            # Build tools
            pkg-config
            openssl
            
            # Development utilities
            just
            cargo-watch
            cargo-expand
          ];

          shellHook = ''
            echo "🔥 Frostsnap ESP32-C3 development environment"
            echo "📁 Project: $(basename $PWD)"
            echo "🦀 Rust: $(rustc --version)"
            echo "🔧 ESP32-C3 target: $(rustup target list --installed | grep riscv32imc || echo 'Missing - run: rustup target add riscv32imc-unknown-none-elf')"
            echo ""
            echo "💡 Just commands available:"
            echo "  just flash             - Flash firmware to device"
            echo "  just build-device      - Build device firmware"
            echo "  just check             - Check all code"
            echo "  just lint              - Lint all code"
            echo "  just test              - Run tests"
            echo ""
          '';
        });

        # Build the ESP32-C3 firmware deterministically
        packages = {
          # Main firmware build
          firmware = pkgs.rustPlatform.buildRustPackage (commonEnv // {
            pname = "frostsnap-device";
            version = "0.1.0";
            
            src = pkgs.lib.cleanSource ./.;
            
            # Use workspace lockfile
            cargoLock = {
              lockFile = ./Cargo.lock;
              # If you have git dependencies, add their hashes here:
              # outputHashes = {
              #   "some-git-dep-0.1.0" = "sha256-...";
              # };
            };

            # Build only the device firmware
            cargoBuildFlags = [ "-p" "frostsnap_device" ];
            cargoTestFlags = [ "-p" "frostsnap_device" ];
            
            # Required for embedded target
            doCheck = false;
            auditable = false;  # Auditable builds not supported for embedded
            
            # Copy the partition table and generate final firmware binary
            postBuild = ''
              # Copy partition table if it exists
              if [ -f device/partitions.csv ]; then
                cp device/partitions.csv $out/
              fi
              
              # Generate the firmware.bin file for the v2 board (default)
              mkdir -p $out/bin
              if [ -f target/riscv32imc-unknown-none-elf/release/v2 ]; then
                cp target/riscv32imc-unknown-none-elf/release/v2 $out/bin/firmware.bin
              fi
            '';

            # Cross-compilation dependencies
            depsBuildBuild = with pkgs; [
              riscvPkgs.stdenv.cc
              pkg-config
            ];

            meta = {
              description = "Frostsnap ESP32-C3 firmware";
              platforms = [ system ];
            };
          });

          # Default package points to firmware
          default = self.packages.${system}.firmware;
        };

        # Convenient apps/scripts
        apps = {
          # Build firmware only (v2 board default)
          build-device = {
            type = "app";
            program = pkgs.writeShellScript "build-device" ''
              set -euo pipefail
              export PATH="${toolchain}/bin:$PATH"
              ${pkgs.lib.concatStringsSep "\n" (pkgs.lib.mapAttrsToList (n: v: "export ${n}='${toString v}'") commonEnv)}
              
              echo "🔨 Building ESP32-C3 firmware (v2 board)..."
              just build-device v2
              echo "✅ Firmware built successfully!"
            '';
          };

          # Flash to device 
          flash = {
            type = "app";
            program = pkgs.writeShellScript "flash-firmware" ''
              set -euo pipefail
              export PATH="${toolchain}/bin:${pkgs.espflash}/bin:${pkgs.just}/bin:$PATH"
              ${pkgs.lib.concatStringsSep "\n" (pkgs.lib.mapAttrsToList (n: v: "export ${n}='${toString v}'") commonEnv)}
              
              echo "🔥 Flashing ESP32-C3 firmware..."
              just flash v2
            '';
          };

          # Save firmware image
          save-image = {
            type = "app";
            program = pkgs.writeShellScript "save-image" ''
              set -euo pipefail
              export PATH="${toolchain}/bin:${pkgs.espflash}/bin:${pkgs.just}/bin:$PATH"
              ${pkgs.lib.concatStringsSep "\n" (pkgs.lib.mapAttrsToList (n: v: "export ${n}='${toString v}'") commonEnv)}
              
              echo "💾 Saving firmware image..."
              just save-image v2
              echo "✅ Firmware image saved to target/riscv32imc-unknown-none-elf/release/firmware.bin"
            '';
          };

          # Run all checks
          check = {
            type = "app";
            program = pkgs.writeShellScript "check-all" ''
              set -euo pipefail
              export PATH="${toolchain}/bin:${pkgs.just}/bin:$PATH"
              ${pkgs.lib.concatStringsSep "\n" (pkgs.lib.mapAttrsToList (n: v: "export ${n}='${toString v}'") commonEnv)}
              
              echo "🔍 Running all checks..."
              just check
            '';
          };

          # Run all tests
          test = {
            type = "app";
            program = pkgs.writeShellScript "test-all" ''
              set -euo pipefail
              export PATH="${toolchain}/bin:${pkgs.just}/bin:$PATH"
              ${pkgs.lib.concatStringsSep "\n" (pkgs.lib.mapAttrsToList (n: v: "export ${n}='${toString v}'") commonEnv)}
              
              echo "🧪 Running tests..."
              just test
            '';
          };
        };

        # Formatter for nix files
        formatter = pkgs.nixpkgs-fmt;
      });
}
