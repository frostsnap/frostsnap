use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, RwLock,
    },
};

use tracing::Level;

use crate::frb_generated::{SseEncode, StreamSink};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct SinkRegistrationId(pub u32);

/// Fans each value out to every registered Dart sink. Expose one to Dart with
/// `frostsnapp_macros::broadcast_handle!`.
pub struct Broadcast<T> {
    next_id: Arc<AtomicU32>,
    subscriptions: Arc<RwLock<BTreeMap<u32, StreamSink<T>>>>,
}

impl<T> Default for Broadcast<T> {
    fn default() -> Self {
        Self {
            next_id: Arc::new(AtomicU32::new(0)),
            subscriptions: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

impl<T> Clone for Broadcast<T> {
    fn clone(&self) -> Self {
        Self {
            next_id: Arc::clone(&self.next_id),
            subscriptions: Arc::clone(&self.subscriptions),
        }
    }
}

impl<T> Broadcast<T> {
    pub fn subscriber_count(&self) -> u32 {
        self.subscriptions.read().unwrap().len() as u32
    }

    pub fn unregister(&self, id: SinkRegistrationId) -> bool {
        self.subscriptions.write().unwrap().remove(&id.0).is_some()
    }
}

impl<T: SseEncode + Clone> Broadcast<T> {
    pub fn register(&self, sink: StreamSink<T>) -> SinkRegistrationId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.subscriptions.write().unwrap().insert(id, sink);
        SinkRegistrationId(id)
    }

    pub fn add(&self, data: &T) {
        for (id, sink) in self.subscriptions.read().unwrap().iter() {
            if sink.add(data.clone()).is_err() {
                tracing::event!(Level::ERROR, id, "Failed to add to sink");
            }
        }
    }
}

/// A [`Broadcast`] that holds a current value and hands it to each new subscriber first, like
/// RxJS's `BehaviorSubject`.
pub struct BehaviorBroadcast<T> {
    inner: Broadcast<T>,
    latest: Arc<RwLock<T>>,
}

impl<T> BehaviorBroadcast<T> {
    pub fn seeded(initial: T) -> Self {
        Self {
            inner: Broadcast::default(),
            latest: Arc::new(RwLock::new(initial)),
        }
    }

    pub fn subscriber_count(&self) -> u32 {
        self.inner.subscriber_count()
    }

    pub fn unregister(&self, id: SinkRegistrationId) -> bool {
        self.inner.unregister(id)
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
        // `add` holds the write lock across updating `latest` and fanning out, so holding the
        // read lock across the cached emit and the insert means a new sink sees every value
        // exactly once and in order: either before it's cached or through the fan-out.
        let latest = self.latest.read().unwrap();
        if sink.add(latest.clone()).is_err() {
            tracing::event!(Level::ERROR, "Failed to emit cached value to new sink");
        }
        self.inner.register(sink)
    }

    pub fn add(&self, data: &T) {
        let mut latest = self.latest.write().unwrap();
        *latest = data.clone();
        self.inner.add(data);
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
