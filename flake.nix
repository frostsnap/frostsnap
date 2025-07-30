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
        
        # Pin exact Rust version for deterministic builds (use same as CI)
        toolchain = pkgs.rust-bin.stable."1.88.0".default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "riscv32imc-unknown-none-elf" ];
        };
        
        # RISC-V cross-compilation environment for C dependencies
        riscvPkgs = pkgs.pkgsCross.riscv32-embedded;
        
        commonEnv = {
          # Rust environment - strict deterministic settings
          CARGO_BUILD_INCREMENTAL = "false";
          CARGO_BUILD_JOBS = "1";  # Single-threaded for consistency
          CARGO_TARGET_DIR = "./target";
          RUST_BACKTRACE = "1";
          
          # Timezone consistency
          TZ = "UTC";
          LANG = "C.UTF-8";
          LC_ALL = "C.UTF-8";
          
          # C compiler for RISC-V (needed by secp256k1-sys and other C deps)
          CC_riscv32imc_unknown_none_elf = "${riscvPkgs.stdenv.cc}/bin/${riscvPkgs.stdenv.cc.targetPrefix}cc";
          
          # ESP32-C3 specific
          IDF_TARGET = "esp32c3";
          PKG_CONFIG_ALLOW_CROSS = "1";
          
          # Deterministic build settings
          SOURCE_DATE_EPOCH = "1704067200"; # 2024-01-01

          # Ensure consistent hash algorithms and disable randomization
          CARGO_BUILD_RUSTFLAGS = "-C codegen-units=1 -C debuginfo=0 -C strip=symbols";
          
          # Disable build script caching that might vary
          CARGO_BUILD_TARGET_APPLY_TO_HOST = "false";
          
          # Force consistent temporary directory
          TMPDIR = "/tmp";
          
          # Disable incremental compilation completely
          RUSTC_BOOTSTRAP = "1";
          CARGO_INCREMENTAL = "0";
          
          # Consistent umask
          UMASK = "022";

        };
      in 
      {
        # Development shell
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain with RISC-V target
            toolchain
            
            # ESP32 tools
            esptool
            
            # RISC-V GCC for C dependencies
            riscvPkgs.stdenv.cc
            
            # Build tools
            pkg-config
            openssl
            
            # Development utilities
            just
            cargo-watch
            cargo-expand
          ];
          
          # Explicitly set environment variables
          inherit (commonEnv) 
            CARGO_BUILD_INCREMENTAL 
            CARGO_BUILD_JOBS 
            CARGO_TARGET_DIR 
            RUST_BACKTRACE 
            TZ 
            LANG 
            LC_ALL 
            CC_riscv32imc_unknown_none_elf 
            IDF_TARGET 
            PKG_CONFIG_ALLOW_CROSS 
            SOURCE_DATE_EPOCH;
          
          shellHook = ''
            export PATH="${toolchain}/bin:$PATH"
            export RUST_SYSROOT="${toolchain}"
            
            echo "🔥 Frostsnap ESP32-C3 development environment"
            echo "📁 Project: $(basename $PWD)"
            echo "🦀 Rust: $(rustc --version)"
            echo "🔧 Cargo: $(which cargo) - $(cargo --version)"  # Add this to verify
            
                      
            # Verify RISC-V target availability
            if rustup target list --installed | grep -q "riscv32imc-unknown-none-elf"; then
              echo "✅ ESP32-C3 target: available"
            else
              echo "⚠️  Installing ESP32-C3 target..."
              rustup target add riscv32imc-unknown-none-elf
            fi
            
            # Check espflash availability
            if command -v espflash >/dev/null; then
              espflash_version=$(espflash --version | cut -d' ' -f2 2>/dev/null || echo "unknown")
              if [[ "$espflash_version" == "3.2.0" ]]; then
                echo "✅ espflash: $espflash_version"
              else
                echo "⚠️  espflash: $espflash_version (recommend 3.2.0)"
                echo "   Install with: cargo install espflash@3.2.0 --force"
              fi
            else
              echo "⚠️  espflash: not found"
              echo "   Install with: cargo install espflash@3.2.0"
            fi
            
            echo ""
            echo "💡 Available commands:"
            just --list 2>/dev/null || echo "  Run 'just --list' to see available commands"
            echo ""
          '';
        };
        
        # Build package for CI/deterministic builds
        packages.default = pkgs.rustPlatform.buildRustPackage (commonEnv // {
          pname = "frostsnap-device";
          version = "0.1.0";
          
          src = pkgs.lib.cleanSource ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          # Build only the device firmware
          cargoBuildFlags = [ "-p" "frostsnap_device" ];
          
          # Skip tests for embedded builds
          doCheck = false;
          auditable = false;
          
          # Cross-compilation dependencies
          depsBuildBuild = with pkgs; [
            riscvPkgs.stdenv.cc
            pkg-config
          ];
          
          # Ensure target is available during build
          preBuild = ''
            rustup target add riscv32imc-unknown-none-elf
          '';
          
          meta = with pkgs.lib; {
            description = "Frostsnap ESP32-C3 firmware";
            homepage = "https://github.com/frostsnap/frostsnap";
            license = licenses.mit; # or whatever license you use
            platforms = [ system ];
            maintainers = [ ]; # add maintainer info if desired
          };
        });
        
        formatter = pkgs.nixpkgs-fmt;
      });
}