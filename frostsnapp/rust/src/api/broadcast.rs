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
    inner: Arc<RwLock<BroadcastInner<T>>>,
}

#[derive(Default)]
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
    fn _id(&self) -> u32 {
        self.id
    }

    fn _is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Errors when the subscription is already started.
    fn _start(&self, sink: StreamSink<T>) -> Result<(), StartError> {
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

    fn _stop(&self) -> bool {
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
