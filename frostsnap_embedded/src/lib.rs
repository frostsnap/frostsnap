#![no_std]
#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[macro_use]
extern crate alloc;

mod ab_write;
#[cfg(test)]
pub mod test;
pub use ab_write::*;
mod nor_flash_log;
pub use nor_flash_log::*;
mod partition;
pub use partition::*;
mod nonce_slots;
pub use nonce_slots::*;
