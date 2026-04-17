{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-esp-dev.url = "github:mirrexagon/nixpkgs-esp-dev";
  };

  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, nixpkgs-esp-dev, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs { inherit system; };
      # Override to ESP-IDF v5.3.1 — v5.4+ added esp_app_desc_t efuse_blk_rev validation
      # that rejects bare-metal esp-hal images missing a proper app descriptor.
      esp-idf-riscv = (nixpkgs-esp-dev.packages.${system}.esp-idf-riscv).override {
        rev = "v5.5.4";
        sha256 = "sha256-rItbBrwItkfJf8tKImAQsiXDR95sr0LqaM51gDZG/nI=";
      };
      esp-idf-xtensa = (nixpkgs-esp-dev.packages.${system}.esp-idf-xtensa).override {
        rev = "v5.5.4";
        sha256 = "sha256-rItbBrwItkfJf8tKImAQsiXDR95sr0LqaM51gDZG/nI=";
      };

      buildBootloader = { variant ? "dev", chip ? "esp32c3" }: pkgs.stdenv.mkDerivation {
        name = "frostsnap-bootloader-${variant}-${chip}";
        src = ./.;

        buildInputs = [
          (if chip == "esp32s3" then esp-idf-xtensa else esp-idf-riscv)
        ];

        phases = [ "buildPhase" "installPhase" ];

        buildPhase = ''
          cp -r $src/* .
          chmod -R +w .
          mkdir temp-home
          export HOME=$(readlink -f temp-home)
          export IDF_COMPONENT_MANAGER=0

          DEFAULTS="sdkconfig.defaults"
          ${if variant == "dev" then ''DEFAULTS="$DEFAULTS;sdkconfig.defaults.dev"'' else ""}

          idf.py -DSDKCONFIG_DEFAULTS="$DEFAULTS" set-target ${chip}

          idf.py bootloader
        '';

        installPhase = ''
          mkdir -p $out
          cp build/bootloader/bootloader.bin $out/bootloader.bin
          cp sdkconfig $out/sdkconfig
        '';
      };
    in {
      packages = {
        dev-esp32c3 = buildBootloader { variant = "dev"; chip = "esp32c3"; };
        prod-esp32c3 = buildBootloader { variant = "prod"; chip = "esp32c3"; };
        dev-esp32s3 = buildBootloader { variant = "dev"; chip = "esp32s3"; };
        prod-esp32s3 = buildBootloader { variant = "prod"; chip = "esp32s3"; };

        # Backward compatible aliases (C3)
        dev = self.packages.${system}.dev-esp32c3;
        prod = self.packages.${system}.prod-esp32c3;
        default = self.packages.${system}.dev;
      };

      devShells.default = pkgs.mkShell {
        buildInputs = [ esp-idf-riscv esp-idf-xtensa ];
      };
    });
}
