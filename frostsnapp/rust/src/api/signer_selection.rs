use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use flutter_rust_bridge::frb;
use frostsnap_core::DeviceId;

use super::broadcast::{Broadcast, UnitBroadcastSubscription};
use crate::coordinator::{NonceReservedListeners, NonceSource};

/// The devices chosen to sign. A device with no nonces left can't sign, so it is never selected:
/// selecting it does nothing, and it leaves the selection as soon as a signing session reserves
/// its last nonces.
#[derive(Clone)]
pub struct SignerSelection {
    selected: Arc<RwLock<HashSet<DeviceId>>>,
    nonces: Arc<dyn NonceSource + Send + Sync>,
    changed: Broadcast<()>,
}

impl SignerSelection {
    pub(crate) fn new(
        nonces: Arc<dyn NonceSource + Send + Sync>,
        nonces_reserved: &NonceReservedListeners,
        changed: Broadcast<()>,
    ) -> Self {
        let selected = Arc::new(RwLock::new(HashSet::new()));
        let weak_selected = Arc::downgrade(&selected);
        let listener_nonces = nonces.clone();
        let listener_changed = changed.clone();
        nonces_reserved.register(move || {
            let Some(selected) = weak_selected.upgrade() else {
                return false;
            };
            selected
                .write()
                .unwrap()
                .retain(|&d_id| listener_nonces.nonces_available(d_id) > 0);
            // Even with no selected device dropped, an unselected one may have just run out and
            // its checkbox must be disabled.
            listener_changed.add(&());
            true
        });
        Self {
            selected,
            nonces,
            changed,
        }
    }

    #[frb(sync)]
    pub fn subscribe(&self) -> UnitBroadcastSubscription {
        UnitBroadcastSubscription(self.changed.subscribe())
    }

    #[frb(sync)]
    pub fn selected(&self) -> Vec<DeviceId> {
        self.selected.read().unwrap().iter().copied().collect()
    }

    #[frb(sync)]
    pub fn select(&self, d_id: DeviceId) {
        let inserted = {
            // Checked under the lock the reservation listener takes, so a reservation landing
            // after the check is only announced once the device is in and can be removed.
            let mut selected = self.selected.write().unwrap();
            self.nonces.nonces_available(d_id) > 0 && selected.insert(d_id)
        };
        if inserted {
            self.changed.add(&());
        }
    }

    #[frb(sync)]
    pub fn deselect(&self, d_id: DeviceId) {
        let removed = self.selected.write().unwrap().remove(&d_id);
        if removed {
            self.changed.add(&());
        }
    }

    /// Doesn't announce the change, for callers that announce their own.
    pub(crate) fn clear_quietly(&self) {
        self.selected.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{mpsc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use super::*;

    #[derive(Default)]
    struct Nonces(Mutex<HashMap<DeviceId, u32>>);

    impl Nonces {
        fn set(&self, id: DeviceId, n: u32) {
            self.0.lock().unwrap().insert(id, n);
        }
    }

    impl NonceSource for Nonces {
        fn nonces_available(&self, id: DeviceId) -> u32 {
            self.0.lock().unwrap().get(&id).copied().unwrap_or(0)
        }
    }

    const A: DeviceId = DeviceId([1; 33]);
    const B: DeviceId = DeviceId([2; 33]);

    fn selection(nonces: &Arc<Nonces>, listeners: &NonceReservedListeners) -> SignerSelection {
        SignerSelection::new(nonces.clone(), listeners, Broadcast::default())
    }

    #[test]
    fn a_selected_device_leaves_when_its_nonces_are_reserved() {
        let nonces = Arc::new(Nonces::default());
        let listeners = NonceReservedListeners::default();
        let selection = selection(&nonces, &listeners);
        nonces.set(A, 3);
        nonces.set(B, 3);
        selection.select(A);
        selection.select(B);

        nonces.set(A, 0);
        listeners.notify();

        assert_eq!(*selection.selected.read().unwrap(), HashSet::from([B]));
    }

    #[test]
    fn a_device_exhausted_by_another_signing_session_cannot_be_selected() {
        let nonces = Arc::new(Nonces::default());
        let listeners = NonceReservedListeners::default();
        let open_screen = selection(&nonces, &listeners);
        nonces.set(A, 1);

        // Another screen's signing session spends A's last nonce while this one shows A as
        // selectable; a tap on the stale checkbox arrives afterwards.
        nonces.set(A, 0);
        listeners.notify();
        open_screen.select(A);

        assert!(open_screen.selected().is_empty());
    }

    /// Reports one nonce, but reserves it on another thread as soon as it's asked.
    struct ReservedDuringCheck {
        listeners: NonceReservedListeners,
        nonces: AtomicU32,
        reservation: Mutex<Option<JoinHandle<()>>>,
    }

    impl NonceSource for ReservedDuringCheck {
        fn nonces_available(&self, _id: DeviceId) -> u32 {
            let nonces = self.nonces.swap(0, Ordering::SeqCst);
            if nonces > 0 {
                let listeners = self.listeners.clone();
                let (done, reserved) = mpsc::channel();
                *self.reservation.lock().unwrap() = Some(thread::spawn(move || {
                    listeners.notify();
                    let _ = done.send(());
                }));
                // Give the reservation every chance to finish before the caller inserts.
                let _ = reserved.recv_timeout(Duration::from_millis(200));
            }
            nonces
        }
    }

    #[test]
    fn a_reservation_racing_a_select_still_leaves_the_device_out() {
        let listeners = NonceReservedListeners::default();
        let nonces = Arc::new(ReservedDuringCheck {
            listeners: listeners.clone(),
            nonces: AtomicU32::new(1),
            reservation: Mutex::new(None),
        });
        let selection = SignerSelection::new(nonces.clone(), &listeners, Broadcast::default());

        selection.select(A);
        let reservation = nonces.reservation.lock().unwrap().take().unwrap();
        reservation.join().unwrap();

        assert!(selection.selected().is_empty());
    }

    #[test]
    fn a_dropped_selection_stops_listening() {
        let nonces = Arc::new(Nonces::default());
        let listeners = NonceReservedListeners::default();
        drop(selection(&nonces, &listeners));

        listeners.notify();

        assert!(listeners.is_empty());
    }
}
