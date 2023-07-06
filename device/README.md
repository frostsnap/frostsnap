## Requires RISC-V GCC toolchain

Requires `gcc-riscv64-unknown-elf` to compile rust bitcoin to the device. [Debian](https://stackoverflow.com/questions/74231514/how-to-install-riscv32-unknown-elf-gcc-on-debian-based-linuxes), [Arch](https://aur.archlinux.org/riscv-gnu-toolchain-bin.git)

# Usage

## Run the coordinator

```
cd coordinator-cli/
cargo run
```

## Flash and run the device

no-std on RISC-V toolchain installation

```
rustup toolchain install nightly --component rust-src
```

Flash device. Use a good USB cable.

```
cd device/
cargo run --release --target blue --features blue
```
