#[derive(Debug)]
pub struct AnimationState {
    start: Option<crate::Instant>,
    last: Option<crate::Instant>,
    duration: crate::Duration,
    finished: bool,
}

impl AnimationState {
    pub fn new(duration: crate::Duration) -> Self {
        Self {
            duration,
            last: None,
            start: None,
            finished: false,
        }
    }

    pub fn reset(&mut self) {
        self.start = None;
        self.last = None;
        self.finished = false;
    }

    pub fn completion(&self) -> Option<f32> {
        let duration = self.last?.checked_duration_since(self.start?).unwrap();
        Some((duration.to_micros() as f32 / self.duration.to_micros() as f32).clamp(0.0, 1.0))
    }

    pub fn poll(&mut self, now: crate::Instant) -> AnimationProgress {
        if self.finished {
            return AnimationProgress::Done;
        }

        let _ = self.start.get_or_insert(now);
        self.last = Some(now);
        let completion = self.completion().unwrap();
        if completion == 1.0 {
            self.finished = true;
            AnimationProgress::Done
        } else {
            AnimationProgress::Progress(completion)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AnimationProgress {
    Progress(f32),
    Done,
}
