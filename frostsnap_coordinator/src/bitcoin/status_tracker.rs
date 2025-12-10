use super::chain_sync::{ChainStatus, ChainStatusState};
use crate::settings::ElectrumEnabled;
use crate::Sink;

/// Manages chain status tracking and updates - single source of truth
pub struct StatusTracker {
    current: ChainStatus,
    enabled: ElectrumEnabled,
    sink: Box<dyn Sink<ChainStatus>>,
}

impl StatusTracker {
    pub fn new(initial_status: ChainStatus) -> Self {
        Self {
            current: initial_status,
            enabled: ElectrumEnabled::default(),
            sink: Box::new(()),
        }
    }

    pub fn set_sink(&mut self, new_sink: Box<dyn Sink<ChainStatus>>) {
        self.sink = new_sink;
        self.sink.send(self.current.clone());
    }

    fn emit(&mut self) {
        self.sink.send(self.current.clone());
    }

    pub fn current(&self) -> &ChainStatus {
        &self.current
    }

    pub fn set_state(&mut self, state: ChainStatusState) {
        self.current.state = state;
        self.emit();
    }

    pub fn set_state_and_server(&mut self, state: ChainStatusState, on_backup: bool) {
        self.current.state = state;
        self.current.on_backup = on_backup;
        self.emit();
    }

    pub fn set_urls(&mut self, primary: String, backup: String) {
        self.current.primary_url = primary;
        self.current.backup_url = backup;
        self.emit();
    }

    pub fn set_enabled(&mut self, enabled: ElectrumEnabled) {
        self.enabled = enabled;
    }

    pub fn primary_url(&self) -> &str {
        &self.current.primary_url
    }

    pub fn backup_url(&self) -> &str {
        &self.current.backup_url
    }

    pub fn enabled(&self) -> ElectrumEnabled {
        self.enabled
    }

    pub fn on_backup(&self) -> bool {
        self.current.on_backup
    }
}
