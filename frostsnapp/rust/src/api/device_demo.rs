use crate::frb_generated::StreamSink;
use flutter_rust_bridge::frb;
use frostsnap_device_demo::{Demo, DemoKind, Gesture};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const FRAME: Duration = Duration::from_millis(33);

pub enum DeviceDemoKind {
    Sign,
    VerifyAddress,
    Keygen,
    Backup,
    CheckBackup,
    EnterBackup,
    Erase,
    Upgrade,
}

pub enum DeviceDemoEvent {
    /// The device's whole screen as RGBA, sent only when it changed.
    Frame(Vec<u8>),
    /// A swipe up whose finger lands `contact_in_ms` from now.
    Swipe { contact_in_ms: u32 },
    /// A press that lands `contact_in_ms` from now and is held for `hold_ms`.
    Hold { contact_in_ms: u32, hold_ms: u32 },
}

/// A device screen played by a scripted finger. It runs until disposed.
#[frb(opaque)]
pub struct DeviceDemo {
    kind: DemoKind,
    stopped: Arc<AtomicBool>,
}

impl DeviceDemo {
    #[frb(sync)]
    pub fn new(kind: DeviceDemoKind) -> Self {
        Self {
            kind: match kind {
                DeviceDemoKind::Sign => DemoKind::Sign,
                DeviceDemoKind::VerifyAddress => DemoKind::VerifyAddress,
                DeviceDemoKind::Keygen => DemoKind::Keygen,
                DeviceDemoKind::Backup => DemoKind::Backup,
                DeviceDemoKind::CheckBackup => DemoKind::CheckBackup,
                DeviceDemoKind::EnterBackup => DemoKind::EnterBackup,
                DeviceDemoKind::Erase => DemoKind::Erase,
                DeviceDemoKind::Upgrade => DemoKind::Upgrade,
            },
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    #[frb(sync, type_64bit_int)]
    pub fn width() -> usize {
        frostsnap_device_demo::WIDTH as usize
    }

    #[frb(sync, type_64bit_int)]
    pub fn height() -> usize {
        frostsnap_device_demo::HEIGHT as usize
    }

    pub fn events(&self, sink: StreamSink<DeviceDemoEvent>) {
        let kind = self.kind;
        let stopped = self.stopped.clone();
        std::thread::spawn(move || {
            let mut demo = Demo::new(kind);
            let start = Instant::now();
            let mut next = start;
            while !stopped.load(Ordering::Relaxed) {
                let step = demo.step(start.elapsed().as_millis() as u64);
                let cues = step.cues.into_iter().map(|cue| match cue.gesture {
                    Gesture::Swipe => DeviceDemoEvent::Swipe {
                        contact_in_ms: cue.contact_in_ms,
                    },
                    Gesture::Hold { ms } => DeviceDemoEvent::Hold {
                        contact_in_ms: cue.contact_in_ms,
                        hold_ms: ms,
                    },
                });
                for event in cues.chain(step.frame.map(DeviceDemoEvent::Frame)) {
                    if sink.add(event).is_err() {
                        return;
                    }
                }
                next += FRAME;
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
        });
    }
}

impl Drop for DeviceDemo {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}
