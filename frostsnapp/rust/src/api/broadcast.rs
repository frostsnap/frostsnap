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
#[derive(Default, Clone)]
pub struct Broadcast<T> {
    next_id: Arc<AtomicU32>,
    inner: LockedBroadcastInner<T>,
}

type LockedBroadcastInner<T> = Arc<RwLock<BroadcastInner<T>>>;

#[derive(Default)]
struct BroadcastInner<T> {
    subscriptions: BTreeMap<u32, Arc<StreamSink<T>>>,
}

impl<T: SseEncode + Clone> Broadcast<T> {
    #[frb(sync)]
    pub fn subscribe(&self) -> BroadcastSubscription<T> {
        BroadcastSubscription::new(self)
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

/// Tuple of broadcast-subscription-id and broadcast inner.
type BroadcastSubscriptionInner<T> = (u32, LockedBroadcastInner<T>);

/// A single broadcast subscription can subscribe to multiple broadcasts.
#[derive(Clone)]
pub struct BroadcastSubscription<T> {
    is_running: Arc<AtomicBool>,
    inner: Vec<BroadcastSubscriptionInner<T>>,
}

#[derive(Debug, Copy, Clone)]
pub enum StartError {
    /// Occurs when `BroadcastSubscription` is already started.
    AlreadyRunning,
}

impl<T> BroadcastSubscription<T> {
    fn new(broadcast: &Broadcast<T>) -> Self {
        Self::multi(core::iter::once(broadcast))
    }

    fn multi(broadcasts: impl IntoIterator<Item = &Broadcast<T>>) -> Self {
        let is_running = Arc::new(AtomicBool::new(false));
        let mut inner = Vec::<BroadcastSubscriptionInner<T>>::new();
        for broadcast in broadcasts {
            let id = broadcast
                .next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            inner.push((id, broadcast.inner.clone()));
        }
        Self { is_running, inner }
    }

    fn _id(&self) -> u32 {
        self.id
    }

    fn _is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Errors when the subscription is already started.
    fn _start(&self, sink: StreamSink<T>) -> Result<(), StartError> {
        use std::sync::atomic::Ordering;

        let sink = Arc::new(sink);

        if self
            .is_running
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(StartError::AlreadyRunning);
        }
        for (id, broadcast) in &self.inner {
            let mut broadcast_guard = broadcast.write().unwrap();
            broadcast_guard.subscriptions.insert(*id, Arc::clone(&sink));
        }
        Ok(())
    }

    fn _stop(&self) -> bool {
        use std::sync::atomic::Ordering;

        if self
            .is_running
            .compare_exchange(true, false, Ordering::Release, Ordering::Relaxed)
            .is_ok()
        {
            for (id, broadcast) in &self.inner {
                let mut broadcast_guard = broadcast.write().unwrap();
                broadcast_guard.subscriptions.remove(id);
            }
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
