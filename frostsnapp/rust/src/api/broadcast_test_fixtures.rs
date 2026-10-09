//! Fixtures for `test/broadcast_lifetime_test.dart`. Nothing in the app uses them.
//!
//! They ship in the bridge because frb generates one set of Dart bindings from whatever is
//! compiled, and the app and `flutter test` share it: a cfg that hid these from the app would hide
//! them from the tests too.

use crate::api::broadcast::{BehaviorBroadcast, Broadcast};
use flutter_rust_bridge::frb;
use frostsnapp_macros::broadcast_handle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

broadcast_handle! { pub struct TestUnitBcast(pub Broadcast<()>); }
broadcast_handle! { pub struct TestI32Bcast(pub Broadcast<i32>); }
broadcast_handle! { pub struct TestI32BehaviorBcast(pub BehaviorBroadcast<i32>); }

pub struct TestBroadcast {
    bcast: Broadcast<()>,
}

impl TestBroadcast {
    #[frb(sync)]
    pub fn create() -> Self {
        Self {
            bcast: Broadcast::default(),
        }
    }

    #[frb(sync)]
    pub fn fire(&self) {
        self.bcast.add(&());
    }

    #[frb(sync)]
    pub fn subscriber_count(&self) -> u32 {
        self.bcast.subscriber_count()
    }

    #[frb(sync)]
    pub fn broadcast(&self) -> TestUnitBcast {
        TestUnitBcast::new(self.bcast.clone())
    }
}

pub struct TestBehaviorBroadcast {
    bcast: BehaviorBroadcast<i32>,
}

impl TestBehaviorBroadcast {
    #[frb(sync)]
    pub fn create(initial: i32) -> Self {
        Self {
            bcast: BehaviorBroadcast::seeded(initial),
        }
    }

    #[frb(sync)]
    pub fn add(&self, value: i32) {
        self.bcast.add(&value);
    }

    #[frb(sync)]
    pub fn subscriber_count(&self) -> u32 {
        self.bcast.subscriber_count()
    }

    #[frb(sync)]
    pub fn broadcast(&self) -> TestI32BehaviorBcast {
        TestI32BehaviorBcast::new(self.bcast.clone())
    }
}

/// Adds 0, 1, 2, … from its own thread until dropped.
pub struct TestTicker {
    bcast: Broadcast<i32>,
    stop: Arc<AtomicBool>,
}

impl TestTicker {
    #[frb(sync)]
    pub fn create(interval_ms: u32) -> Self {
        let bcast = Broadcast::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (bcast_thread, stop_thread) = (bcast.clone(), Arc::clone(&stop));
        let interval = Duration::from_millis(interval_ms.into());
        std::thread::spawn(move || {
            let mut tick: i32 = 0;
            while !stop_thread.load(Ordering::Relaxed) {
                bcast_thread.add(&tick);
                tick = tick.wrapping_add(1);
                std::thread::sleep(interval);
            }
        });
        Self { bcast, stop }
    }

    #[frb(sync)]
    pub fn subscriber_count(&self) -> u32 {
        self.bcast.subscriber_count()
    }

    #[frb(sync)]
    pub fn broadcast(&self) -> TestI32Bcast {
        TestI32Bcast::new(self.bcast.clone())
    }
}

impl Drop for TestTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
