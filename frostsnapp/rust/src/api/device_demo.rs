use crate::device_demo::{self, Demo, DemoKind};
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

pub enum DeviceDemoEvent {
    /// The device's whole screen as RGBA, sent only when it changed.
    Frame(Vec<u8>),
    Gesture(DeviceDemoCue),
}

/// A gesture of the hand, whose finger lands `contact_in_ms` from now.
pub struct DeviceDemoCue {
    pub contact_in_ms: u32,
    /// Played in order. Before, between and after them the hand keeps the last frame shown.
    pub clips: Vec<DeviceDemoClip>,
    /// When the hand is back at rest.
    pub settle_in_ms: i32,
    /// How far, in the device screen's pixels, the hand moves from where the clips put the
    /// fingertip, reaching it by the touch.
    pub offset_x: i32,
    pub offset_y: i32,
}

/// One of the hand's animated clips, an asset, starting `start_in_ms` from now (or that long ago,
/// if negative).
pub struct DeviceDemoClip {
    pub asset: String,
    pub start_in_ms: i32,
}

/// A device screen played by a scripted finger. It runs until disposed.
#[frb(opaque)]
pub struct DeviceDemo {
    kind: DemoKind,
    started: AtomicBool,
    stopped: Arc<AtomicBool>,
}

impl DeviceDemo {
    #[frb(sync)]
    pub fn new(kind: DeviceDemoKind) -> Self {
        Self {
            kind: kind.into(),
            started: AtomicBool::new(false),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The screen one play of `kind` ends on, as RGBA.
    #[frb(sync)]
    pub fn end_screen(kind: DeviceDemoKind) -> Vec<u8> {
        device_demo::end_frame(kind.into())
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
        let kind = self.kind;
        let stopped = self.stopped.clone();
        // Each time a demo plays, its quiz puts the right answers in different places.
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_nanos())
            .ok()
            .and_then(NonZeroU32::new)
            .unwrap_or(NonZeroU32::MIN);
        std::thread::spawn(move || {
            let mut demo = Demo::new(kind, seed);
            let start = Instant::now();
            let mut next = start;
            while !stopped.load(Ordering::Relaxed) {
                let step = demo.step(start.elapsed().as_millis() as u64);
                let cues = step.cues.into_iter().map(|cue| {
                    let hand = cue.hand();
                    DeviceDemoEvent::Gesture(DeviceDemoCue {
                        contact_in_ms: cue.contact_in_ms,
                        clips: hand
                            .clips
                            .into_iter()
                            .map(|clip| DeviceDemoClip {
                                asset: clip.asset.to_string(),
                                start_in_ms: clip.start_in_ms,
                            })
                            .collect(),
                        settle_in_ms: hand.settle_in_ms,
                        offset_x: hand.offset.0,
                        offset_y: hand.offset.1,
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

impl From<DeviceDemoKind> for DemoKind {
    fn from(kind: DeviceDemoKind) -> Self {
        match kind {
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
        }
    }
}

impl Drop for DeviceDemo {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}
