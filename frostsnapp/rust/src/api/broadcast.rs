use std::{
    collections::BTreeMap,
    marker::PhantomData,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, RwLock,
    },
};

use tracing::Level;

use crate::{
    frb_generated::{SseEncode, StreamSink},
    sink_wrap::BroadcastSink,
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct SinkRegistrationId(pub u32);

/// Fans each value out to every registered Dart sink. Expose one to Dart with
/// `frostsnapp_macros::broadcast_handle!`.
pub struct Broadcast<T, S = StreamSink<T>> {
    next_id: Arc<AtomicU32>,
    subscriptions: Arc<RwLock<BTreeMap<u32, S>>>,
    _value: PhantomData<fn(T)>,
}

impl<T, S> Default for Broadcast<T, S> {
    fn default() -> Self {
        Self {
            next_id: Arc::new(AtomicU32::new(0)),
            subscriptions: Arc::new(RwLock::new(BTreeMap::new())),
            _value: PhantomData,
        }
    }
}

impl<T, S> Clone for Broadcast<T, S> {
    fn clone(&self) -> Self {
        Self {
            next_id: Arc::clone(&self.next_id),
            subscriptions: Arc::clone(&self.subscriptions),
            _value: PhantomData,
        }
    }
}

impl<T, S> Broadcast<T, S> {
    pub fn subscriber_count(&self) -> u32 {
        self.subscriptions.read().unwrap().len() as u32
    }

    pub fn unregister(&self, id: SinkRegistrationId) -> bool {
        self.subscriptions.write().unwrap().remove(&id.0).is_some()
    }
}

impl<T: Clone, S: BroadcastSink<T>> Broadcast<T, S> {
    pub fn register(&self, sink: S) -> SinkRegistrationId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.subscriptions.write().unwrap().insert(id, sink);
        SinkRegistrationId(id)
    }

    pub fn add(&self, data: &T) {
        for (id, sink) in self.subscriptions.read().unwrap().iter() {
            if !sink.deliver(data.clone()) {
                tracing::event!(Level::ERROR, id, "Failed to add to sink");
            }
        }
    }
}

/// A [`Broadcast`] that holds a current value and hands it to each new subscriber first, like
/// RxJS's `BehaviorSubject`.
pub struct BehaviorBroadcast<T, S = StreamSink<T>> {
    inner: Broadcast<T, S>,
    latest: Arc<RwLock<T>>,
}

impl<T, S> BehaviorBroadcast<T, S> {
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

impl<T, S> Clone for BehaviorBroadcast<T, S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            latest: Arc::clone(&self.latest),
        }
    }
}

impl<T: Clone, S: BroadcastSink<T>> BehaviorBroadcast<T, S> {
    pub fn register(&self, sink: S) -> SinkRegistrationId {
        // `add` holds the write lock across updating `latest` and fanning out, so holding the
        // read lock across the cached emit and the insert means a new sink sees every value
        // exactly once and in order: either before it's cached or through the fan-out.
        let latest = self.latest.read().unwrap();
        if !sink.deliver(latest.clone()) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<i32>>>);

    impl Recorder {
        fn received(&self) -> Vec<i32> {
            self.0.lock().unwrap().clone()
        }
    }

    impl BroadcastSink<i32> for Recorder {
        fn deliver(&self, value: i32) -> bool {
            self.0.lock().unwrap().push(value);
            true
        }
    }

    #[test]
    fn unregistering_one_sink_leaves_the_other() {
        let bcast = Broadcast::<i32, Recorder>::default();
        let (a, b) = (Recorder::default(), Recorder::default());
        let id_a = bcast.register(a.clone());
        let id_b = bcast.clone().register(b.clone());
        assert_eq!(bcast.subscriber_count(), 2);

        bcast.add(&1);
        assert_eq!(a.received(), [1]);
        assert_eq!(b.received(), [1]);

        assert!(bcast.unregister(id_b));
        assert_eq!(bcast.subscriber_count(), 1);
        bcast.add(&2);
        assert_eq!(a.received(), [1, 2], "remaining sink still receives");
        assert_eq!(b.received(), [1], "unregistered sink gets nothing");

        assert!(bcast.unregister(id_a));
        assert_eq!(bcast.subscriber_count(), 0);
    }

    #[test]
    fn behavior_broadcast_gives_a_new_sink_the_current_value_then_later_adds() {
        let bcast = BehaviorBroadcast::<i32, Recorder>::seeded(0);
        bcast.add(&42);
        let sink = Recorder::default();
        bcast.register(sink.clone());
        assert_eq!(sink.received(), [42]);

        bcast.add(&43);
        assert_eq!(sink.received(), [42, 43]);
    }
}
