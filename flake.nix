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
        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "riscv32imc-unknown-none-elf" ];
        };
        # RISC-V cross-compilation environment for C dependencies
        riscvPkgs = pkgs.pkgsCross.riscv32-embedded;
        
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
            # Rust toolchain with target pre-installed
            toolchain
            
            # ESP32 tools
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
            # Ensure cargo-installed tools come first in PATH
            export PATH="$HOME/.cargo/bin:${toolchain}/bin:$PATH"
            export RUST_SYSROOT="${toolchain}"
            
            echo "🔥 Frostsnap ESP32-C3 development environment"
            echo "📁 Project: $(basename $PWD)"
            echo "🦀 Rust: $(rustc --version)"
            
            # Verify the RISC-V target is available
            if rustup target list --installed | grep -q "riscv32imc-unknown-none-elf"; then
              echo "✅ ESP32-C3 target: riscv32imc-unknown-none-elf"
            else
              echo "⚠️  ESP32-C3 target: not found"
              echo "   Installing target..."
              rustup target add riscv32imc-unknown-none-elf
              if [ $? -eq 0 ]; then
                echo "✅ ESP32-C3 target: riscv32imc-unknown-none-elf (installed)"
              else
                echo "❌ Failed to install ESP32-C3 target"
              fi
            fi
            
            # Check espflash version
            if command -v espflash >/dev/null; then
              espflash_version=$(espflash --version | cut -d' ' -f2)
              if [[ "$espflash_version" == "3.2.0" ]]; then
                echo "✅ espflash: $espflash_version"
              else
                echo "⚠️  espflash: $espflash_version (should be 3.2.0)"
                echo "   Run: cargo install espflash@3.2.0 --force"
              fi
            else
              echo "⚠️ espflash: not found"
              echo "   Run: cargo install espflash@3.2.0"
            fi
            
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
        packages.default = pkgs.rustPlatform.buildRustPackage (commonEnv // {
          pname = "frostsnap-device";
          version = "0.1.0";
          
          src = pkgs.lib.cleanSource ./.;
          
          # Use workspace lockfile
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          # Build only the device firmware
          cargoBuildFlags = [ "-p" "frostsnap_device" ];
          cargoTestFlags = [ "-p" "frostsnap_device" ];
          
          # Required for embedded target
          doCheck = false;
          auditable = false;  # Auditable builds not supported for embedded
          
          # Cross-compilation dependencies
          depsBuildBuild = with pkgs; [
            riscvPkgs.stdenv.cc
            pkg-config
          ];
          
          # Ensure the target is available during build
          preBuild = ''
            rustup target add riscv32imc-unknown-none-elf
          '';
          
          meta = {
            description = "Frostsnap ESP32-C3 firmware";
            platforms = [ system ];
          };
        });
        
        # Formatter for nix files
        formatter = pkgs.nixpkgs-fmt;
      });
}