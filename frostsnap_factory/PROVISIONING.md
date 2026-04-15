# Device Provisioning

End-to-end guide for setting up keys, building firmware, and flashing a device with ESP32 Secure Boot v2.

Everything is controlled by the `env` [just](https://github.com/casey/just) variable: `dev` (default) or `prod`. Override it with `just env=prod <recipe>`.

- **Dev**: JTAG stays enabled, `SECURE_BOOT_INSECURE` allows reflashing freely.
- **Prod**: JTAG permanently disabled on first boot, full secure boot enforcement.

## Prerequisites

- [Nix](https://nixos.org/download/) with flakes enabled
- Rust toolchain with `riscv32imc-unknown-none-elf` target
- `espflash` installed (`cargo install espflash`)

## 1. Generate keys 

**NOTE:** Dev keys are already committed to the repo — you only need this for a fresh prod setup.

```bash
just gen-keys
```

This creates two sets of keys:

**`frostsnap_factory/bootloader/dev/`** — Secure Boot keys:

| File | Purpose |
|------|---------|
| `secure-boot-key.pem` | RSA-3072 private key for signing bootloader and firmware (ESP32 Secure Boot v2) |

**`frostsnap_factory/genuine/dev/`** — Genuine device certificate keys:

| File | Purpose |
|------|---------|
| `secret_key.hex` | Schnorr secret key for signing genuine device certificates during factory provisioning |
| `public_key.hex` | Schnorr public key compiled into the app to verify genuine certificates |

For production: `just env=prod gen-keys` (production keys should already exist — see MIGRATION.md).

## 2. Full device provision (first time)

For a brand new device that needs bootloader + firmware + factory provisioning:

```bash
just full-provision black
```

This:
1. Builds the bootloader via Nix (ESP-IDF v5.3.1, cached after first build)
2. Signs the bootloader with `secure-boot-key.pem` → `signed-bootloader.bin`
3. Builds the frontier firmware and signs it → `dev-frontier.bin`
4. Flashes bootloader + partition table to the device
5. Flashes firmware + otadata to the device
6. Runs factory provisioning (DS key + genuine certificate)

On first boot, the bootloader burns the signing key digest into eFuses and enables secure boot.

**How Secure Boot v2 key binding works:** The bootloader and firmware are "key-agnostic" at build
time — the Nix build produces identical unsigned binaries for dev and prod. The RSA public key is
embedded in the 4KB signature block appended during signing (see `secure_boot.rs`). On first boot,
the ROM hashes the public key from the signature block and burns that hash into eFuse. On subsequent
boots, it re-hashes the key from the signature block and checks it against the eFuse digest. This
means the eFuse stores only a hash, and the full public key travels with every signed binary.
See: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32c3/security/secure-boot-v2.html

For production: `just env=prod full-provision black`

## 3. Reflash firmware only

For a device that already has secure boot running:

```bash
just flash
```

Builds and signs the frontier firmware, then flashes firmware + otadata. This does **not**
write the bootloader — only `just full-provision` does that. Accidentally writing the wrong
bootloader to a device with burned eFuses would make it unbootable.

## 4. Build the app

```bash
just build linux
```

Compiles the Flutter app with the env's `public_key.hex` embedded and the signed firmware bundled.

## Quick reference

| Task | Dev (default) | Prod |
|------|--------------|------|
| Generate keys | `just gen-keys` | `just env=prod gen-keys` |
| Full provision | `just full-provision black` | `just env=prod full-provision black` |
| Flash firmware only | `just flash` | `just env=prod flash` |
| Build app | `just build linux` | `just env=prod build linux` |
| Run app | `just run` | `just env=prod run` |
| Legacy flash (no SB) | `just legacy-flash` | N/A |
| Legacy run | `just legacy-run` | N/A |

## Factory batch provisioning

For production runs with multiple devices:

```bash
# First time setup (keys should already exist for production):
# just env=prod gen-keys
# just env=prod build-bootloader
# just env=prod sign-bootloader

cargo run -p frostsnap_factory -- batch \
    --color black \
    --quantity 50 \
    --operator "name" \
    --env prod \
    --db-connection-url "mysql://..."
```

## Artifact naming

### Firmware images (in `target/riscv32imc-unknown-none-elf/release/`)

| Board | Unsigned | Signed (dev) | Signed (prod) |
|---|---|---|---|
| `frontier` | `frontier.bin` | `dev-frontier.bin` | `prod-frontier.bin` |
| `legacy` | `legacy.bin` | N/A | N/A |

### Bootloader images (in `frostsnap_factory/bootloader/{env}/`)

| Artifact | Filename |
|---|---|
| Nix build output | `bootloader.bin` |
| Signed | `signed-bootloader.bin` |

## Directory layout

```
frostsnap_factory/
  bootloader/
    flake.nix               # Nix build for unsigned bootloader (dev + prod envs)
    sdkconfig.defaults      # Shared ESP-IDF config (secure boot v2, external signing)
    sdkconfig.defaults.dev  # Dev overrides (JTAG on, insecure mode, logging)
    CMakeLists.txt          # Minimal ESP-IDF project (required by idf.py)
    main/                   # Empty stub component (required by ESP-IDF)
    dev/
      secure-boot-key.pem   # Tracked (dev key, not a secret)
      bootloader.bin         # Build artifact (gitignored)
      signed-bootloader.bin  # Build artifact (gitignored)
    prod/
      secure-boot-key.pem    # Gitignored (production secret)
      bootloader.bin          # Build artifact (gitignored)
      signed-bootloader.bin   # Tracked (migrated from old bootloader-frontier.bin)
  genuine/
    dev/
      secret_key.hex          # Tracked (dev key, not a secret)
      public_key.hex          # Tracked
    prod/
      secret_key.hex          # Gitignored (production secret)
      public_key.hex          # Tracked (from old FACTORY_PUBLIC_KEY)
```
