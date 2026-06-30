use tokio::sync::watch;

use super::chain_sync::{ChainStatus, ChainStatusState, ElectrumConfig};
use crate::Sink;

/// The connection's observable lifecycle phase — the single source of truth for status.
/// `ChainStatus` is a pure projection of this plus the config; `state` and `on_backup` are
/// never maintained independently, so they cannot disagree.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnPhase {
    Idle,
    Connecting { on_backup: bool },
    Connected { on_backup: bool },
    Disconnected,
}

impl ConnPhase {
    fn on_backup(self) -> bool {
        matches!(
            self,
            ConnPhase::Connecting { on_backup: true } | ConnPhase::Connected { on_backup: true }
        )
    }

    fn state(self) -> ChainStatusState {
        match self {
            ConnPhase::Idle => ChainStatusState::Idle,
            ConnPhase::Connecting { .. } => ChainStatusState::Connecting,
            ConnPhase::Connected { .. } => ChainStatusState::Connected,
            ConnPhase::Disconnected => ChainStatusState::Disconnected,
        }
    }
}

/// Projects the connection phase + config into `ChainStatus` and emits it to the UI sink.
/// Holds no status replica beyond the current phase — `state`, `on_backup` and the urls are
/// all derived. Emission is deduped: an identical successive status is not re-sent.
pub struct StatusTracker {
    phase: ConnPhase,
    last_emitted: Option<ChainStatus>,
    config_rx: watch::Receiver<ElectrumConfig>,
    sink: Box<dyn Sink<ChainStatus>>,
}

impl StatusTracker {
    pub fn new(config_rx: watch::Receiver<ElectrumConfig>) -> Self {
        Self {
            phase: ConnPhase::Idle,
            last_emitted: None,
            config_rx,
            sink: Box::new(()),
        }
    }

    pub fn set_sink(&mut self, new_sink: Box<dyn Sink<ChainStatus>>) {
        self.sink = new_sink;
        // A fresh subscriber must see the current status even if it equals the last one we
        // emitted to the previous sink, so bypass the dedupe here.
        let status = self.project();
        self.last_emitted = Some(status.clone());
        self.sink.send(status);
    }

    fn project(&self) -> ChainStatus {
        let config = self.config_rx.borrow();
        ChainStatus {
            primary_url: config.primary.clone(),
            backup_url: config.backup.clone(),
            on_backup: self.phase.on_backup(),
            state: self.phase.state(),
        }
    }

    fn emit(&mut self) {
        let status = self.project();
        if self.last_emitted.as_ref() == Some(&status) {
            return;
        }
        self.last_emitted = Some(status.clone());
        self.sink.send(status);
    }

    /// Set the connection phase and emit the projected status (deduped). The sole status
    /// mutator: status changes only by moving the connection through its phases.
    pub(super) fn set_phase(&mut self, phase: ConnPhase) {
        self.phase = phase;
        self.emit();
    }

    /// Re-project and emit after a config-url change (phase unchanged); deduped, so a config
    /// change that doesn't alter the displayed status is silent.
    pub fn refresh(&mut self) {
        self.emit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ElectrumEnabled;
    use std::sync::{Arc, Mutex};

    struct Recorder(Arc<Mutex<Vec<ChainStatus>>>);
    impl Sink<ChainStatus> for Recorder {
        fn send(&self, status: ChainStatus) {
            self.0.lock().unwrap().push(status);
        }
    }

    fn tracker(
        log: Arc<Mutex<Vec<ChainStatus>>>,
    ) -> (StatusTracker, watch::Sender<ElectrumConfig>) {
        let (tx, rx) = watch::channel(ElectrumConfig {
            enabled: ElectrumEnabled::All,
            primary: "tcp://p:1".into(),
            backup: "tcp://b:1".into(),
        });
        let mut tracker = StatusTracker::new(rx);
        tracker.set_sink(Box::new(Recorder(log))); // emits the initial Idle
        (tracker, tx)
    }

    /// `ChainStatus` is a total projection of the phase (+ config urls): every phase maps to
    /// the right `state` + `on_backup`, and the urls always come from the config.
    #[test]
    fn projects_phase_to_status() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let (mut t, _tx) = tracker(log.clone());

        t.set_phase(ConnPhase::Connecting { on_backup: true });
        t.set_phase(ConnPhase::Connected { on_backup: false });
        t.set_phase(ConnPhase::Disconnected);
        t.set_phase(ConnPhase::Idle);

        let log = log.lock().unwrap();
        let observed: Vec<_> = log.iter().map(|s| (s.state, s.on_backup)).collect();
        assert_eq!(
            observed,
            vec![
                (ChainStatusState::Idle, false), // initial, from set_sink
                (ChainStatusState::Connecting, true),
                (ChainStatusState::Connected, false),
                (ChainStatusState::Disconnected, false),
                (ChainStatusState::Idle, false),
            ]
        );
        assert_eq!(log[1].primary_url, "tcp://p:1");
        assert_eq!(log[1].backup_url, "tcp://b:1");
    }

    /// Identical successive statuses are not re-emitted; a config-url change re-emits.
    #[test]
    fn dedupes_identical_status() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let (mut t, tx) = tracker(log.clone());
        let base = log.lock().unwrap().len(); // 1: the initial Idle

        t.set_phase(ConnPhase::Connected { on_backup: false });
        t.set_phase(ConnPhase::Connected { on_backup: false }); // identical → no emit
        assert_eq!(log.lock().unwrap().len(), base + 1);

        t.refresh(); // no url change → no emit
        assert_eq!(log.lock().unwrap().len(), base + 1);

        tx.send_modify(|c| c.primary = "tcp://p:2".into());
        t.refresh(); // urls changed → emit
        assert_eq!(log.lock().unwrap().len(), base + 2);
        assert_eq!(log.lock().unwrap().last().unwrap().primary_url, "tcp://p:2");
    }
}
