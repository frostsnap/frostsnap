// because flutter rust bridge is currently making code that triggers this
#![allow(clippy::unnecessary_literal_unwrap)]
mod api;
mod bridge_generated;
mod coordinator;
pub use coordinator::*;
mod device_list;
