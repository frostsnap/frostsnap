{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-esp-dev.url = "github:mirrexagon/nixpkgs-esp-dev";
  };

  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, nixpkgs-esp-dev, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs { inherit system; };
      # Pin ESP-IDF to v5.5.4. v5.4+ enforces esp_app_desc_t efuse_blk_rev
      # validation, rejecting images without a proper app descriptor.
      # FIXME: our images currently lack one. esp_app_desc!() emits into
      # .rodata_desc.appdesc but esp-hal 1.1's linker only KEEPs .flash.appdesc,
      # so the descriptor is GC'd — a freshly-built v5.5.4 bootloader would reject
      # them. Tracked for a follow-up.
      esp-idf-riscv = (nixpkgs-esp-dev.packages.${system}.esp-idf-riscv).override {
        rev = "v5.5.4";
        sha256 = "sha256-rItbBrwItkfJf8tKImAQsiXDR95sr0LqaM51gDZG/nI=";
      };

      buildBootloader = { variant ? "dev" }: pkgs.stdenv.mkDerivation {
        name = "frostsnap-bootloader-${variant}";
        src = ./.;

        buildInputs = [ esp-idf-riscv ];

        phases = [ "buildPhase" "installPhase" ];

        buildPhase = ''
          cp -r $src/* .
          chmod -R +w .
          mkdir temp-home
          export HOME=$(readlink -f temp-home)
          export IDF_COMPONENT_MANAGER=0

          DEFAULTS="sdkconfig.defaults"
          ${if variant == "dev" then ''DEFAULTS="sdkconfig.defaults;sdkconfig.defaults.dev"'' else ""}

          idf.py -DSDKCONFIG_DEFAULTS="$DEFAULTS" set-target esp32c3

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
        dev = buildBootloader { variant = "dev"; };
        prod = buildBootloader { variant = "prod"; };
        default = self.packages.${system}.dev;
      };

      devShells.default = pkgs.mkShell {
        buildInputs = [ esp-idf-riscv ];
      };
    });
}
