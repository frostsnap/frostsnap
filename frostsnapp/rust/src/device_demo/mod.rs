//! The device's own screens, played by a scripted finger, for the app to show how to work the
//! device.
//!
//! Only the device is rendered here. The app draws the hand by playing each [`Cue`]'s
//! [`Cue::clips`], and a cue goes out [`Cue::contact_in_ms`] before the finger lands, giving the
//! hand time to reach the screen. The clips' timing is [`hand_gestures`], generated with them.
//! Amounts, addresses, hashes and words are placeholders and are blurred (see
//! `frostsnap_widgets::redact`). The security check is given the real code instead: what the user
//! does there is check that every device shows it.

use embedded_graphics::{
    geometry::{Point, Size},
    pixelcolor::{Rgb565, RgbColor},
};
use frostsnap_core::bitcoin_transaction::{
    PromptDestination, PromptRecipient, PromptSignBitcoinTx,
};
use frostsnap_widgets::{
    backup::{BackupDisplay, CheckBackupScreen, EnterShareScreen},
    palette::PALETTE,
    redact, sign_prompt,
    vec_framebuffer::VecFramebuffer,
    AddressWithIndex, Center, EraseDevice, FirmwareUpgradeConfirm, Instant, KeygenCheck,
    SignTxPrompt, SuperDrawTarget, Widget, WidgetList, HOLD_TO_CONFIRM_TIME_LONG_MS,
    HOLD_TO_CONFIRM_TIME_MS, HOLD_TO_CONFIRM_TIME_SHORT_MS,
};
use hand_gestures::{FINGERTIP, HOLD, SWIPE, TAP};
use std::{num::NonZeroU32, str::FromStr};

pub mod hand_gestures;

pub const WIDTH: u32 = 240;
pub const HEIGHT: u32 = 280;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoKind {
    Sign,
    VerifyAddress,
    Keygen {
        t_of_n: (u16, u16),
        security_check_code: [u8; 4],
    },
    Backup,
    CheckBackup,
    EnterBackup,
    Erase,
    Upgrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Swipe,
    Hold { ms: u32 },
    Tap { at: Point },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cue {
    pub gesture: Gesture,
    pub contact_in_ms: u32,
}

/// One of the hand's clips, starting `start_in_ms` after its cue goes out (before it, if
/// negative).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipStart {
    pub asset: &'static str,
    pub start_in_ms: i32,
}

// The touch lands on a whole millisecond, so put it mid-frame: either side of the frame's edge it
// would show the frame before or after it.
const HALF_FRAME_MS: i64 = 500 / hand_gestures::FPS as i64;

/// How the app plays a cue's hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hand {
    /// Played in order. Between two clips, and after the last, the hand keeps the clip's final
    /// frame: for a hold, the finger still pressed.
    pub clips: Vec<ClipStart>,
    /// When the hand is back at rest.
    pub settle_in_ms: i32,
    /// How far, in screen pixels, the hand moves from where its clips put the fingertip so the
    /// finger lands where the gesture touches.
    pub offset: (i32, i32),
}

impl Cue {
    pub fn hand(&self) -> Hand {
        let contact = i64::from(self.contact_in_ms);
        let at = |clip: &hand_gestures::Clip, touch: i64, point_ms: u32| {
            (
                ClipStart {
                    asset: clip.asset,
                    start_in_ms: (touch - i64::from(point_ms) - HALF_FRAME_MS) as i32,
                },
                clip.length_ms,
            )
        };
        let (clips, press): (Vec<_>, Point) = match self.gesture {
            Gesture::Swipe => (
                vec![at(&SWIPE.clip, contact, SWIPE.contact_ms)],
                Point::new(SWIPE_X, SWIPE_FROM_Y),
            ),
            Gesture::Tap { at: target } => (vec![at(&TAP.clip, contact, TAP.contact_ms)], target),
            Gesture::Hold { ms } => (
                vec![
                    at(&HOLD.press, contact, HOLD.contact_ms),
                    at(&HOLD.release, contact + i64::from(ms), HOLD.lift_ms),
                ],
                HOLD_BUTTON,
            ),
        };
        let (last, length_ms) = clips.last().expect("every gesture has a clip");
        Hand {
            settle_in_ms: last.start_in_ms + *length_ms as i32,
            clips: clips.iter().map(|(clip, _)| *clip).collect(),
            offset: (press.x - FINGERTIP.0, press.y - FINGERTIP.1),
        }
    }
}

#[derive(Default)]
pub struct Step {
    pub cues: Vec<Cue>,
    /// The whole screen as RGBA, present only when it changed.
    pub frame: Option<Vec<u8>>,
}

const REST_BEFORE_REPLAY_MS: u64 = 1000;

const SWIPE_X: i32 = 120;
const SWIPE_FROM_Y: i32 = 222;
const SWIPE_TO_Y: i32 = 10;
const SWIPE_STEPS: i32 = 3;
const FRAME_MS: u64 = 33;
const HOLD_BUTTON: Point = Point::new(120, 215);
const QUIZ_OPTIONS: [Point; 3] = [
    Point::new(120, 104),
    Point::new(120, 166),
    Point::new(120, 228),
];

/// One thing the finger does.
#[derive(Clone, Copy, Debug)]
enum Action {
    Swipe,
    /// Held a little past the screen's threshold, as a person's hold would be, and never shorter
    /// than the hand clip's own press.
    Hold {
        threshold_ms: u32,
    },
    Tap(Target),
    Pause(u64),
}

#[derive(Clone, Copy, Debug)]
enum Target {
    At(Point),
    /// The quiz shuffles its options, so where the right answer is is only known when it is asked.
    RightAnswer,
}

enum Input {
    Press(Point),
    Drag { from_y: i32, to_y: i32 },
    // The device only turns the page when a drag is released, and only if the release moved.
    SwipeRelease { from_y: i32, to_y: i32 },
    Release(Point),
}

enum Event {
    Cue(Cue),
    TapCue { target: Target, contact_in_ms: u32 },
    Input(Input),
    TapPress,
    TapRelease,
}

/// Object-safe face of a screen widget, so every kind runs through one loop.
trait Screen {
    fn draw(&mut self, fb: &mut VecFramebuffer<Rgb565>, now: Instant);
    fn input(&mut self, input: &Input, now: Instant);

    fn right_answer(&mut self) -> Option<Point> {
        None
    }
}

fn draw_widget<W: Widget<Color = Rgb565>>(
    widget: &mut W,
    fb: &mut VecFramebuffer<Rgb565>,
    now: Instant,
) {
    let mut target = SuperDrawTarget::new(fb, PALETTE.background);
    widget.draw(&mut target, now).unwrap();
}

fn touch<W: Widget<Color = Rgb565>>(widget: &mut W, input: &Input, now: Instant) {
    match *input {
        Input::Press(point) => {
            widget.handle_touch(point, now, false);
        }
        Input::Drag { from_y, to_y } => {
            widget.handle_vertical_drag(Some(from_y as u32), to_y as u32, false);
        }
        Input::SwipeRelease { from_y, to_y } => {
            widget.handle_vertical_drag(Some(from_y as u32), to_y as u32, true);
            widget.handle_touch(Point::new(SWIPE_X, to_y), now, true);
        }
        Input::Release(point) => {
            widget.handle_touch(point, now, true);
        }
    }
}

struct Plain<W>(W);

impl<W: Widget<Color = Rgb565>> Screen for Plain<W> {
    fn draw(&mut self, fb: &mut VecFramebuffer<Rgb565>, now: Instant) {
        draw_widget(&mut self.0, fb, now);
    }

    fn input(&mut self, input: &Input, now: Instant) {
        touch(&mut self.0, input, now);
    }
}

struct Quiz(CheckBackupScreen);

impl Screen for Quiz {
    fn draw(&mut self, fb: &mut VecFramebuffer<Rgb565>, now: Instant) {
        draw_widget(&mut self.0, fb, now);
    }

    fn input(&mut self, input: &Input, now: Instant) {
        touch(&mut self.0, input, now);
    }

    fn right_answer(&mut self) -> Option<Point> {
        Some(QUIZ_OPTIONS[self.0.correct_option()? as usize])
    }
}

/// Where the finger's tap lands, fixed when its cue goes out, so the touch is where the hand went.
#[derive(Default)]
struct Finger {
    tap_at: Option<Point>,
}

impl Finger {
    fn play(&mut self, event: &Event, screen: &mut dyn Screen, now: Instant) -> Option<Cue> {
        match *event {
            Event::Cue(cue) => return Some(cue),
            Event::TapCue {
                target,
                contact_in_ms,
            } => {
                self.tap_at = match target {
                    Target::At(point) => Some(point),
                    Target::RightAnswer => screen.right_answer(),
                };
                return self.tap_at.map(|at| Cue {
                    gesture: Gesture::Tap { at },
                    contact_in_ms,
                });
            }
            Event::Input(ref input) => screen.input(input, now),
            Event::TapPress => {
                if let Some(at) = self.tap_at {
                    screen.input(&Input::Press(at), now);
                }
            }
            Event::TapRelease => {
                if let Some(at) = self.tap_at.take() {
                    screen.input(&Input::Release(at), now);
                }
            }
        }
        None
    }
}

pub struct Demo {
    kind: DemoKind,
    seed: NonZeroU32,
    screen: Box<dyn Screen>,
    finger: Finger,
    timeline: Vec<(u64, Event)>,
    next_event: usize,
    loop_ms: u64,
    loop_started_at: u64,
    fb: VecFramebuffer<Rgb565>,
    last_frame: Vec<u8>,
}

impl Demo {
    /// `seed` shuffles the first play's quiz; each replay shuffles it differently.
    pub fn new(kind: DemoKind, seed: NonZeroU32) -> Self {
        let _redacting = redact::enable_on_this_thread();
        let (timeline, loop_ms) = schedule(&script(kind));
        Self {
            kind,
            seed,
            screen: screen(kind, seed),
            finger: Finger::default(),
            timeline,
            next_event: 0,
            loop_ms,
            loop_started_at: 0,
            fb: VecFramebuffer::new(WIDTH as usize, HEIGHT as usize),
            last_frame: Vec::new(),
        }
    }

    /// Advances the screen to `now_ms`, which must not go backwards.
    pub fn step(&mut self, now_ms: u64) -> Step {
        // Screens build some of their widgets as they are paged through, not only up front.
        let _redacting = redact::enable_on_this_thread();
        let mut step = Step::default();
        if now_ms >= self.loop_started_at + self.loop_ms {
            self.seed = next_play(self.seed);
            self.screen = screen(self.kind, self.seed);
            self.finger = Finger::default();
            self.next_event = 0;
            self.loop_started_at = now_ms;
            self.fb.clear(PALETTE.background);
        }
        let now = Instant::from_millis(now_ms);
        while let Some((at, event)) = self.timeline.get(self.next_event) {
            if self.loop_started_at + at > now_ms {
                break;
            }
            step.cues
                .extend(self.finger.play(event, self.screen.as_mut(), now));
            self.next_event += 1;
        }
        self.screen.draw(&mut self.fb, now);
        if self.fb.take_dirty() {
            let frame = rgba(&self.fb);
            if frame != self.last_frame {
                self.last_frame = frame.clone();
                step.frame = Some(frame);
            }
        }
        step
    }
}

/// The screen one play of `kind` ends on, as RGBA: for a confirmation, the widget after the
/// finger has confirmed it, which is what the device keeps showing.
pub fn end_frame(kind: DemoKind) -> Vec<u8> {
    let mut demo = Demo::new(kind, NonZeroU32::MIN);
    let mut last = Vec::new();
    // At `loop_ms` the demo starts its next play.
    for now_ms in (0..demo.loop_ms).step_by(FRAME_MS as usize) {
        if let Some(frame) = demo.step(now_ms).frame {
            last = frame;
        }
    }
    last
}

fn rgba(fb: &VecFramebuffer<Rgb565>) -> Vec<u8> {
    let mut out = Vec::with_capacity(fb.width() * fb.height() * 4);
    for color in fb.contiguous_pixels() {
        out.push((color.r() << 3) | (color.r() >> 2));
        out.push((color.g() << 2) | (color.g() >> 4));
        out.push((color.b() << 3) | (color.b() >> 2));
        out.push(0xff);
    }
    out
}

// An odd multiplier permutes the u32s and keeps zero at zero, so a non-zero seed stays non-zero,
// which the quiz's xorshift shuffle needs to move at all.
fn next_play(seed: NonZeroU32) -> NonZeroU32 {
    NonZeroU32::new(seed.get().wrapping_mul(0x9e37_79b9)).expect("non-zero stays non-zero")
}

fn sized<W: Widget<Color = Rgb565>>(mut widget: W) -> W {
    widget.set_constraints(Size::new(WIDTH, HEIGHT));
    widget
}

fn screen(kind: DemoKind, seed: NonZeroU32) -> Box<dyn Screen> {
    match kind {
        DemoKind::Sign => Box::new(Plain(sized(SignTxPrompt::new_with_seed(
            placeholder_sign_prompt(),
            0,
        )))),
        DemoKind::VerifyAddress => Box::new(Plain(sized(Center::new(
            AddressWithIndex::new_with_seed(placeholder_address(), 7, 0),
        )))),
        DemoKind::Keygen {
            t_of_n,
            security_check_code,
        } => Box::new(Plain(sized(KeygenCheck::new(t_of_n, security_check_code)))),
        DemoKind::Backup => Box::new(Plain(sized(BackupDisplay::new(placeholder_words(), 1)))),
        DemoKind::CheckBackup => Box::new(Quiz(sized(CheckBackupScreen::new(
            placeholder_words(),
            1,
            seed.get(),
        )))),
        DemoKind::EnterBackup => Box::new(Plain(sized(EnterShareScreen::new()))),
        DemoKind::Erase => Box::new(Plain(sized(EraseDevice::new()))),
        DemoKind::Upgrade => Box::new(Plain(sized(FirmwareUpgradeConfirm::new(
            core::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(11)),
            1_468_006,
        )))),
    }
}

fn script(kind: DemoKind) -> Vec<Action> {
    let swipes = |n: usize| core::iter::repeat_n(Action::Swipe, n);
    match kind {
        DemoKind::Sign => {
            let pages =
                sign_prompt::SignPromptPageList::new_with_seed(placeholder_sign_prompt(), 0).len();
            swipes(pages - 1)
                .chain([Action::Hold {
                    threshold_ms: sign_prompt::HOLD_TO_SIGN_TIME_MS,
                }])
                .collect()
        }
        DemoKind::VerifyAddress => vec![Action::Pause(10_000)],
        DemoKind::Keygen { .. } => vec![Action::Hold {
            threshold_ms: HOLD_TO_CONFIRM_TIME_MS,
        }],
        // The full backup is a dozen pages; the first few show how to page through it.
        DemoKind::Backup => swipes(3).collect(),
        DemoKind::CheckBackup => {
            let mut actions = vec![Action::Pause(1200)];
            for _ in 0..4 {
                actions.extend([Action::Tap(Target::RightAnswer), Action::Pause(400)]);
            }
            actions
        }
        DemoKind::EnterBackup => {
            let tap = |x, y, after| {
                [
                    Action::Tap(Target::At(Point::new(x, y))),
                    Action::Pause(after),
                ]
            };
            let mut actions = vec![Action::Pause(1200)];
            for (x, y, after) in [
                // Hardly anyone has ten or more devices, so the key number is one digit.
                (41, 147, 0),
                (195, 255, 380),
                (90, 82, 0),
                (30, 182, 380),
                (90, 147, 480),
                (210, 182, 0),
                (30, 82, 180),
                (90, 92, 380),
                (175, 96, 1280),
            ] {
                actions.extend(tap(x, y, after));
            }
            actions
        }
        DemoKind::Erase => vec![
            Action::Swipe,
            Action::Hold {
                threshold_ms: HOLD_TO_CONFIRM_TIME_LONG_MS,
            },
        ],
        DemoKind::Upgrade => vec![Action::Hold {
            threshold_ms: HOLD_TO_CONFIRM_TIME_SHORT_MS,
        }],
    }
}

fn schedule(actions: &[Action]) -> (Vec<(u64, Event)>, u64) {
    let mut timeline = Vec::new();
    let mut t = 0;
    for action in actions {
        match *action {
            Action::Swipe => {
                let contact = t + u64::from(SWIPE.contact_ms);
                timeline.push((
                    t,
                    Event::Cue(Cue {
                        gesture: Gesture::Swipe,
                        contact_in_ms: SWIPE.contact_ms,
                    }),
                ));
                timeline.push((
                    contact,
                    Event::Input(Input::Press(Point::new(SWIPE_X, SWIPE_FROM_Y))),
                ));
                let y = |j: i32| SWIPE_FROM_Y + (SWIPE_TO_Y - SWIPE_FROM_Y) * j / SWIPE_STEPS;
                for j in 1..=SWIPE_STEPS {
                    timeline.push((
                        contact + j as u64 * FRAME_MS,
                        Event::Input(Input::Drag {
                            from_y: y(j - 1),
                            to_y: y(j),
                        }),
                    ));
                }
                timeline.push((
                    contact + (SWIPE_STEPS as u64 + 1) * FRAME_MS,
                    Event::Input(Input::SwipeRelease {
                        from_y: SWIPE_TO_Y,
                        to_y: SWIPE_TO_Y - 2,
                    }),
                ));
                t = contact + u64::from(SWIPE.clip.length_ms - SWIPE.contact_ms);
            }
            Action::Hold { threshold_ms } => {
                let clips_own_press_ms = HOLD.press.length_ms - HOLD.contact_ms + HOLD.lift_ms;
                let ms = (threshold_ms + 100).max(clips_own_press_ms);
                let contact = t + u64::from(HOLD.contact_ms);
                timeline.push((
                    t,
                    Event::Cue(Cue {
                        gesture: Gesture::Hold { ms },
                        contact_in_ms: HOLD.contact_ms,
                    }),
                ));
                timeline.push((contact, Event::Input(Input::Press(HOLD_BUTTON))));
                let release = contact + ms as u64;
                timeline.push((release, Event::Input(Input::Release(HOLD_BUTTON))));
                t = release + u64::from(HOLD.release.length_ms - HOLD.lift_ms);
            }
            Action::Tap(target) => {
                timeline.push((
                    t,
                    Event::TapCue {
                        target,
                        contact_in_ms: TAP.contact_ms,
                    },
                ));
                let contact = t + u64::from(TAP.contact_ms);
                timeline.push((contact, Event::TapPress));
                let release = contact + u64::from(TAP.lift_ms - TAP.contact_ms);
                timeline.push((release, Event::TapRelease));
                t = release + u64::from(TAP.clip.length_ms - TAP.lift_ms);
            }
            Action::Pause(ms) => t += ms,
        }
    }
    (timeline, t + REST_BEFORE_REPLAY_MS)
}

fn placeholder_address() -> bitcoin::Address {
    bitcoin::Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
        .expect("valid address")
        .assume_checked()
}

fn placeholder_words() -> [u16; 25] {
    core::array::from_fn(|i| ((i * 331 + 17) % 2048) as u16)
}

fn placeholder_sign_prompt() -> PromptSignBitcoinTx {
    PromptSignBitcoinTx {
        recipients: vec![PromptRecipient {
            destination: PromptDestination::Address(placeholder_address()),
            amount: bitcoin::Amount::from_sat(250_284),
            owned: None,
        }],
        fee: bitcoin::Amount::from_sat(284),
        fee_rate_sats_per_vbyte: Some(2.3),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const KEYGEN: DemoKind = DemoKind::Keygen {
        t_of_n: (2, 3),
        security_check_code: [0xa3, 0x5f, 0x19, 0xc2],
    };

    const SEED: NonZeroU32 = NonZeroU32::new(0x5eed).expect("non-zero");

    const KINDS: [DemoKind; 8] = [
        DemoKind::Sign,
        DemoKind::VerifyAddress,
        KEYGEN,
        DemoKind::Backup,
        DemoKind::CheckBackup,
        DemoKind::EnterBackup,
        DemoKind::Erase,
        DemoKind::Upgrade,
    ];

    fn play(screen: &mut dyn Screen, actions: &[Action]) {
        let (timeline, loop_ms) = schedule(actions);
        let mut fb = VecFramebuffer::new(WIDTH as usize, HEIGHT as usize);
        let mut finger = Finger::default();
        let mut events = timeline.iter().peekable();
        for now_ms in (0..loop_ms).step_by(FRAME_MS as usize) {
            let now = Instant::from_millis(now_ms);
            while let Some((_, event)) = events.next_if(|(at, _)| *at <= now_ms) {
                finger.play(event, screen, now);
            }
            screen.draw(&mut fb, now);
        }
    }

    /// Where the finger taps in each play of the demo, over `plays` plays.
    fn taps(kind: DemoKind, seed: NonZeroU32, plays: u64) -> Vec<Vec<Point>> {
        let mut demo = Demo::new(kind, seed);
        let mut plays_taps = vec![Vec::new(); plays as usize];
        for now_ms in (0..demo.loop_ms * plays).step_by(FRAME_MS as usize) {
            for cue in demo.step(now_ms).cues {
                if let Gesture::Tap { at } = cue.gesture {
                    plays_taps[(now_ms / demo.loop_ms) as usize].push(at);
                }
            }
        }
        plays_taps
    }

    #[test]
    fn each_cue_leads_its_touch_by_contact_in_ms() {
        for kind in KINDS {
            let (timeline, _) = schedule(&script(kind));
            for (i, (at, event)) in timeline.iter().enumerate() {
                let contact_in_ms = match event {
                    Event::Cue(cue) => cue.contact_in_ms,
                    Event::TapCue { contact_in_ms, .. } => *contact_in_ms,
                    _ => continue,
                };
                let (press_at, _) = timeline[i + 1..]
                    .iter()
                    .find(|(_, e)| matches!(e, Event::Input(Input::Press(_)) | Event::TapPress))
                    .expect("every cue has a touch");
                assert_eq!(*press_at, at + contact_in_ms as u64, "{kind:?}");
            }
        }
    }

    fn cue_of(event: &Event) -> Option<Cue> {
        match *event {
            Event::Cue(cue) => Some(cue),
            Event::TapCue { contact_in_ms, .. } => Some(Cue {
                gesture: Gesture::Tap { at: Point::zero() },
                contact_in_ms,
            }),
            _ => None,
        }
    }

    #[test]
    fn no_gesture_starts_before_the_last_one_is_back_at_rest() {
        for kind in KINDS {
            let (timeline, _) = schedule(&script(kind));
            let mut at_rest_from = 0;
            for (at, cue) in timeline.iter().filter_map(|(at, e)| Some((at, cue_of(e)?))) {
                assert!(*at as i64 >= at_rest_from, "{kind:?} at {at}");
                at_rest_from = *at as i64 + i64::from(cue.hand().settle_in_ms);
            }
        }
    }

    #[test]
    fn a_hold_releases_only_after_its_press_clip_has_played() {
        for ms in [0, HOLD_TO_CONFIRM_TIME_MS, HOLD_TO_CONFIRM_TIME_LONG_MS] {
            let (timeline, _) = schedule(&[Action::Hold { threshold_ms: ms }]);
            let cue = timeline
                .iter()
                .find_map(|(_, e)| cue_of(e))
                .expect("a hold is cued");
            let [press, release] = cue.hand().clips[..] else {
                panic!("a hold is a press and a release");
            };
            assert!(
                release.start_in_ms >= press.start_in_ms + HOLD.press.length_ms as i32,
                "{ms}"
            );
        }
    }

    /// When each frame of a checked-in clip starts, by its decoded timing, then when it ends.
    fn frame_starts(asset: &str) -> Vec<u32> {
        use image::AnimationDecoder;
        let path = format!("{}/../{asset}", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).expect("clip is checked in");
        let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(bytes))
            .expect("clip decodes");
        let mut starts = vec![0];
        for frame in decoder.into_frames() {
            let (numer, denom) = frame.expect("frame decodes").delay().numer_denom_ms();
            starts.push(starts.last().copied().unwrap_or(0) + numer / denom);
        }
        starts
    }

    #[test]
    fn the_clips_last_as_long_as_their_manifest_says() {
        for clip in [SWIPE.clip, TAP.clip, HOLD.press, HOLD.release] {
            assert_eq!(
                frame_starts(clip.asset).last().copied(),
                Some(clip.length_ms),
                "{}",
                clip.asset
            );
        }
    }

    // The encoder merges identical neighbouring frames, so this goes by the decoded clips' times:
    // when the device feels each touch, the clip shown is on the frame its manifest times it by.
    #[test]
    fn every_touch_lands_on_its_contact_or_lift_frame() {
        for kind in KINDS {
            let (timeline, _) = schedule(&script(kind));
            for (i, (cued_at, cue)) in timeline
                .iter()
                .enumerate()
                .filter_map(|(i, (at, e))| Some((i, (*at, cue_of(e)?))))
            {
                let hand = cue.hand();
                let after = |is: fn(&Event) -> bool| {
                    let (at, _) = timeline[i + 1..]
                        .iter()
                        .find(|(_, e)| is(e))
                        .expect("the cue's touch is scheduled");
                    (at - cued_at) as i64
                };
                let press =
                    || after(|e| matches!(e, Event::Input(Input::Press(_)) | Event::TapPress));
                let release =
                    || after(|e| matches!(e, Event::Input(Input::Release(_)) | Event::TapRelease));
                let touches = match cue.gesture {
                    Gesture::Swipe => vec![(press(), 0, SWIPE.contact_ms)],
                    Gesture::Tap { .. } => {
                        vec![(press(), 0, TAP.contact_ms), (release(), 0, TAP.lift_ms)]
                    }
                    Gesture::Hold { .. } => {
                        vec![(press(), 0, HOLD.contact_ms), (release(), 1, HOLD.lift_ms)]
                    }
                };
                for (touch, clip, frame_ms) in touches {
                    let clip = hand.clips[clip];
                    let elapsed = (touch - i64::from(clip.start_in_ms)) as u32;
                    let starts = frame_starts(clip.asset);
                    let showing = starts
                        .windows(2)
                        .find(|edges| edges[0] <= elapsed && elapsed < edges[1])
                        .map(|edges| edges[0]);
                    assert_eq!(showing, Some(frame_ms), "{kind:?} {}", clip.asset);
                }
            }
        }
    }

    #[test]
    fn the_hand_moves_onto_each_touch() {
        let offset = |gesture| {
            Cue {
                gesture,
                contact_in_ms: 0,
            }
            .hand()
            .offset
        };
        assert_eq!(offset(Gesture::Hold { ms: 3000 }), (0, 0));
        assert_eq!(offset(Gesture::Swipe), (0, SWIPE_FROM_Y - FINGERTIP.1));
        assert_eq!(
            offset(Gesture::Tap {
                at: Point::new(41, 147)
            }),
            (41 - FINGERTIP.0, 147 - FINGERTIP.1)
        );
    }

    #[test]
    fn the_hold_screens_confirm() {
        let mut sign = Plain(sized(SignTxPrompt::new_with_seed(
            placeholder_sign_prompt(),
            0,
        )));
        play(&mut sign, &script(DemoKind::Sign));
        assert!(sign.0.is_confirmed());

        let mut keygen = Plain(sized(KeygenCheck::new((2, 3), [0; 4])));
        play(&mut keygen, &script(KEYGEN));
        assert!(keygen.0.is_confirmed());

        let mut erase = Plain(sized(EraseDevice::new()));
        play(&mut erase, &script(DemoKind::Erase));
        assert!(erase.0.is_confirmed());

        let mut upgrade = Plain(sized(FirmwareUpgradeConfirm::new([0; 32], 1)));
        play(&mut upgrade, &script(DemoKind::Upgrade));
        assert!(upgrade.0.is_confirmed());
    }

    #[test]
    fn the_quiz_is_answered_right_every_time() {
        let mut quiz = Quiz(sized(CheckBackupScreen::new(
            placeholder_words(),
            1,
            SEED.get(),
        )));
        let mut actions = vec![Action::Pause(1200)];
        for _ in 0..26 {
            actions.extend([Action::Tap(Target::RightAnswer), Action::Pause(1400)]);
        }
        play(&mut quiz, &actions);
        assert!(quiz.0.is_verified());
    }

    #[test]
    fn the_quiz_answers_move_between_plays() {
        let first_seed = taps(DemoKind::CheckBackup, SEED, 2);
        let other_seed = taps(
            DemoKind::CheckBackup,
            NonZeroU32::new(7).expect("non-zero"),
            1,
        );
        for play in [&first_seed[0], &first_seed[1], &other_seed[0]] {
            assert_eq!(play.len(), 4);
        }
        assert_ne!(first_seed[0], first_seed[1], "a replay");
        assert_ne!(first_seed[0], other_seed[0], "another seed");
    }

    #[test]
    fn every_tap_has_a_finger_on_it() {
        let (timeline, _) = schedule(&script(DemoKind::EnterBackup));
        let touches = timeline
            .iter()
            .filter(|(_, e)| matches!(e, Event::Input(Input::Press(_))))
            .count();
        assert_eq!(touches, 0, "every key is pressed by a cued tap");
        assert_eq!(
            taps(DemoKind::EnterBackup, SEED, 1)[0].len(),
            timeline
                .iter()
                .filter(|(_, e)| matches!(e, Event::TapPress))
                .count()
        );
    }

    #[test]
    fn every_kind_replays_and_sends_only_changed_screens() {
        for kind in KINDS {
            let mut demo = Demo::new(kind, SEED);
            let mut frames = 0;
            let mut previous: Option<Vec<u8>> = None;
            for now_ms in (0..demo.loop_ms * 2).step_by(FRAME_MS as usize) {
                if let Some(frame) = demo.step(now_ms).frame {
                    assert_eq!(frame.len(), (WIDTH * HEIGHT * 4) as usize);
                    assert_ne!(previous.as_ref(), Some(&frame), "{kind:?}");
                    previous = Some(frame);
                    frames += 1;
                }
            }
            assert!(frames > 0, "{kind:?}");
        }
    }

    #[test]
    fn the_security_check_ends_on_its_confirmed_screen() {
        let end = end_frame(KEYGEN);
        let loop_ms = Demo::new(KEYGEN, SEED).loop_ms;
        assert_ne!(end, frame_at(KEYGEN, 0));
        assert_eq!(end, frame_at(KEYGEN, loop_ms - 1));
    }

    fn frame_at(kind: DemoKind, at_ms: u64) -> Vec<u8> {
        let mut demo = Demo::new(kind, SEED);
        let mut last = Vec::new();
        for now_ms in (0..=at_ms).step_by(FRAME_MS as usize) {
            if let Some(frame) = demo.step(now_ms).frame {
                last = frame;
            }
        }
        last
    }

    /// Whether any two neighbouring pixels in the rectangle differ like the edge of crisp text.
    fn has_sharp_edges(frame: &[u8], (x0, y0, x1, y1): (usize, usize, usize, usize)) -> bool {
        let luma = |x: usize, y: usize| {
            let i = (y * WIDTH as usize + x) * 4;
            (frame[i] as i32 * 3 + frame[i + 1] as i32 * 6 + frame[i + 2] as i32) / 10
        };
        (y0..y1).any(|y| (x0..x1).any(|x| (luma(x + 1, y) - luma(x, y)).abs() > 60))
    }

    #[test]
    fn the_security_check_shows_its_real_code() {
        let keygen = frame_at(KEYGEN, 990);
        assert!(
            has_sharp_edges(&keygen, (40, 66, 200, 118)),
            "threshold and code"
        );
    }

    #[test]
    fn placeholder_values_are_blurred_and_their_labels_are_not() {
        let upgrade = frame_at(DemoKind::Upgrade, 990);
        assert!(
            !has_sharp_edges(&upgrade, (40, 120, 200, 138)),
            "firmware size"
        );
        assert!(
            has_sharp_edges(&upgrade, (40, 141, 200, 159)),
            "hold to upgrade"
        );

        let address_page = frame_at(DemoKind::Sign, 3500);
        assert!(
            // Built only when the first swipe pages to it.
            !has_sharp_edges(&address_page, (10, 50, 229, 160)),
            "address"
        );
        assert!(
            has_sharp_edges(&address_page, (60, 20, 180, 40)),
            "to address"
        );

        let verify = frame_at(DemoKind::VerifyAddress, 990);
        assert!(!has_sharp_edges(&verify, (10, 50, 229, 165)), "address");
        assert!(
            has_sharp_edges(&verify, (30, 18, 210, 42)),
            "receive address #7"
        );

        let typing_key_number = frame_at(DemoKind::EnterBackup, 2200);
        assert!(
            // Above the cursor, which is drawn sharp under the text.
            !has_sharp_edges(&typing_key_number, (0, 6, 110, 34)),
            "key number"
        );
        assert!(
            has_sharp_edges(&typing_key_number, (180, 18, 235, 42)),
            "backspace"
        );

        let typing_word = frame_at(DemoKind::EnterBackup, 4600);
        assert!(
            // Above the cursor, which is drawn sharp under the text.
            !has_sharp_edges(&typing_word, (0, 6, 110, 34)),
            "entered word"
        );
        assert!(has_sharp_edges(&typing_word, (0, 72, 239, 96)), "keyboard");
    }
}
