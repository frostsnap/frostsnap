use super::chain_sync::{ChainStatus, ChainStatusState};
use crate::Sink;

/// Manages chain status tracking and updates
pub struct StatusTracker {
    current: ChainStatus,
    sink: Box<dyn Sink<ChainStatus>>,
}

impl StatusTracker {
    pub fn new(initial_url: &str) -> Self {
        Self {
            current: ChainStatus::new(initial_url, ChainStatusState::Disconnected),
            sink: Box::new(()),
        }
    }

    pub fn set_sink(&mut self, new_sink: Box<dyn Sink<ChainStatus>>) {
        self.sink = new_sink;
        // Send current status to new sink
        self.sink.send(self.current.clone());
    }

    pub fn update(&mut self, url: &str, state: ChainStatusState) {
        self.current = ChainStatus::new(url, state);
        self.sink.send(self.current.clone());
    }

    pub fn current(&self) -> &ChainStatus {
        &self.current
    }
}
