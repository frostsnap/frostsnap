//! The device's own screens, played by a scripted finger, for the app to show how to work the
//! device.
//!
//! Only the device is rendered here. The app draws the hand from the [`Cue`]s, so a cue goes out
//! [`Cue::contact_in_ms`] before the finger lands, giving the hand time to reach the screen.
//! Amounts, addresses, codes and words are placeholders and are blurred (see
//! `frostsnap_widgets::redact`).

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
    sign_prompt,
    vec_framebuffer::VecFramebuffer,
    AddressWithIndex, Center, DynWidget, EraseDevice, FirmwareUpgradeConfirm, Instant, KeygenCheck,
    SignTxPrompt, SuperDrawTarget, Widget, WidgetList, HOLD_TO_CONFIRM_TIME_LONG_MS,
    HOLD_TO_CONFIRM_TIME_MS, HOLD_TO_CONFIRM_TIME_SHORT_MS,
};
use std::str::FromStr;

pub const WIDTH: u32 = 240;
pub const HEIGHT: u32 = 280;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoKind {
    Sign,
    VerifyAddress,
    Keygen,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cue {
    pub gesture: Gesture,
    pub contact_in_ms: u32,
}

#[derive(Default)]
pub struct Step {
    pub cues: Vec<Cue>,
    /// The whole screen as RGBA, present only when it changed.
    pub frame: Option<Vec<u8>>,
}

// These match the hand clips the app plays: where in each clip the finger meets the screen, how
// long the finger stays down in an unstretched hold, and how long the clip runs on after it lets
// go. The next gesture waits for the hand to get back.
const SWIPE_APPROACH_MS: u64 = 1000;
const SWIPE_AFTER_CONTACT_MS: u64 = 2167;
const HOLD_APPROACH_MS: u64 = 1500;
const HOLD_CLIP_PRESS_MS: u32 = 3067;
const HOLD_AFTER_RELEASE_MS: u64 = 2433;
const REST_BEFORE_REPLAY_MS: u64 = 1000;

const SWIPE_X: i32 = 120;
const SWIPE_FROM_Y: i32 = 222;
const SWIPE_TO_Y: i32 = 10;
const SWIPE_STEPS: i32 = 3;
const FRAME_MS: u64 = 33;
const TAP_MS: u64 = 120;
const HOLD_BUTTON: Point = Point::new(120, 215);
// The quiz's shuffle is xorshift, which never moves from a zero seed.
const QUIZ_SEED: u32 = 0x5eed;
const QUIZ_OPTIONS: [Point; 3] = [
    Point::new(120, 104),
    Point::new(120, 166),
    Point::new(120, 228),
];

/// One thing the finger does. Only swipes and holds have a hand drawn for them; taps on a
/// keyboard or quiz play on the screen alone.
#[derive(Clone, Copy, Debug)]
enum Action {
    Swipe,
    /// Held a little past the screen's threshold, as a person's hold would be, and never shorter
    /// than the hand clip's own press.
    Hold {
        threshold_ms: u32,
    },
    Tap(Point),
    AnswerQuiz,
    Pause(u64),
}

enum Input {
    Press(Point),
    Drag { from_y: i32, to_y: i32 },
    // The device only turns the page when a drag is released, and only if the release moved.
    SwipeRelease { from_y: i32, to_y: i32 },
    Release(Point),
    PressRightAnswer,
    ReleaseAnswer,
}

enum Event {
    Cue(Cue),
    Input(Input),
}

/// Object-safe face of a screen widget, so every kind runs through one loop.
trait Screen {
    fn draw(&mut self, fb: &mut VecFramebuffer<Rgb565>, now: Instant);
    fn input(&mut self, input: &Input, now: Instant);
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
        Input::PressRightAnswer | Input::ReleaseAnswer => {
            unreachable!("only the quiz is scripted to answer")
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

/// The quiz shuffles its options, so where the right answer is is only known when it is asked.
struct Quiz {
    screen: CheckBackupScreen,
    pressed: Option<Point>,
}

impl Screen for Quiz {
    fn draw(&mut self, fb: &mut VecFramebuffer<Rgb565>, now: Instant) {
        draw_widget(&mut self.screen, fb, now);
    }

    fn input(&mut self, input: &Input, now: Instant) {
        match input {
            Input::PressRightAnswer => {
                if let Some(option) = self.screen.correct_option() {
                    let point = QUIZ_OPTIONS[option as usize];
                    self.screen.handle_touch(point, now, false);
                    self.pressed = Some(point);
                }
            }
            Input::ReleaseAnswer => {
                if let Some(point) = self.pressed.take() {
                    self.screen.handle_touch(point, now, true);
                }
            }
            input => touch(&mut self.screen, input, now),
        }
    }
}

pub struct Demo {
    kind: DemoKind,
    screen: Box<dyn Screen>,
    timeline: Vec<(u64, Event)>,
    next_event: usize,
    loop_ms: u64,
    loop_started_at: u64,
    fb: VecFramebuffer<Rgb565>,
    last_frame: Vec<u8>,
}

impl Demo {
    pub fn new(kind: DemoKind) -> Self {
        let (timeline, loop_ms) = schedule(&script(kind));
        Self {
            kind,
            screen: screen(kind),
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
        let mut step = Step::default();
        if now_ms >= self.loop_started_at + self.loop_ms {
            self.screen = screen(self.kind);
            self.next_event = 0;
            self.loop_started_at = now_ms;
            self.fb.clear(PALETTE.background);
        }
        let now = Instant::from_millis(now_ms);
        while let Some((at, event)) = self.timeline.get(self.next_event) {
            if self.loop_started_at + at > now_ms {
                break;
            }
            match event {
                Event::Cue(cue) => step.cues.push(*cue),
                Event::Input(input) => self.screen.input(input, now),
            }
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

fn sized<W: Widget<Color = Rgb565>>(mut widget: W) -> W {
    widget.set_constraints(Size::new(WIDTH, HEIGHT));
    widget
}

fn screen(kind: DemoKind) -> Box<dyn Screen> {
    match kind {
        DemoKind::Sign => Box::new(Plain(sized(SignTxPrompt::new_with_seed(
            placeholder_sign_prompt(),
            0,
        )))),
        DemoKind::VerifyAddress => Box::new(Plain(sized(Center::new(
            AddressWithIndex::new_with_seed(placeholder_address(), 7, 0),
        )))),
        DemoKind::Keygen => Box::new(Plain(sized(KeygenCheck::new(
            (2, 3),
            [0xa3, 0x5f, 0x19, 0xc2],
        )))),
        DemoKind::Backup => Box::new(Plain(sized(BackupDisplay::new(placeholder_words(), 1)))),
        DemoKind::CheckBackup => Box::new(Quiz {
            screen: sized(CheckBackupScreen::new(placeholder_words(), 1, QUIZ_SEED)),
            pressed: None,
        }),
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
        DemoKind::Keygen => vec![Action::Hold {
            threshold_ms: HOLD_TO_CONFIRM_TIME_MS,
        }],
        // The full backup is a dozen pages; the first few show how to page through it.
        DemoKind::Backup => swipes(3).collect(),
        DemoKind::CheckBackup => {
            let mut actions = vec![Action::Pause(1200)];
            for _ in 0..4 {
                actions.extend([Action::AnswerQuiz, Action::Pause(1400)]);
            }
            actions
        }
        DemoKind::EnterBackup => {
            let tap = |x, y, after| [Action::Tap(Point::new(x, y)), Action::Pause(after)];
            let mut actions = vec![Action::Pause(1200)];
            for (x, y, after) in [
                (41, 147, 520),
                (120, 93, 520),
                (195, 255, 900),
                (90, 82, 520),
                (30, 182, 900),
                (90, 147, 1000),
                (210, 182, 520),
                (30, 82, 700),
                (90, 92, 900),
                (175, 96, 1800),
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
                let contact = t + SWIPE_APPROACH_MS;
                timeline.push((
                    t,
                    Event::Cue(Cue {
                        gesture: Gesture::Swipe,
                        contact_in_ms: SWIPE_APPROACH_MS as u32,
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
                t = contact + SWIPE_AFTER_CONTACT_MS;
            }
            Action::Hold { threshold_ms } => {
                let ms = (threshold_ms + 100).max(HOLD_CLIP_PRESS_MS);
                let contact = t + HOLD_APPROACH_MS;
                timeline.push((
                    t,
                    Event::Cue(Cue {
                        gesture: Gesture::Hold { ms },
                        contact_in_ms: HOLD_APPROACH_MS as u32,
                    }),
                ));
                timeline.push((contact, Event::Input(Input::Press(HOLD_BUTTON))));
                let release = contact + ms as u64;
                timeline.push((release, Event::Input(Input::Release(HOLD_BUTTON))));
                t = release + HOLD_AFTER_RELEASE_MS;
            }
            Action::Tap(point) => {
                timeline.push((t, Event::Input(Input::Press(point))));
                t += TAP_MS;
                timeline.push((t, Event::Input(Input::Release(point))));
            }
            Action::AnswerQuiz => {
                timeline.push((t, Event::Input(Input::PressRightAnswer)));
                t += TAP_MS;
                timeline.push((t, Event::Input(Input::ReleaseAnswer)));
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

    const KINDS: [DemoKind; 8] = [
        DemoKind::Sign,
        DemoKind::VerifyAddress,
        DemoKind::Keygen,
        DemoKind::Backup,
        DemoKind::CheckBackup,
        DemoKind::EnterBackup,
        DemoKind::Erase,
        DemoKind::Upgrade,
    ];

    fn play(screen: &mut dyn Screen, actions: &[Action]) {
        let (timeline, loop_ms) = schedule(actions);
        let mut fb = VecFramebuffer::new(WIDTH as usize, HEIGHT as usize);
        let mut events = timeline.iter().peekable();
        for now_ms in (0..loop_ms).step_by(FRAME_MS as usize) {
            let now = Instant::from_millis(now_ms);
            while let Some((_, event)) = events.next_if(|(at, _)| *at <= now_ms) {
                if let Event::Input(input) = event {
                    screen.input(input, now);
                }
            }
            screen.draw(&mut fb, now);
        }
    }

    #[test]
    fn each_cue_leads_its_touch_by_contact_in_ms() {
        for kind in KINDS {
            let (timeline, _) = schedule(&script(kind));
            for (i, (at, event)) in timeline.iter().enumerate() {
                if let Event::Cue(cue) = event {
                    let (press_at, _) = timeline[i + 1..]
                        .iter()
                        .find(|(_, e)| matches!(e, Event::Input(Input::Press(_))))
                        .expect("every cue has a touch");
                    assert_eq!(*press_at, at + cue.contact_in_ms as u64, "{kind:?}");
                }
            }
        }
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
        play(&mut keygen, &script(DemoKind::Keygen));
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
        let mut quiz = Quiz {
            screen: sized(CheckBackupScreen::new(placeholder_words(), 1, QUIZ_SEED)),
            pressed: None,
        };
        let mut actions = vec![Action::Pause(1200)];
        for _ in 0..26 {
            actions.extend([Action::AnswerQuiz, Action::Pause(1400)]);
        }
        play(&mut quiz, &actions);
        assert!(quiz.screen.is_verified());
    }

    #[test]
    fn every_kind_replays_and_sends_only_changed_screens() {
        for kind in KINDS {
            let mut demo = Demo::new(kind);
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

    fn frame_at(kind: DemoKind, at_ms: u64) -> Vec<u8> {
        let mut demo = Demo::new(kind);
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
    fn placeholder_values_are_blurred_and_their_labels_are_not() {
        let keygen = frame_at(DemoKind::Keygen, 990);
        assert!(
            !has_sharp_edges(&keygen, (40, 66, 200, 118)),
            "threshold and code"
        );
        assert!(
            has_sharp_edges(&keygen, (20, 12, 220, 30)),
            "check this code"
        );
        assert!(
            has_sharp_edges(&keygen, (40, 132, 200, 150)),
            "hold to confirm"
        );

        let verify = frame_at(DemoKind::VerifyAddress, 990);
        assert!(
            !has_sharp_edges(&verify, (0, 0, 239, 280)),
            "address and its index"
        );

        let typing_key_number = frame_at(DemoKind::EnterBackup, 1980);
        assert!(
            !has_sharp_edges(&typing_key_number, (0, 6, 110, 46)),
            "key number"
        );
        assert!(
            has_sharp_edges(&typing_key_number, (180, 18, 235, 42)),
            "backspace"
        );

        let typing_word = frame_at(DemoKind::EnterBackup, 3960);
        assert!(
            // Above the cursor, which is drawn sharp under the text.
            !has_sharp_edges(&typing_word, (0, 6, 110, 34)),
            "entered word"
        );
        assert!(has_sharp_edges(&typing_word, (0, 72, 239, 96)), "keyboard");
    }
}
