use crate::device_demo::{self, Demo, DemoKind, Gesture, HandTiming};
use crate::frb_generated::StreamSink;
use flutter_rust_bridge::frb;
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FRAME: Duration = Duration::from_millis(33);

pub enum DeviceDemoKind {
    Sign,
    VerifyAddress,
    Keygen {
        threshold: u16,
        n_devices: u16,
        security_check_code: [u8; 4],
    },
    Backup,
    CheckBackup,
    EnterBackup,
    Erase,
    Upgrade,
}

/// The timing of the app's hand clips; see [`HandTiming`].
pub struct DeviceDemoHandTiming {
    pub swipe_contact_ms: u32,
    pub swipe_after_contact_ms: u32,
    pub hold_contact_ms: u32,
    pub hold_press_ms: u32,
    pub hold_after_release_ms: u32,
    pub tap_contact_ms: u32,
    pub tap_press_ms: u32,
    pub tap_after_release_ms: u32,
}

pub enum DeviceDemoEvent {
    /// The device's whole screen as RGBA, sent only when it changed.
    Frame(Vec<u8>),
    Gesture(DeviceDemoCue),
}

/// A gesture whose finger lands `contact_in_ms` from now.
pub struct DeviceDemoCue {
    pub contact_in_ms: u32,
    pub gesture: DeviceDemoGesture,
}

pub enum DeviceDemoGesture {
    Swipe,
    /// Held for `hold_ms`.
    Hold {
        hold_ms: u32,
    },
    /// Lands on this point of the device's screen.
    Tap {
        x: i32,
        y: i32,
    },
}

/// A device screen played by a scripted finger. It runs until disposed.
#[frb(opaque)]
pub struct DeviceDemo {
    kind: DemoKind,
    hand: HandTiming,
    started: AtomicBool,
    stopped: Arc<AtomicBool>,
}

impl DeviceDemo {
    #[frb(sync)]
    pub fn new(kind: DeviceDemoKind, hand: DeviceDemoHandTiming) -> Self {
        Self {
            kind: match kind {
                DeviceDemoKind::Sign => DemoKind::Sign,
                DeviceDemoKind::VerifyAddress => DemoKind::VerifyAddress,
                DeviceDemoKind::Keygen {
                    threshold,
                    n_devices,
                    security_check_code,
                } => DemoKind::Keygen {
                    t_of_n: (threshold, n_devices),
                    security_check_code,
                },
                DeviceDemoKind::Backup => DemoKind::Backup,
                DeviceDemoKind::CheckBackup => DemoKind::CheckBackup,
                DeviceDemoKind::EnterBackup => DemoKind::EnterBackup,
                DeviceDemoKind::Erase => DemoKind::Erase,
                DeviceDemoKind::Upgrade => DemoKind::Upgrade,
            },
            hand: HandTiming {
                swipe_contact_ms: hand.swipe_contact_ms,
                swipe_after_contact_ms: hand.swipe_after_contact_ms,
                hold_contact_ms: hand.hold_contact_ms,
                hold_press_ms: hand.hold_press_ms,
                hold_after_release_ms: hand.hold_after_release_ms,
                tap_contact_ms: hand.tap_contact_ms,
                tap_press_ms: hand.tap_press_ms,
                tap_after_release_ms: hand.tap_after_release_ms,
            },
            started: AtomicBool::new(false),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    #[frb(sync, type_64bit_int)]
    pub fn width() -> usize {
        device_demo::WIDTH as usize
    }

    #[frb(sync, type_64bit_int)]
    pub fn height() -> usize {
        device_demo::HEIGHT as usize
    }

    /// Plays the demo into `sink`. Only the first call plays; a demo is played once.
    pub fn events(&self, sink: StreamSink<DeviceDemoEvent>) {
        if self.started.swap(true, Ordering::Relaxed) {
            return;
        }
        let (kind, hand) = (self.kind, self.hand);
        let stopped = self.stopped.clone();
        // Each time a demo plays, its quiz puts the right answers in different places.
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_nanos())
            .ok()
            .and_then(NonZeroU32::new)
            .unwrap_or(NonZeroU32::MIN);
        std::thread::spawn(move || {
            let mut demo = Demo::new(kind, hand, seed);
            let start = Instant::now();
            let mut next = start;
            while !stopped.load(Ordering::Relaxed) {
                let step = demo.step(start.elapsed().as_millis() as u64);
                let cues = step.cues.into_iter().map(|cue| {
                    DeviceDemoEvent::Gesture(DeviceDemoCue {
                        contact_in_ms: cue.contact_in_ms,
                        gesture: match cue.gesture {
                            Gesture::Swipe => DeviceDemoGesture::Swipe,
                            Gesture::Hold { ms } => DeviceDemoGesture::Hold { hold_ms: ms },
                            Gesture::Tap { at } => DeviceDemoGesture::Tap { x: at.x, y: at.y },
                        },
                    })
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
