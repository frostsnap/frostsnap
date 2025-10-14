use std::collections::VecDeque;

use bdk_chain::bitcoin::Txid;

/// Tracks outgoing transactions.
#[derive(Clone, Default)]
pub struct OutgoingTracker {
    /// Queue of the outgoing transactions.
    queue: VecDeque<Txid>,
}

pub enum Mutation {
    Push(Txid),
    Forget(Txid),
}

impl OutgoingTracker {
    pub fn queue(&self) -> &VecDeque<Txid> {
        &self.queue
    }

    pub fn forget(&mut self, txid: Txid) -> bool {
        let to_forget = self
            .queue
            .iter()
            .enumerate()
            .find(|(_, &i_txid)| i_txid == txid)
            .map(|(i, _)| i);
        if let Some(i) = to_forget {
            self.queue.remove(i);
            true
        } else {
            false
        }
    }
}
