#![no_std]

extern crate alloc;


pub mod palette;
pub mod widgets;

// Re-export commonly used items
pub use widgets::*;

// Time representation for animations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    ticks: u64,
}

impl Instant {
    pub fn from_ticks(ticks: u64) -> Self {
        Self { ticks }
    }

    pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
        if self.ticks >= earlier.ticks {
            Some(Duration::from_millis((self.ticks - earlier.ticks) as u32))
        } else {
            None
        }
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Duration {
    millis: u32,
}

impl Duration {
    pub fn from_millis(millis: u32) -> Self {
        Self { millis }
    }

    pub fn to_millis(&self) -> u32 {
        self.millis
    }
}