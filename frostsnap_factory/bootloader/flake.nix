{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-esp-dev.url = "github:mirrexagon/nixpkgs-esp-dev";
  };

  outputs = { self, nixpkgs, nixpkgs-esp-dev }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs { inherit system; };
    # Override to ESP-IDF v5.3.1 — v5.4+ added esp_app_desc_t efuse_blk_rev validation
    # that rejects bare-metal esp-hal images missing a proper app descriptor.
    esp-idf-riscv = (nixpkgs-esp-dev.packages.${system}.esp-idf-riscv).override {
      rev = "v5.3.1";
      sha256 = "sha256-hcE4Tr5PTRQjfiRYgvLB1+8sR7KQQ1TnQJqViodGdBw=";
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
    packages.${system} = {
      dev = buildBootloader { variant = "dev"; };
      prod = buildBootloader { variant = "prod"; };
      default = self.packages.${system}.dev;
    };

    devShells.${system}.default = pkgs.mkShell {
      buildInputs = [ esp-idf-riscv ];
    };
  };
}
