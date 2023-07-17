## Requires RISC-V GCC toolchain

Requires `gcc-riscv64-unknown-elf` to compile rust bitcoin for the device ([Debian](https://stackoverflow.com/questions/74231514/how-to-install-riscv32-unknown-elf-gcc-on-debian-based-linuxes), [Arch](https://aur.archlinux.org/riscv-gnu-toolchain-bin.git)).

# Usage

## Flash and run the device

Install no-std for RISC-V toolchain installation

```
rustup toolchain install nightly --component rust-src
```

Flash the device (use a good USB cable)

```
cd device/
cargo run --release --target blue --features blue
```
