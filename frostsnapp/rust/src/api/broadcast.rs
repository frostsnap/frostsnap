use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicU32},
        Arc, RwLock,
    },
};

use flutter_rust_bridge::frb;
use tracing::Level;

use crate::frb_generated::{SseEncode, StreamSink};

/// A broadcast stream that can be managed from rust.
pub struct Broadcast<T> {
    next_id: Arc<AtomicU32>,
    inner: Arc<RwLock<BroadcastInner<T>>>,
}

impl<T> Default for Broadcast<T> {
    fn default() -> Self {
        Self {
            next_id: Arc::new(AtomicU32::new(0)),
            inner: Arc::new(RwLock::new(BroadcastInner {
                subscriptions: BTreeMap::new(),
            })),
        }
    }
}

impl<T> Clone for Broadcast<T> {
    fn clone(&self) -> Self {
        Self {
            next_id: Arc::clone(&self.next_id),
            inner: Arc::clone(&self.inner),
        }
    }
}

struct BroadcastInner<T> {
    subscriptions: BTreeMap<u32, StreamSink<T>>,
}

impl<T: SseEncode + Clone> Broadcast<T> {
    #[frb(sync)]
    pub fn subscribe(&self) -> BroadcastSubscription<T> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        BroadcastSubscription {
            id,
            is_running: Arc::new(AtomicBool::new(false)),
            inner: Arc::clone(&self.inner),
        }
    }

    #[frb(sync)]
    pub fn add(&self, data: &T) {
        let inner = self.inner.read().unwrap();
        for (id, sink) in &inner.subscriptions {
            if sink.add(data.clone()).is_err() {
                tracing::event!(Level::ERROR, id, "Failed to add to sink");
            }
        }
    }
}

#[derive(Clone)]
pub struct BroadcastSubscription<T> {
    id: u32,
    is_running: Arc<AtomicBool>,
    inner: Arc<RwLock<BroadcastInner<T>>>,
}

#[derive(Debug, Copy, Clone)]
pub enum StartError {
    /// Occurs when `BroadcastSubscription` is already started.
    AlreadyRunning,
}

impl<T> BroadcastSubscription<T> {
    pub(crate) fn _id(&self) -> u32 {
        self.id
    }

    pub(crate) fn _is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Errors when the subscription is already started.
    pub(crate) fn _start(&self, sink: StreamSink<T>) -> Result<(), StartError> {
        use std::sync::atomic::Ordering;

        if self
            .is_running
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(StartError::AlreadyRunning);
        }
        let mut inner = self.inner.write().unwrap();
        inner.subscriptions.insert(self.id, sink);
        Ok(())
    }

    pub(crate) fn _stop(&self) -> bool {
        use std::sync::atomic::Ordering;

        if self
            .is_running
            .compare_exchange(true, false, Ordering::Release, Ordering::Relaxed)
            .is_ok()
        {
            let mut inner = self.inner.write().unwrap();
            inner.subscriptions.remove(&self.id);
            true
        } else {
            false
        }
    }
}

impl<T> Drop for BroadcastSubscription<T> {
    fn drop(&mut self) {
        self._stop();
    }
}

/// Thin wrapper around [`Broadcast`] that also caches the most recent
/// value. Newly-arriving subscribers see the current state immediately
/// (via `_start`) instead of waiting for the next `add`. Modelled after
/// RxJS `BehaviorSubject`.
pub struct BehaviorBroadcast<T> {
    inner: Broadcast<T>,
    latest: Arc<RwLock<Option<T>>>,
}

impl<T> Default for BehaviorBroadcast<T> {
    fn default() -> Self {
        Self {
            inner: Broadcast::default(),
            latest: Arc::new(RwLock::new(None)),
        }
    }
}

impl<T> BehaviorBroadcast<T> {
    /// Create a `BehaviorBroadcast` with a pre-populated cached value.
    /// Fresh subscribers see `initial` on subscribe without the caller
    /// having to follow `default()` with `add()` — and without the
    /// `add()` fan-out to a (necessarily empty) subscriber set.
    pub fn seeded(initial: T) -> Self {
        Self {
            inner: Broadcast::default(),
            latest: Arc::new(RwLock::new(Some(initial))),
        }
    }
}

impl<T> Clone for BehaviorBroadcast<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            latest: Arc::clone(&self.latest),
        }
    }
}

impl<T: SseEncode + Clone> BehaviorBroadcast<T> {
    #[frb(sync)]
    pub fn subscribe(&self) -> BehaviorBroadcastSubscription<T> {
        BehaviorBroadcastSubscription {
            inner: self.inner.subscribe(),
            latest: Arc::clone(&self.latest),
        }
    }

    #[frb(sync)]
    pub fn add(&self, data: &T) {
        // Hold `latest.write()` for the full add — cache update AND
        // fan-out. Pairs with `_start` taking `latest.read()` for its
        // full critical section (cached emit + sink register). The two
        // operations are mutually exclusive on this lock, so a fresh
        // subscriber sees either: (a) the cached value at register
        // time plus all subsequent fan-outs in order, or (b) all
        // fan-outs in order. Never misses, never reorders.
        let mut latest = self.latest.write().unwrap();
        *latest = Some(data.clone());
        self.inner.add(data);
    }

    /// Current cached value, if any.
    #[frb(sync)]
    pub fn latest(&self) -> Option<T> {
        self.latest.read().unwrap().clone()
    }
}

#[derive(Clone)]
pub struct BehaviorBroadcastSubscription<T> {
    inner: BroadcastSubscription<T>,
    latest: Arc<RwLock<Option<T>>>,
}

impl<T: SseEncode + Clone> BehaviorBroadcastSubscription<T> {
    pub(crate) fn _id(&self) -> u32 {
        self.inner._id()
    }

    pub(crate) fn _is_running(&self) -> bool {
        self.inner._is_running()
    }

    /// Starts the subscription and, if a cached value exists, immediately
    /// emits it to the freshly-installed sink. Errors when the subscription
    /// is already started.
    pub(crate) fn _start(&self, sink: StreamSink<T>) -> Result<(), StartError> {
        // Refuse before emitting cached so an `AlreadyRunning` caller
        // doesn't end up with a sink that received one value and was then
        // discarded.
        if self.inner._is_running() {
            return Err(StartError::AlreadyRunning);
        }
        // Hold `latest.read()` across cached emit + sink register. Pairs
        // with `BehaviorBroadcast::add` taking `latest.write()` across
        // its cache-update + fan-out. Without this, the subscriber could
        // read cached A, then a concurrent `add(B)` could write cache and
        // fan out (without us — sink isn't in the map yet), and we'd
        // register after the fact: cached A delivered, B silently lost.
        let latest = self.latest.read().unwrap();
        if let Some(value) = latest.clone() {
            if sink.add(value).is_err() {
                tracing::event!(
                    Level::ERROR,
                    id = self.inner._id(),
                    "Failed to emit cached value to fresh sink"
                );
            }
        }
        self.inner._start(sink)
    }

    pub(crate) fn _stop(&self) -> bool {
        self.inner._stop()
    }
}

impl<T: SseEncode + Clone + Send + Sync + 'static> frostsnap_coordinator::Sink<T> for Broadcast<T> {
    fn send(&self, data: T) {
        self.add(&data);
    }
}

impl<T: SseEncode + Clone + Send + Sync + 'static> frostsnap_coordinator::Sink<T>
    for BehaviorBroadcast<T>
{
    fn send(&self, data: T) {
        self.add(&data);
    }
}

pub struct UnitBroadcastSubscription(pub(crate) BroadcastSubscription<()>);

impl UnitBroadcastSubscription {
    #[frb(sync)]
    pub fn id(&self) -> u32 {
        self.0._id()
    }

    #[frb(sync)]
    pub fn is_running(&self) -> bool {
        self.0._is_running()
    }

    #[frb(sync)]
    pub fn start(&self, sink: StreamSink<()>) -> Result<(), StartError> {
        self.0._start(sink)
    }

    #[frb(sync)]
    pub fn stop(&self) -> bool {
        self.0._stop()
    }
}
