use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicU32, Arc, RwLock},
};

use tracing::Level;

use crate::frb_generated::{SseEncode, StreamSink};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct SinkRegistrationId(pub u32);

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

impl<T> Broadcast<T> {
    pub fn subscriber_count(&self) -> u32 {
        self.inner.read().unwrap().subscriptions.len() as u32
    }
}

impl<T: SseEncode + Clone> Broadcast<T> {
    pub fn register(&self, sink: StreamSink<T>) -> SinkRegistrationId {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.write().unwrap().subscriptions.insert(id, sink);
        SinkRegistrationId(id)
    }

    pub fn unregister(&self, id: SinkRegistrationId) -> bool {
        self.inner
            .write()
            .unwrap()
            .subscriptions
            .remove(&id.0)
            .is_some()
    }

    pub fn add(&self, data: &T) {
        let inner = self.inner.read().unwrap();
        for (id, sink) in &inner.subscriptions {
            if sink.add(data.clone()).is_err() {
                tracing::event!(Level::ERROR, id, "Failed to add to sink");
            }
        }
    }
}

/// Thin wrapper around [`Broadcast`] that also caches the most recent
/// value. Newly-arriving subscribers see the current state immediately
/// (via the cached emit inside `register`) instead of waiting for the
/// next `add`. Modelled after RxJS `BehaviorSubject`.
pub struct BehaviorBroadcast<T> {
    inner: Broadcast<T>,
    latest: Arc<RwLock<Option<T>>>,
}

impl<T> BehaviorBroadcast<T> {
    /// Construct with a pre-populated cached value. Every
    /// `BehaviorBroadcast` is born with a current state — there is no
    /// `default()` constructor that produces an empty one. Fresh
    /// subscribers see `initial` on attach.
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
    pub fn register(&self, sink: StreamSink<T>) -> SinkRegistrationId {
        // Hold `latest.read()` across the entire critical section:
        // cached emit AND `inner.register` (which inserts the sink into
        // the broadcast map). `BehaviorBroadcast::add` holds
        // `latest.write()` across (cache update + fan-out), so the two
        // serialize and there is no window where a sink is registered
        // but missed by a concurrent `add`.
        let latest = self.latest.read().unwrap();
        if let Some(value) = latest.clone() {
            if sink.add(value).is_err() {
                tracing::event!(Level::ERROR, "Failed to emit cached value to fresh sink");
            }
        }
        let reg = self.inner.register(sink);
        drop(latest); // explicit: insert must be inside the read-lock scope
        reg
    }

    pub fn unregister(&self, id: SinkRegistrationId) -> bool {
        self.inner.unregister(id)
    }

    pub fn subscriber_count(&self) -> u32 {
        self.inner.subscriber_count()
    }

    pub fn add(&self, data: &T) {
        // Hold `latest.write()` for the full add — cache update AND
        // fan-out. Pairs with `register` taking `latest.read()` for its
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
    pub fn latest(&self) -> Option<T> {
        self.latest.read().unwrap().clone()
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
