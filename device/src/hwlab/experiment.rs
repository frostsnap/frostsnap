//! The experiment this build runs. A spec's patch replaces `run`; this default is calibration.

use super::{param, Card};
use crate::resources::Resources;
use core::fmt;

/// Length of the test text, to match the payload size real experiments will use.
const LEN: u32 = param(option_env!("HWLAB_LEN"), 60);
const BEATS: u32 = param(option_env!("HWLAB_BEATS"), 600);

/// Known text the host checks byte for byte.
struct TestText(u32);

impl fmt::Display for TestText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.0 {
            fmt::Write::write_char(f, (b'a' + (i % 26) as u8) as char)?;
        }
        Ok(())
    }
}

pub fn run(_resources: &mut Resources<'_>) -> ! {
    let mut card = Card::take();
    let mut drawn = 0;
    while drawn < BEATS {
        if card.beat(format_args!("cal {LEN} {}", TestText(LEN))) {
            drawn += 1;
        }
    }
    card.done(format_args!("cal {LEN} {}", TestText(LEN)))
}
