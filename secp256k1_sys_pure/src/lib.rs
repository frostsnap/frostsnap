// SPDX-License-Identifier: CC0-1.0

//! Stand-in for `secp256k1-sys`, swapped in by the workspace `[patch.crates-io]`.
//!
//! The firmware has no C compiler and only ever asks secp256k1 to parse,
//! serialize and tweak keys, so on riscv32 this crate answers to the
//! `secp256k1-sys` API with pure Rust that does just that (see `pure.rs` for
//! what is missing and why). Everywhere else it re-exports the real
//! `secp256k1-sys`, so host builds keep the C libsecp256k1 unchanged.
//!
//! `RUSTFLAGS="--cfg secp256k1_pure"` selects the pure backend on the host,
//! which is how `tests/backend_parity.rs` gets run against both.

#![no_std]

#[cfg(not(any(target_arch = "riscv32", secp256k1_pure)))]
pub use c_sys::*;

#[cfg(any(target_arch = "riscv32", secp256k1_pure))]
mod arith;
#[cfg(any(target_arch = "riscv32", secp256k1_pure))]
mod pure;
#[cfg(any(target_arch = "riscv32", secp256k1_pure))]
pub use pure::*;
