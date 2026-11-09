use crate::frb_generated::StreamSink;
use flutter_rust_bridge::frb;
use frostsnap_device_demo::{Demo, DemoKind, Gesture, HandTiming};
use std::{
    num::NonZeroUsize,
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
}

/// `device` counts along the chain from the device plugged in.
pub enum DeviceDemoEvent {
    /// A device's whole screen as RGBA, sent only when it changed.
    Frame {
        device: u32,
        pixels: Vec<u8>,
    },
    Gesture(DeviceDemoCue),
}

/// A gesture whose finger lands on `device` `contact_in_ms` from now.
pub struct DeviceDemoCue {
    pub device: u32,
    pub contact_in_ms: u32,
    pub gesture: DeviceDemoGesture,
}

pub enum DeviceDemoGesture {
    Swipe,
    /// Held for `hold_ms`.
    Hold {
        hold_ms: u32,
    },
}

/// Chained devices' screens played by a scripted finger. It runs until disposed.
#[frb(opaque)]
pub struct DeviceDemo {
    kind: DemoKind,
    devices: NonZeroUsize,
    hand: HandTiming,
    started: AtomicBool,
    stopped: Arc<AtomicBool>,
}

impl DeviceDemo {
    /// A chain of `devices` devices, at least one.
    #[frb(sync)]
    pub fn new(kind: DeviceDemoKind, devices: u32, hand: DeviceDemoHandTiming) -> Self {
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
            devices: NonZeroUsize::new(devices as usize).unwrap_or(NonZeroUsize::MIN),
            hand: HandTiming {
                swipe_contact_ms: hand.swipe_contact_ms,
                swipe_after_contact_ms: hand.swipe_after_contact_ms,
                hold_contact_ms: hand.hold_contact_ms,
                hold_press_ms: hand.hold_press_ms,
                hold_after_release_ms: hand.hold_after_release_ms,
            },
            started: AtomicBool::new(false),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// How many devices the chain has, which every `device` index is below.
    #[frb(sync)]
    pub fn devices(&self) -> u32 {
        self.devices.get() as u32
    }

    #[frb(sync, type_64bit_int)]
    pub fn width() -> usize {
        frostsnap_device_demo::WIDTH as usize
    }

    #[frb(sync, type_64bit_int)]
    pub fn height() -> usize {
        frostsnap_device_demo::HEIGHT as usize
    }

    /// Plays the demo into `sink`. Only the first call plays; a demo is played once.
    pub fn events(&self, sink: StreamSink<DeviceDemoEvent>) {
        if self.started.swap(true, Ordering::Relaxed) {
            return;
        }
        let (kind, devices, hand) = (self.kind, self.devices, self.hand);
        let stopped = self.stopped.clone();
        std::thread::spawn(move || {
            let mut demo = Demo::new(kind, devices, hand);
            let start = Instant::now();
            let mut next = start;
            while !stopped.load(Ordering::Relaxed) {
                let step = demo.step(start.elapsed().as_millis() as u64);
                let cues = step.cues.into_iter().map(|cue| {
                    DeviceDemoEvent::Gesture(DeviceDemoCue {
                        device: cue.device as u32,
                        contact_in_ms: cue.contact_in_ms,
                        gesture: match cue.gesture {
                            Gesture::Swipe => DeviceDemoGesture::Swipe,
                            Gesture::Hold { ms } => DeviceDemoGesture::Hold { hold_ms: ms },
                        },
                    })
                });
                let frames =
                    step.frames
                        .into_iter()
                        .map(|(device, pixels)| DeviceDemoEvent::Frame {
                            device: device as u32,
                            pixels,
                        });
                for event in cues.chain(frames) {
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
