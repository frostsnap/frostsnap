mod lobby;

pub use lobby::*;

use std::time::Duration;

/// A recovery lobby is a longer-lived channel than keygen — participants
/// might trickle in over hours or days. TTL matches "one month" of
/// meaningful liveness before relays are entitled to drop unclaimed
/// share events.
pub const RECOVERY_MESSAGE_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
