{
  description = "Frostsnap ESP32-C3 firmware with deterministic builds";
  
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };
  
  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    let
      versions = {
        rust = "1.88.0";
        riscv-gcc = "2024.09.03"; # Should match justfetch.lock version!
      };
    in
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        
        toolchain = pkgs.rust-bin.stable.${versions.rust}.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
          targets = [ "riscv32imc-unknown-none-elf" ];
        };
        
        riscvPkgs = pkgs.pkgsCross.riscv32-embedded;
        
      in 
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            toolchain
            riscvPkgs.stdenv.cc
            just
            esptool
            openssl
          ];
          
          shellHook = ''
            # Version info
            echo "Toolchain versions:"
            echo "Rust: ${versions.rust}"
            echo "RISC-V GCC target: ${versions.riscv-gcc} (justfetch), $(riscv32-unknown-elf-gcc --version | head -1) (nix)"
            echo

            # Deterministic environment settings
            export SOURCE_DATE_EPOCH="1704067200"
            export CARGO_BUILD_INCREMENTAL="false"
            export CARGO_BUILD_JOBS="1"
            export CARGO_INCREMENTAL="0"
            export LC_ALL="C.UTF-8"
            export TZ="UTC"
            
            # Force identical cargo home for all environments
            export CARGO_HOME="/tmp/deterministic-cargo"
            mkdir -p "$CARGO_HOME"
            
            # Simple path remapping - now cargo paths will be identical
            export CARGO_BUILD_RUSTFLAGS="--remap-path-prefix=/tmp/deterministic-cargo=CARGO_HOME --remap-path-prefix=/nix/store=NIXSTORE --remap-path-prefix=/tmp=TMP --remap-path-prefix=/home=HOME -Ccodegen-units=1 -Cdebuginfo=0 -Cstrip=symbols -Cllvm-args=-disable-symbolication -Clink-arg=--sort-section=name -Clink-arg=--build-id=none -Clink-arg=--hash-style=gnu"
            
            echo "Rust build environment:"
            echo "CARGO_HOME: $CARGO_HOME"
            echo "RUSTFLAGS: $CARGO_BUILD_RUSTFLAGS"
          '';
          
          # ESP32 stuff
          CC_riscv32imc_unknown_none_elf = "${riscvPkgs.stdenv.cc}/bin/${riscvPkgs.stdenv.cc.targetPrefix}cc";
          IDF_TARGET = "esp32c3";
          PKG_CONFIG_ALLOW_CROSS = "1";
        };
        
        formatter = pkgs.nixpkgs-fmt;
      });
}