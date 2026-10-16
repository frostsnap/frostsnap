//! Work addressed to this device, parked while the crypto engine is attesting.
//!
//! Coordinator messages and UI events share one FIFO, so handlers run in the order the device
//! admitted them. Each source has its own share of the bound, so neither can crowd out the other.
//! Nothing is dropped at the bound: a full coordinator share stops upstream reads, and a UI event
//! that may not be admitted stays in its widget.

use alloc::collections::VecDeque;

pub const COORDINATOR_SHARE: usize = 16;
pub const UI_SHARE: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalWork<C, U> {
    Coordinator(C),
    Ui(U),
}

#[derive(Debug)]
pub struct LocalQueue<C, U> {
    items: VecDeque<LocalWork<C, U>>,
    upstream_backpressured: bool,
}

impl<C, U> Default for LocalQueue<C, U> {
    fn default() -> Self {
        Self {
            items: VecDeque::with_capacity(COORDINATOR_SHARE + UI_SHARE),
            upstream_backpressured: false,
        }
    }
}

impl<C, U> LocalQueue<C, U> {
    fn count_ui(&self) -> usize {
        self.items
            .iter()
            .filter(|work| matches!(work, LocalWork::Ui(_)))
            .count()
    }

    /// Whether the upstream reader may take another frame. When it may not, upstream counts as
    /// backpressured until [`Self::upstream_drained`].
    pub fn may_read_upstream(&mut self) -> bool {
        let room = self.items.len() - self.count_ui() < COORDINATOR_SHARE;
        if !room {
            self.upstream_backpressured = true;
        }
        room
    }

    /// The upstream reader found nothing more to read.
    pub fn upstream_drained(&mut self) {
        self.upstream_backpressured = false;
    }

    /// # Panics
    ///
    /// If the coordinator share is full; check [`Self::may_read_upstream`] before reading.
    pub fn park_coordinator(&mut self, message: C) {
        assert!(
            self.items.len() - self.count_ui() < COORDINATOR_SHARE,
            "coordinator share is full"
        );
        self.items.push_back(LocalWork::Coordinator(message));
    }

    /// Whether a UI event may be taken from the UI now. It may not while a coordinator frame is
    /// held back in the transport, so that an event never overtakes a frame the device received
    /// first.
    pub fn may_take_ui_event(&self) -> bool {
        !self.upstream_backpressured && self.count_ui() < UI_SHARE
    }

    /// # Panics
    ///
    /// If [`Self::may_take_ui_event`] is false.
    pub fn park_ui(&mut self, event: U) {
        assert!(self.may_take_ui_event(), "UI event parked out of turn");
        self.items.push_back(LocalWork::Ui(event));
    }

    pub fn pop(&mut self) -> Option<LocalWork<C, U>> {
        self.items.pop_front()
    }

    /// For a cancel that has just been handled: the UI events behind it came from the prompt it
    /// closed.
    pub fn discard_ui_events(&mut self) {
        self.items.retain(|work| !matches!(work, LocalWork::Ui(_)));
    }

    /// For a session reset: the parked work belongs to a session the coordinator has abandoned.
    pub fn clear(&mut self) {
        self.items.clear();
        self.upstream_backpressured = false;
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use alloc::vec::Vec;

    type Queue = LocalQueue<u32, &'static str>;

    fn drain(queue: &mut Queue) -> Vec<LocalWork<u32, &'static str>> {
        core::iter::from_fn(|| queue.pop()).collect()
    }

    /// The device loop's upstream read discipline.
    fn read_upstream(queue: &mut Queue, frames: &mut VecDeque<u32>) {
        loop {
            if !queue.may_read_upstream() {
                return;
            }
            match frames.pop_front() {
                Some(frame) => queue.park_coordinator(frame),
                None => return queue.upstream_drained(),
            }
        }
    }

    #[test]
    fn work_runs_in_admission_order() {
        let mut queue = Queue::default();
        queue.park_coordinator(1);
        queue.park_ui("confirm");
        queue.park_coordinator(2);
        assert_eq!(
            drain(&mut queue),
            [
                LocalWork::Coordinator(1),
                LocalWork::Ui("confirm"),
                LocalWork::Coordinator(2)
            ]
        );
    }

    #[test]
    fn a_full_coordinator_share_pauses_reading_instead_of_dropping() {
        let mut queue = Queue::default();
        let mut transport: VecDeque<u32> = (0..20).collect();
        read_upstream(&mut queue, &mut transport);
        assert_eq!(queue.len(), COORDINATOR_SHARE);
        assert_eq!(transport.len(), 20 - COORDINATOR_SHARE);

        let parked = drain(&mut queue);
        read_upstream(&mut queue, &mut transport);
        let rest = drain(&mut queue);
        let all: Vec<_> = parked.into_iter().chain(rest).collect();
        assert_eq!(
            all,
            (0..20).map(LocalWork::Coordinator).collect::<Vec<_>>(),
            "every frame, in order"
        );
    }

    #[test]
    fn the_ui_share_holds_one_event() {
        let mut queue = Queue::default();
        assert!(queue.may_take_ui_event());
        queue.park_ui("first");
        assert!(!queue.may_take_ui_event(), "the next stays in its widget");
        queue.pop();
        assert!(queue.may_take_ui_event());
    }

    #[test]
    fn a_full_ui_share_does_not_stop_coordinator_reads() {
        let mut queue = Queue::default();
        queue.park_ui("confirm");
        let mut transport: VecDeque<u32> = (0..3).collect();
        read_upstream(&mut queue, &mut transport);
        assert!(transport.is_empty());
        assert_eq!(queue.len(), 4);
    }

    #[test]
    fn a_confirmation_waits_behind_backpressured_frames() {
        let mut queue = Queue::default();
        let mut transport: VecDeque<u32> = (0..COORDINATOR_SHARE as u32 + 1).collect();
        read_upstream(&mut queue, &mut transport);
        assert!(
            !queue.may_take_ui_event(),
            "a frame is held back in the transport"
        );

        drain(&mut queue);
        assert!(
            !queue.may_take_ui_event(),
            "room alone is not enough: the held-back frame has not been read"
        );
        read_upstream(&mut queue, &mut transport);
        assert!(queue.may_take_ui_event());
        queue.park_ui("confirm");
        assert_eq!(
            drain(&mut queue),
            [
                LocalWork::Coordinator(COORDINATOR_SHARE as u32),
                LocalWork::Ui("confirm")
            ]
        );
    }

    #[test]
    fn a_cancel_discards_the_ui_events_behind_it_but_not_coordinator_messages() {
        let mut queue = Queue::default();
        queue.park_coordinator(0);
        queue.park_ui("confirm");
        queue.park_coordinator(1);
        assert_eq!(queue.pop(), Some(LocalWork::Coordinator(0)));
        queue.discard_ui_events();
        assert_eq!(drain(&mut queue), [LocalWork::Coordinator(1)]);
    }

    #[test]
    fn a_session_reset_clears_parked_work_and_backpressure() {
        let mut queue = Queue::default();
        let mut transport: VecDeque<u32> = (0..COORDINATOR_SHARE as u32 + 1).collect();
        read_upstream(&mut queue, &mut transport);
        queue.clear();
        assert!(queue.is_empty());
        assert!(queue.may_take_ui_event());
        assert!(queue.may_read_upstream());
    }

    #[test]
    #[should_panic(expected = "coordinator share is full")]
    fn parking_past_the_coordinator_share_panics() {
        let mut queue = Queue::default();
        for frame in 0..=COORDINATOR_SHARE as u32 {
            queue.park_coordinator(frame);
        }
    }
}
