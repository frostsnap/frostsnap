use super::distractor::find_closest_distractors;
use crate::{
    animation_speed::AnimationSpeed,
    circle_button::{CircleButton, CircleButtonState},
    fade_switcher::FadeSwitcher,
    gray4_style::Gray4TextStyle,
    palette::PALETTE,
    prelude::*,
    progress_bars::ProgressBars,
    touch_listener::TouchListener,
    translate::Translate,
    DefaultTextStyle, DynWidget, Key, KeyTouch, Sizing, Widget, FONT_HUGE_MONO, FONT_MED,
    FONT_SMALL,
};
use alloc::string::String;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use frost_backup::bip39_words::BIP39_WORDS;

const TOTAL_SCREENS: usize = 26; // 1 share index + 25 words
const NUM_OPTIONS: usize = 3;

/// Red color matching erase_device warning text
const WRONG_ANSWER_COLOR: Rgb565 = Rgb565::new(31, 14, 8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeedbackKind {
    Correct,
    Wrong,
}

/// xorshift32 PRNG — simple, no_std compatible
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn shuffle_3<T: Copy>(rng_state: &mut u32, arr: &mut [T; NUM_OPTIONS]) {
    for i in (1..NUM_OPTIONS).rev() {
        let j = (xorshift32(rng_state) % (i as u32 + 1)) as usize;
        arr.swap(i, j);
    }
}

fn generate_screen_content(
    current_screen: usize,
    share_index: u16,
    word_indices: &[u16; 25],
    rng_state: &mut u32,
) -> (String, QuizButtons, &'static str) {
    if current_screen == 0 {
        let correct = share_index;
        let mut options = [0u16; NUM_OPTIONS];
        options[0] = correct;

        let mut count = 1;
        let mut offset = 1u16;
        while count < NUM_OPTIONS {
            if correct >= offset {
                let alt = correct - offset;
                if !options[..count].contains(&alt) {
                    options[count] = alt;
                    count += 1;
                    if count >= NUM_OPTIONS {
                        break;
                    }
                }
            }
            let alt = correct + offset;
            if !options[..count].contains(&alt) {
                options[count] = alt;
                count += 1;
            }
            offset += 1;
        }

        shuffle_3(rng_state, &mut options);
        let labels = [
            alloc::format!("#{}", options[0]),
            alloc::format!("#{}", options[1]),
            alloc::format!("#{}", options[2]),
        ];
        let words = [
            BIP39_WORDS[options[0] as usize],
            BIP39_WORDS[options[1] as usize],
            BIP39_WORDS[options[2] as usize],
        ];
        let labels_ref: [&str; 3] = [&labels[0], &labels[1], &labels[2]];
        (
            String::from("Select Key Number"),
            QuizButtons::new(&labels_ref, words),
            BIP39_WORDS[correct as usize],
        )
    } else {
        let correct_word_index = word_indices[current_screen - 1];
        let distractors = find_closest_distractors(correct_word_index);
        let mut indices = [correct_word_index, distractors[0], distractors[1]];
        shuffle_3(rng_state, &mut indices);

        let words = [
            BIP39_WORDS[indices[0] as usize],
            BIP39_WORDS[indices[1] as usize],
            BIP39_WORDS[indices[2] as usize],
        ];
        (
            alloc::format!("Select Word {}", current_screen),
            QuizButtons::new(&words, words),
            BIP39_WORDS[correct_word_index as usize],
        )
    }
}

const BUTTON_WIDTH: u32 = 200;
const BUTTON_HEIGHT: u32 = 54;
const BUTTON_CORNER_RADIUS: u32 = 8;

#[derive(frostsnap_macros::Widget)]
struct OptionButton {
    word: &'static str,
    #[widget_delegate]
    translate: Translate<Container<Center<Text<DefaultTextStyle>>>>,
}

impl OptionButton {
    fn new(label: &str, word: &'static str) -> TouchListener<Self> {
        let text = Text::new(
            String::from(label),
            DefaultTextStyle::new(FONT_HUGE_MONO, PALETTE.primary),
        );
        let container =
            Container::with_size(Center::new(text), Size::new(BUTTON_WIDTH, BUTTON_HEIGHT))
                .with_fill(PALETTE.surface)
                .with_corner_radius(Size::new(BUTTON_CORNER_RADIUS, BUTTON_CORNER_RADIUS))
                .with_anti_aliasing(true);
        let mut translate =
            Translate::new(container, PALETTE.background).with_aggressive_framebuffer();
        translate.set_animation_speed(AnimationSpeed::DampedShake {
            half_cycles: SHAKE_HALF_CYCLES as u32,
        });
        TouchListener::new(Self { word, translate }, |_, _, is_release, child| {
            if is_release {
                None
            } else {
                Some(Key::WordSelector(child.word))
            }
        })
    }

    fn start_shake(&mut self) {
        self.translate
            .animate_to(Point::new(SHAKE_AMPLITUDE, 0), SHAKE_DURATION_MS);
    }

    fn set_style(&mut self, fill_color: Rgb565, text_color: Rgb565) {
        let container = &mut self.translate.child;
        if container.fill_color() == Some(fill_color) {
            return;
        }
        container.set_fill(fill_color);
        container
            .child
            .child
            .set_character_style(DefaultTextStyle::new(FONT_HUGE_MONO, text_color));
    }
}

/// Shake animation amplitude in pixels
const SHAKE_AMPLITUDE: i32 = 6;
/// Duration of the shake animation in ms
const SHAKE_DURATION_MS: u64 = 500;
/// Number of oscillation half-cycles during the shake
const SHAKE_HALF_CYCLES: u64 = 5;

const FADE_OUT: crate::FadeConfig = crate::FadeConfig::new(150);
const FADE_IN_BUTTONS: crate::FadeConfig = crate::FadeConfig {
    duration_ms: 250,
    speed: AnimationSpeed::Linear,
};
const FADE_IN_SCREEN: crate::FadeConfig = crate::FadeConfig {
    duration_ms: 300,
    speed: AnimationSpeed::Linear,
};

type OptionBtn = TouchListener<OptionButton>;

#[derive(frostsnap_macros::Widget)]
struct QuizButtons {
    #[widget_delegate]
    inner: Center<Column<(OptionBtn, OptionBtn, OptionBtn)>>,
}

impl QuizButtons {
    fn new(labels: &[&str; NUM_OPTIONS], words: [&'static str; NUM_OPTIONS]) -> Self {
        let btn0 = OptionButton::new(labels[0], words[0]);
        let btn1 = OptionButton::new(labels[1], words[1]);
        let btn2 = OptionButton::new(labels[2], words[2]);

        let buttons = Column::builder()
            .push(btn0)
            .gap(8)
            .push(btn1)
            .gap(8)
            .push(btn2)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self {
            inner: Center::new(buttons),
        }
    }

    fn button_mut(&mut self, index: u8) -> Option<&mut OptionButton> {
        let buttons = &mut self.inner.child;
        match index {
            0 => Some(&mut buttons.children.0.child),
            1 => Some(&mut buttons.children.1.child),
            2 => Some(&mut buttons.children.2.child),
            _ => None,
        }
    }

    fn find_button_index(&self, word: &str) -> Option<u8> {
        let buttons = &self.inner.child;
        [
            &buttons.children.0.child,
            &buttons.children.1.child,
            &buttons.children.2.child,
        ]
        .iter()
        .position(|btn| btn.word == word)
        .map(|i| i as u8)
    }

    fn set_button_style(&mut self, index: u8, fill_color: Rgb565, text_color: Rgb565) {
        if let Some(btn) = self.button_mut(index) {
            btn.set_style(fill_color, text_color);
        }
    }
}

/// Success content using Column layout with flex, matching the HoldToConfirm pattern.
#[derive(frostsnap_macros::Widget)]
struct SuccessContent {
    #[widget_delegate]
    inner: Column<(
        Center<Column<(Text<Gray4TextStyle>, SizedBox<Rgb565>, Text<Gray4TextStyle>)>>,
        CircleButton,
        SizedBox<Rgb565>,
    )>,
}

impl SuccessContent {
    fn new() -> Self {
        let title = Text::new(
            String::from("Success!"),
            Gray4TextStyle::new(FONT_MED, PALETTE.on_background),
        );

        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 15));

        let subtitle = Text::new(
            String::from("Backup Checked"),
            Gray4TextStyle::new(FONT_SMALL, PALETTE.text_secondary),
        );

        let text_column = Column::new((title, spacer, subtitle))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let text_content = Center::new(text_column);

        let mut button = CircleButton::new();
        button.set_state(CircleButtonState::ShowingCheckmark);

        let bottom_spacer = SizedBox::<Rgb565>::new(Size::new(0, 10));

        let inner = Column::builder()
            .push(text_content)
            .flex(1)
            .push(button)
            .push(bottom_spacer)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self { inner }
    }

    fn button_mut(&mut self) -> &mut CircleButton {
        &mut self.inner.children.1
    }
}

/// The quiz page: title + progress + buttons in a Column
type TitleInner = Container<Align<Text<DefaultTextStyle>>>;
type TitleWidget = Padding<crate::Switcher<TitleInner>>;

type QuizPageInner = Column<(TitleWidget, ProgressBars, FadeSwitcher<QuizButtons>)>;

#[derive(frostsnap_macros::Widget)]
struct QuizPage {
    #[widget_delegate]
    inner: QuizPageInner,
}

impl QuizPage {
    fn make_title(title_str: String) -> TitleInner {
        let text = Text::new(
            title_str,
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );
        Container::new(Align::new(text).vertical(VerticalAlignment::Top))
            .with_width(u32::MAX)
            .with_height(FONT_MED.line_height)
    }

    fn new(title_str: String, screen_idx: usize, total: usize, buttons: QuizButtons) -> Self {
        let title =
            Padding::only(crate::Switcher::new(Self::make_title(title_str)).with_shrink_to_fit())
                .top(TITLE_TOP_PAD)
                .build();

        let mut progress = ProgressBars::new(total);
        progress.progress(screen_idx);

        let content = FadeSwitcher::new(buttons)
            .with_fade_out(FADE_OUT)
            .with_fade_in(FADE_IN_BUTTONS);

        let inner = Column::builder()
            .push(title)
            .gap(TITLE_GAP)
            .push(progress)
            .push(content)
            .flex(1);

        Self { inner }
    }

    fn quiz_buttons(&self) -> &QuizButtons {
        self.inner.children.2.current()
    }

    fn quiz_buttons_mut(&mut self) -> &mut QuizButtons {
        self.inner.children.2.current_mut()
    }

    fn switch_buttons(&mut self, buttons: QuizButtons) {
        self.inner.children.2.switch_to(buttons);
    }

    fn update_title(&mut self, title_str: String) {
        self.inner
            .children
            .0
            .child
            .switch_to(Self::make_title(title_str));
    }

    fn update_progress(&mut self, screen_idx: usize) {
        self.inner.children.1.progress(screen_idx);
    }
}

type ScreenContent = crate::any_of::AnyOf<(QuizPage, SuccessContent)>;

const TITLE_TOP_PAD: u32 = 8;
const TITLE_GAP: u32 = 6;

pub struct CheckBackupScreen {
    word_indices: [u16; 25],
    share_index: u16,
    current_screen: usize,
    rng_state: u32,

    correct_word: &'static str,

    screen: FadeSwitcher<ScreenContent>,

    active_press: Option<(&'static str, u8)>,
    feedback: Option<(FeedbackKind, u8)>,
    feedback_since: Option<crate::Instant>,

    completed_since: Option<crate::Instant>,
    current_time: Option<crate::Instant>,
    checkmark_started: bool,
}

impl CheckBackupScreen {
    fn make_fade_switcher<T: Widget<Color = Rgb565>>(initial: T) -> FadeSwitcher<T> {
        FadeSwitcher::new(initial)
            .with_fade_out(FADE_OUT)
            .with_fade_in(FADE_IN_SCREEN)
    }

    pub fn new(word_indices: [u16; 25], share_index: u16, rand_seed: u32) -> Self {
        let mut rng_state = rand_seed;
        let (title, quiz, correct_word) =
            generate_screen_content(0, share_index, &word_indices, &mut rng_state);
        let page = QuizPage::new(title, 0, TOTAL_SCREENS, quiz);

        Self {
            word_indices,
            share_index,
            current_screen: 0,
            rng_state,
            correct_word,
            screen: Self::make_fade_switcher(crate::any_of::AnyOf::new(page)),
            active_press: None,
            feedback: None,
            feedback_since: None,
            completed_since: None,
            current_time: None,
            checkmark_started: false,
        }
    }

    pub fn is_verified(&self) -> bool {
        if let (Some(completed_since), Some(now)) = (self.completed_since, self.current_time) {
            let elapsed = now.saturating_duration_since(completed_since);
            return elapsed >= 1000;
        }
        false
    }

    fn quiz_page_mut(&mut self) -> Option<&mut QuizPage> {
        self.screen.current_mut().downcast_mut::<QuizPage>()
    }

    fn regenerate_content(&mut self) {
        if self.current_screen >= TOTAL_SCREENS {
            return;
        }

        let (title_str, quiz, correct_word) = generate_screen_content(
            self.current_screen,
            self.share_index,
            &self.word_indices,
            &mut self.rng_state,
        );
        self.correct_word = correct_word;
        let current_screen = self.current_screen;
        if let Some(page) = self.quiz_page_mut() {
            page.update_title(title_str);
            page.update_progress(current_screen);
            page.switch_buttons(quiz);
        }
    }

    fn advance_screen(&mut self) {
        self.current_screen += 1;
        self.feedback = None;
        self.feedback_since = None;

        if self.current_screen >= TOTAL_SCREENS {
            self.completed_since = self.current_time;
            self.screen
                .switch_to(crate::any_of::AnyOf::new(SuccessContent::new()));
        } else {
            self.regenerate_content();
        }
    }

    fn handle_press(&mut self, word: &'static str) {
        let option_index = self
            .quiz_page_mut()
            .and_then(|page| page.quiz_buttons().find_button_index(word));

        let Some(option_index) = option_index else {
            return;
        };

        let is_correct = word == self.correct_word;
        let (fill_color, text_color) = if is_correct {
            (PALETTE.tertiary_container, PALETTE.on_background)
        } else {
            (WRONG_ANSWER_COLOR, PALETTE.on_background)
        };

        if let Some(page) = self.quiz_page_mut() {
            page.quiz_buttons_mut()
                .set_button_style(option_index, fill_color, text_color);
        }

        self.active_press = Some((word, option_index));
    }

    fn cancel_press(&mut self) {
        if let Some((_, option_index)) = self.active_press.take() {
            if let Some(page) = self.quiz_page_mut() {
                page.quiz_buttons_mut().set_button_style(
                    option_index,
                    PALETTE.surface,
                    PALETTE.primary,
                );
            }
        }
    }

    fn handle_release(&mut self, current_time: crate::Instant) {
        let Some((word, option_index)) = self.active_press.take() else {
            return;
        };

        let is_correct = word == self.correct_word;
        let kind = if is_correct {
            FeedbackKind::Correct
        } else {
            if let Some(page) = self.quiz_page_mut() {
                if let Some(btn) = page.quiz_buttons_mut().button_mut(option_index) {
                    btn.start_shake();
                }
            }
            FeedbackKind::Wrong
        };

        self.feedback = Some((kind, option_index));
        self.feedback_since = Some(current_time);
    }

    fn check_feedback_timeout(&mut self) {
        let (kind, button_index) = match self.feedback {
            Some(fb) => fb,
            None => return,
        };
        let feedback_since = match self.feedback_since {
            Some(t) => t,
            None => return,
        };
        let now = match self.current_time {
            Some(t) => t,
            None => return,
        };

        let elapsed = now.saturating_duration_since(feedback_since);

        match kind {
            FeedbackKind::Correct if elapsed >= 400 => {
                self.advance_screen();
            }
            FeedbackKind::Wrong if elapsed >= 600 => {
                if let Some(page) = self.quiz_page_mut() {
                    page.quiz_buttons_mut().set_button_style(
                        button_index,
                        PALETTE.surface,
                        PALETTE.primary,
                    );
                }
                self.feedback = None;
                self.feedback_since = None;
            }
            _ => {}
        }
    }
}

impl DynWidget for CheckBackupScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.screen.set_constraints(max_size);
    }

    fn sizing(&self) -> Sizing {
        self.screen.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<KeyTouch> {
        if self.current_screen >= TOTAL_SCREENS {
            return None;
        }

        // Block input during correct feedback (about to advance)
        if matches!(self.feedback, Some((FeedbackKind::Correct, _))) {
            return None;
        }

        self.current_time = Some(current_time);

        // If wrong feedback is showing and user taps again, clear it
        if let Some((FeedbackKind::Wrong, button_index)) = self.feedback {
            if !lift_up {
                if let Some(page) = self.quiz_page_mut() {
                    page.quiz_buttons_mut().set_button_style(
                        button_index,
                        PALETTE.surface,
                        PALETTE.primary,
                    );
                }
                self.feedback = None;
                self.feedback_since = None;
            } else {
                return None;
            }
        }

        if lift_up {
            self.handle_release(current_time);
        } else {
            self.cancel_press();
            let key_touch = self.screen.handle_touch(point, current_time, lift_up);
            if let Some(key_touch) = key_touch {
                if let Key::WordSelector(word) = key_touch.key {
                    self.handle_press(word);
                }
            }
        }

        None
    }

    fn force_full_redraw(&mut self) {
        self.screen.force_full_redraw();
    }
}

impl Widget for CheckBackupScreen {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut crate::SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.current_time = Some(current_time);
        self.check_feedback_timeout();

        self.screen.draw(target, current_time)?;

        if !self.checkmark_started && self.completed_since.is_some() && self.screen.is_idle() {
            if let Some(success) = self.screen.current_mut().downcast_mut::<SuccessContent>() {
                let button = success.button_mut();
                button.force_full_redraw();
                button.checkmark_mut().start_drawing();
                self.checkmark_started = true;
            }
        }

        Ok(())
    }
}

impl core::fmt::Debug for CheckBackupScreen {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CheckBackupScreen")
            .field("current_screen", &self.current_screen)
            .field("feedback", &self.feedback)
            .finish()
    }
}
