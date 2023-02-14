use log::LevelFilter;

pub const OUR_INDEX: usize = 0;
pub const LOG_LEVEL: LevelFilter = LevelFilter::Debug;

// pub const OUR_INDEX: usize = 2;
// pub const LOG_LEVEL: LevelFilter = LevelFilter::Off;
pub mod frost_core;
pub mod io;
pub mod message;
