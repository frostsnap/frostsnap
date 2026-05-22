use crate::api::broadcast::{BehaviorBroadcast, Broadcast};
use flutter_rust_bridge::frb;
use frostsnap_macros::broadcast_handle;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

broadcast_handle! { pub struct TestUnitBcast(pub Broadcast<()>); }
broadcast_handle! { pub struct TestI32Bcast(pub Broadcast<i32>); }
broadcast_handle! { pub struct TestI32BehaviorBcast(pub BehaviorBroadcast<i32>); }

pub struct TestBroadcastHandle {
    bcast: Broadcast<()>,
}

impl TestBroadcastHandle {
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

pub struct TestBehaviorBroadcastHandle {
    bcast: BehaviorBroadcast<i32>,
}

impl TestBehaviorBroadcastHandle {
    #[frb(sync)]
    pub fn create() -> Self {
        Self {
            bcast: BehaviorBroadcast::seeded(0),
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

pub struct TestTickerHandle {
    bcast: Broadcast<i32>,
    stop: Arc<AtomicBool>,
}

impl TestTickerHandle {
    #[frb(sync)]
    pub fn create(interval_ms: u32) -> Self {
        let bcast = Broadcast::default();
        let stop = Arc::new(AtomicBool::new(false));
        let bcast_thread = bcast.clone();
        let stop_thread = Arc::clone(&stop);
        let interval = Duration::from_millis(interval_ms as u64);
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

impl Drop for TestTickerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub struct TestKeyedTickerHandle {
    bcasts: Arc<RwLock<HashMap<String, Broadcast<i32>>>>,
    stop: Arc<AtomicBool>,
}

impl TestKeyedTickerHandle {
    #[frb(sync)]
    pub fn create() -> Self {
        Self {
            bcasts: Arc::new(RwLock::new(HashMap::new())),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    #[frb(sync)]
    pub fn add_ticker(&self, key: String, interval_ms: u32) {
        let bcast = Broadcast::default();
        {
            let mut map = self.bcasts.write().unwrap();
            assert!(!map.contains_key(&key), "ticker for {key} already exists");
            map.insert(key.clone(), bcast.clone());
        }
        let stop = Arc::clone(&self.stop);
        let interval = Duration::from_millis(interval_ms as u64);
        std::thread::spawn(move || {
            let mut tick: i32 = 0;
            while !stop.load(Ordering::Relaxed) {
                bcast.add(&tick);
                tick = tick.wrapping_add(1);
                std::thread::sleep(interval);
            }
        });
    }

    #[frb(sync)]
    pub fn broadcast(&self, key: String) -> TestI32Bcast {
        let map = self.bcasts.read().unwrap();
        let bcast = map
            .get(&key)
            .unwrap_or_else(|| panic!("no ticker for key: {key}"));
        TestI32Bcast::new(bcast.clone())
    }

    #[frb(sync)]
    pub fn subscriber_count(&self, key: String) -> u32 {
        self.bcasts
            .read()
            .unwrap()
            .get(&key)
            .map(|b| b.subscriber_count())
            .unwrap_or(0)
    }
}

impl Drop for TestKeyedTickerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
