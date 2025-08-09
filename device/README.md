# Frostsnap Firmware

ESP32-C3 firmware built with Rust.

Requires RISC-V GCC toolchain for `riscv32imc-unknown-none-elf` target.

### Install Rust toolchain

```bash
rustup toolchain install stable --component rust-src
rustup target add riscv32imc-unknown-none-elf
```

### Install RISC-V GCC

- **Debian/Ubuntu**: `apt install gcc-riscv32-unknown-elf`
- **Arch**: Install `riscv-gnu-toolchain-bin` from AUR
- **Other**: Use `just fetch-riscv` to download pinned version

### Build and flash

```bash
just build-device v2
just flash
```

## Deterministic Builds

```bash
./deterministic-build.sh
```
