use crate::{
    circle_button::{CircleButton, CircleButtonState},
    gray4_style::Gray4TextStyle,
    palette::PALETTE,
    prelude::*,
    progress_bars::ProgressBars,
    slide_in_transition::SlideInTransition,
    super_draw_target::SuperDrawTarget,
    vec_framebuffer::VecFramebuffer,
    DefaultTextStyle, DynWidget, KeyTouch, Sizing, Widget, FONT_HUGE_MONO, FONT_MED, FONT_SMALL,
};
use alloc::string::String;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    text::renderer::{CharacterStyle, TextRenderer},
};
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

/// Levenshtein edit distance between two byte slices.
/// BIP39 words are ASCII and max 8 chars, so a stack buffer of 9 is enough.
fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    const MAX_LEN: usize = 9; // max BIP39 word length + 1
    let mut prev = [0usize; MAX_LEN];
    let mut curr = [0usize; MAX_LEN];

    let b_len = b.len();
    for j in 0..=b_len {
        prev[j] = j;
    }

    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        // swap
        let tmp = prev;
        prev = curr;
        curr = tmp;
    }
    prev[b_len]
}

/// Length of the longest shared suffix between two byte slices.
fn shared_suffix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

/// Score how good a distractor `candidate` is for `target` (lower = better distractor).
/// Combines Levenshtein distance with a bonus for shared suffixes (rhyming).
fn distractor_score(target: &[u8], candidate: &[u8]) -> i32 {
    let edit_dist = levenshtein(target, candidate) as i32;
    let suffix = shared_suffix_len(target, candidate) as i32;
    // Weight edit distance heavily, but reward shared suffixes.
    // A 2-char shared suffix is worth ~1 edit distance reduction.
    edit_dist * 3 - suffix * 2
}

/// Find the 2 best BIP39 distractors for the word at `correct_index`.
/// Picks words that are closest in edit distance and share suffixes (rhyming).
fn find_closest_distractors(correct_index: u16) -> [u16; 2] {
    let target = BIP39_WORDS[correct_index as usize].as_bytes();

    // Track top 2: (score, index)
    let mut best = [(i32::MAX, 0u16); 2];

    for (i, &word) in BIP39_WORDS.iter().enumerate() {
        if i as u16 == correct_index {
            continue;
        }
        let score = distractor_score(target, word.as_bytes());

        if score < best[0].0 {
            best[1] = best[0];
            best[0] = (score, i as u16);
        } else if score < best[1].0 {
            best[1] = (score, i as u16);
        }
    }

    [best[0].1, best[1].1]
}

const BUTTON_WIDTH: u32 = 200;
const BUTTON_HEIGHT: u32 = 54;
const BUTTON_CORNER_RADIUS: f32 = 8.0;

/// An option button for the check backup quiz.
/// Pre-rendered into a VecFramebuffer to avoid tearing during fade-in
/// (the Fader redraws every frame, and layered fill+text would briefly
/// show the fill without the text).
struct OptionButton {
    /// The button image rendered at rest position
    fb: VecFramebuffer<Rgb565>,
    /// Scratch buffer used during shake to hold the shifted image.
    /// Only allocated while a shake is active.
    shake_fb: Option<VecFramebuffer<Rgb565>>,
    needs_redraw: bool,
    fill_color: Rgb565,
    text_color: Rgb565,
    label: String,
    shake_start: Option<crate::Instant>,
    prev_shake_offset: i32,
}

impl OptionButton {
    /// Render the button into a framebuffer at the given horizontal offset.
    /// The framebuffer is `BUTTON_FB_WIDTH` wide; `x_offset` controls where
    /// the button is drawn within it (default `SHAKE_AMPLITUDE` for centered).
    fn render_into(
        fb: &mut VecFramebuffer<Rgb565>,
        label: &str,
        fill_color: Rgb565,
        text_color: Rgb565,
        bg_color: Rgb565,
        x_offset: i32,
    ) {
        fb.clear(bg_color);

        // Draw filled rounded rect at the given offset
        let border_rect = embedded_graphics::primitives::Rectangle::new(
            Point::new(x_offset, 0),
            Size::new(BUTTON_WIDTH, BUTTON_HEIGHT),
        );
        let fill_style = embedded_graphics::primitives::PrimitiveStyleBuilder::new()
            .fill_color(fill_color)
            .build();
        embedded_graphics::primitives::RoundedRectangle::with_equal_corners(
            border_rect,
            Size::new(BUTTON_CORNER_RADIUS as u32, BUTTON_CORNER_RADIUS as u32),
        )
        .into_styled(fill_style)
        .draw(fb)
        .ok();

        // Draw SDF AA fringe for smooth corners
        let aa_pixels = crate::sdf::render_rounded_rect_fill_aa_pixels(
            x_offset,
            0,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
            BUTTON_CORNER_RADIUS,
            fill_color,
            bg_color,
        );
        fb.draw_iter(aa_pixels.into_iter()).ok();

        // Draw centered text within the button area
        let style = DefaultTextStyle::new(FONT_HUGE_MONO, text_color);
        let text_metrics = style.measure_string(
            label,
            Point::zero(),
            embedded_graphics::text::Baseline::Top,
        );
        let text_w = text_metrics.bounding_box.size.width;
        let text_h = style.line_height();
        let text_x = x_offset + (BUTTON_WIDTH.saturating_sub(text_w)) as i32 / 2;
        // Shift down by half the descender space so uppercase text looks visually centered
        let descender = text_h.saturating_sub(style.font.baseline);
        let text_y = (BUTTON_HEIGHT.saturating_sub(text_h)) as i32 / 2 + descender as i32 / 2;

        let mut text_style = DefaultTextStyle::new(FONT_HUGE_MONO, text_color);
        text_style.set_background_color(Some(fill_color));

        let text_obj = embedded_graphics::text::Text::with_text_style(
            label,
            Point::new(text_x, text_y),
            text_style,
            embedded_graphics::text::TextStyleBuilder::new()
                .baseline(embedded_graphics::text::Baseline::Top)
                .build(),
        );
        text_obj.draw(fb).ok();
    }

    fn render_fb(
        label: &str,
        fill_color: Rgb565,
        text_color: Rgb565,
        bg_color: Rgb565,
    ) -> VecFramebuffer<Rgb565> {
        let mut fb =
            VecFramebuffer::<Rgb565>::new(BUTTON_FB_WIDTH as usize, BUTTON_HEIGHT as usize);
        Self::render_into(&mut fb, label, fill_color, text_color, bg_color, SHAKE_AMPLITUDE);
        fb
    }

    fn new(label: &str) -> Self {
        let fb = Self::render_fb(label, PALETTE.surface, PALETTE.primary, PALETTE.background);
        Self {
            fb,
            shake_fb: None,
            needs_redraw: true,
            fill_color: PALETTE.surface,
            text_color: PALETTE.primary,
            label: String::from(label),
            shake_start: None,
            prev_shake_offset: 0,
        }
    }

    fn start_shake(&mut self, current_time: crate::Instant) {
        self.shake_start = Some(current_time);
        self.shake_fb = Some(VecFramebuffer::<Rgb565>::new(
            BUTTON_FB_WIDTH as usize,
            BUTTON_HEIGHT as usize,
        ));
    }

    fn set_style(&mut self, fill_color: Rgb565, text_color: Rgb565) {
        if self.fill_color != fill_color || self.text_color != text_color {
            self.fill_color = fill_color;
            self.text_color = text_color;
            self.fb = Self::render_fb(&self.label, fill_color, text_color, PALETTE.background);
            self.needs_redraw = true;
        }
    }
}

impl DynWidget for OptionButton {
    fn set_constraints(&mut self, _max_size: Size) {}

    fn sizing(&self) -> Sizing {
        Size::new(BUTTON_FB_WIDTH, BUTTON_HEIGHT).into()
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<KeyTouch> {
        None
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}

impl Widget for OptionButton {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let offset_x = if let Some(start) = self.shake_start {
            let elapsed = current_time.saturating_duration_since(start);
            let offset = shake_offset(elapsed);
            if elapsed >= SHAKE_DURATION_MS {
                self.shake_start = None;
                self.shake_fb = None;
            }
            offset
        } else {
            0
        };

        if offset_x != self.prev_shake_offset {
            self.needs_redraw = true;
            self.prev_shake_offset = offset_x;
        }

        if self.needs_redraw {
            let area = embedded_graphics::primitives::Rectangle::new(
                Point::zero(),
                Size::new(BUTTON_FB_WIDTH, BUTTON_HEIGHT),
            );

            if let (Some(shake_fb), true) = (&mut self.shake_fb, offset_x != 0) {
                let w = BUTTON_FB_WIDTH as usize;
                let bpp = 2usize; // Rgb565 bytes per pixel
                let row_bytes = w * bpp;

                let bg_raw: embedded_graphics::pixelcolor::raw::RawU16 =
                    PALETTE.background.into();
                let bg_val = bg_raw.into_inner();
                let bg_lo = (bg_val & 0xFF) as u8;
                let bg_hi = ((bg_val >> 8) & 0xFF) as u8;

                for y in 0..BUTTON_HEIGHT as usize {
                    let row = y * row_bytes;

                    if offset_x > 0 {
                        let shift = (offset_x as usize).min(w);
                        let copy = w - shift;
                        for i in (row..row + shift * bpp).step_by(2) {
                            shake_fb.data[i] = bg_lo;
                            shake_fb.data[i + 1] = bg_hi;
                        }
                        if copy > 0 {
                            shake_fb.data[row + shift * bpp..row + row_bytes]
                                .copy_from_slice(&self.fb.data[row..row + copy * bpp]);
                        }
                    } else {
                        let shift = ((-offset_x) as usize).min(w);
                        let copy = w - shift;
                        if copy > 0 {
                            shake_fb.data[row..row + copy * bpp]
                                .copy_from_slice(&self.fb.data[row + shift * bpp..row + row_bytes]);
                        }
                        for i in (row + copy * bpp..row + row_bytes).step_by(2) {
                            shake_fb.data[i] = bg_lo;
                            shake_fb.data[i + 1] = bg_hi;
                        }
                    }
                }

                target.fill_contiguous(&area, shake_fb.contiguous_pixels())?;
            } else {
                target.fill_contiguous(&area, self.fb.contiguous_pixels())?;
            }

            self.needs_redraw = false;
        }
        Ok(())
    }
}

/// Height of the progress bar area
const PROGRESS_AREA_HEIGHT: u32 = 4;

/// Shake animation amplitude in pixels
const SHAKE_AMPLITUDE: i32 = 6;
/// Total width of the button framebuffer, including shake padding on both sides
const BUTTON_FB_WIDTH: u32 = BUTTON_WIDTH + 2 * SHAKE_AMPLITUDE as u32;
/// Duration of the shake animation in ms
const SHAKE_DURATION_MS: u64 = 500;
/// Number of oscillation half-cycles during the shake
const SHAKE_HALF_CYCLES: u64 = 5;

/// Compute a damped shake offset for the given elapsed time.
/// Returns a horizontal pixel offset that oscillates and decays to zero.
fn shake_offset(elapsed_ms: u64) -> i32 {
    if elapsed_ms >= SHAKE_DURATION_MS {
        return 0;
    }

    // Progress 0..1 as fixed point (0..1024)
    let progress = (elapsed_ms * 1024 / SHAKE_DURATION_MS) as i32;

    // Damped amplitude: starts at SHAKE_AMPLITUDE, linearly decays to 0
    let amplitude = SHAKE_AMPLITUDE * (1024 - progress) / 1024;

    // Triangle wave oscillation: maps progress through SHAKE_HALF_CYCLES half-cycles
    // Phase goes from 0 to SHAKE_HALF_CYCLES * 1024
    let phase = (elapsed_ms * SHAKE_HALF_CYCLES * 1024 / SHAKE_DURATION_MS) as i32;
    // Triangle wave: rises 0→1024 in first half, falls 1024→0 in second half of each cycle
    let cycle_pos = phase % 2048;
    let triangle = if cycle_pos < 1024 {
        cycle_pos // rising: 0 to 1024
    } else {
        2048 - cycle_pos // falling: 1024 to 0
    };
    // Map triangle from 0..1024 to -1024..1024
    let wave = triangle * 2 - 1024;

    amplitude * wave / 1024
}

/// The 3 quiz buttons with internal touch handling (no Key enum needed).
struct QuizButtons {
    inner: Center<Column<(OptionButton, OptionButton, OptionButton)>>,
    pressed: Option<u8>,
    constraints: Option<Size>,
}

impl QuizButtons {
    fn new(labels: &[String; NUM_OPTIONS]) -> Self {
        let btn0 = OptionButton::new(&labels[0]);
        let btn1 = OptionButton::new(&labels[1]);
        let btn2 = OptionButton::new(&labels[2]);

        let buttons = Column::builder()
            .push(btn0)
            .gap(8)
            .push(btn1)
            .gap(8)
            .push(btn2)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self {
            inner: Center::new(buttons),
            pressed: None,
            constraints: None,
        }
    }

    fn take_pressed(&mut self) -> Option<u8> {
        self.pressed.take()
    }

    fn button_mut(&mut self, index: u8) -> Option<&mut OptionButton> {
        let buttons = &mut self.inner.child;
        match index {
            0 => Some(&mut buttons.children.0),
            1 => Some(&mut buttons.children.1),
            2 => Some(&mut buttons.children.2),
            _ => None,
        }
    }

    fn set_button_style(&mut self, index: u8, fill_color: Rgb565, text_color: Rgb565) {
        if let Some(btn) = self.button_mut(index) {
            btn.set_style(fill_color, text_color);
        }
    }
}

impl DynWidget for QuizButtons {
    fn set_constraints(&mut self, max_size: Size) {
        self.constraints = Some(max_size);
        self.inner.set_constraints(max_size);
    }

    fn sizing(&self) -> Sizing {
        self.inner.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<KeyTouch> {
        if lift_up {
            return None;
        }

        // Translate point from our coordinate space into the Center's child (Column) space
        let center_rect = self.inner.child_rect;
        let column_point = Point::new(
            point.x - center_rect.top_left.x,
            point.y - center_rect.top_left.y,
        );

        // Check which button was hit using the Column's layout rects
        let child_rects = self.inner.child.child_rects.as_ref();
        for (i, rect) in child_rects.iter().enumerate() {
            if rect.contains(column_point) {
                self.pressed = Some(i as u8);
                return None;
            }
        }

        None
    }

    fn force_full_redraw(&mut self) {
        self.inner.force_full_redraw();
    }
}

impl Widget for QuizButtons {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.inner.draw(target, current_time)
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

/// The transitioning content: either quiz buttons or success screen.
type TransitionContent = crate::any_of::AnyOf<(QuizButtons, SuccessContent)>;

/// A multiple-choice quiz screen for verifying backup correctness.
///
/// Presents 26 screens (1 share index + 25 words), each with 3 options.
/// The user taps the correct answer to advance.
///
/// Layout:
///   title (fixed, redrawn on screen change)
///   progress bar (fixed, never transitions)
///   content (SlideInTransition of buttons or success)
pub struct CheckBackupScreen {
    word_indices: [u16; 25],
    share_index: u16,
    current_screen: usize,
    rng_state: u32,

    correct_index: u8,

    title: Padding<Center<Text<DefaultTextStyle>>>,
    title_dirty: bool,
    progress: ProgressBars,
    content: SlideInTransition<TransitionContent>,

    // Feedback
    feedback: Option<(FeedbackKind, u8)>,
    feedback_since: Option<crate::Instant>,

    // Completion
    completed_since: Option<crate::Instant>,
    current_time: Option<crate::Instant>,
    show_progress: bool,
    clear_header: bool,
    checkmark_started: bool,

    size: Size,
    title_height: u32,
    progress_height: u32,
}

/// Padded top offset for the title
const TITLE_TOP_PAD: u32 = 8;
/// Gap between title and progress bar
const TITLE_GAP: u32 = 6;

impl CheckBackupScreen {
    pub fn new(word_indices: [u16; 25], share_index: u16, rand_seed: u32) -> Self {
        let dummy_content: TransitionContent =
            crate::any_of::AnyOf::new(QuizButtons::new(&[
                String::new(),
                String::new(),
                String::new(),
            ]));

        let mut progress = ProgressBars::new(TOTAL_SCREENS);
        progress.progress(0);

        let title_text = Text::new(
            String::new(),
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );
        let title = Padding::only(Center::new(title_text)).top(TITLE_TOP_PAD).build();

        let mut screen = Self {
            word_indices,
            share_index,
            current_screen: 0,
            rng_state: rand_seed,
            correct_index: 0,
            title,
            title_dirty: true,
            progress,
            content: SlideInTransition::new(
                dummy_content,
                750,
                Point::zero(),
                PALETTE.background,
            ),
            feedback: None,
            feedback_since: None,
            completed_since: None,
            current_time: None,
            show_progress: true,
            clear_header: false,
            checkmark_started: false,
            size: Size::zero(),
            title_height: 0,
            progress_height: PROGRESS_AREA_HEIGHT,
        };

        screen.regenerate_content();
        screen
    }

    /// Check if the backup has been fully verified (with delay for success screen)
    pub fn is_verified(&self) -> bool {
        if let (Some(completed_since), Some(now)) = (self.completed_since, self.current_time) {
            let elapsed = now.saturating_duration_since(completed_since);
            return elapsed >= 1000;
        }
        false
    }

    fn regenerate_content(&mut self) {
        if self.current_screen >= TOTAL_SCREENS {
            return;
        }

        self.progress.progress(self.current_screen);

        let (title_str, labels) = if self.current_screen == 0 {
            let labels = self.generate_share_index_options();
            (String::from("Select Key Number"), labels)
        } else {
            let labels = self.generate_word_options(self.current_screen - 1);
            (alloc::format!("Select Word {}", self.current_screen), labels)
        };

        let title_text = Text::new(
            title_str,
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );
        self.title = Padding::only(Center::new(title_text)).top(TITLE_TOP_PAD).build();
        self.title_dirty = true;
        // Re-layout the title with tight height constraint
        if self.size != Size::zero() {
            self.title_height = TITLE_TOP_PAD + FONT_MED.line_height;
            self.title
                .set_constraints(Size::new(self.size.width, self.title_height));
        }

        let quiz = QuizButtons::new(&labels);
        self.content.switch_to(crate::any_of::AnyOf::new(quiz));
    }

    fn generate_share_index_options(&mut self) -> [String; NUM_OPTIONS] {
        let correct = self.share_index;
        let mut options = [0u16; NUM_OPTIONS];
        options[0] = correct;

        // Generate 2 nearby distinct alternatives
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

        self.shuffle_3(&mut options);
        self.correct_index = options.iter().position(|&v| v == correct).unwrap() as u8;

        [
            alloc::format!("#{}", options[0]),
            alloc::format!("#{}", options[1]),
            alloc::format!("#{}", options[2]),
        ]
    }

    fn generate_word_options(&mut self, word_idx: usize) -> [String; NUM_OPTIONS] {
        let correct_word_index = self.word_indices[word_idx];
        let distractors = find_closest_distractors(correct_word_index);

        let mut indices = [correct_word_index, distractors[0], distractors[1]];
        self.shuffle_3(&mut indices);
        self.correct_index = indices
            .iter()
            .position(|&v| v == correct_word_index)
            .unwrap() as u8;

        [
            String::from(BIP39_WORDS[indices[0] as usize]),
            String::from(BIP39_WORDS[indices[1] as usize]),
            String::from(BIP39_WORDS[indices[2] as usize]),
        ]
    }

    fn shuffle_3<T: Copy>(&mut self, arr: &mut [T; NUM_OPTIONS]) {
        // Fisher-Yates shuffle for 3 elements
        for i in (1..NUM_OPTIONS).rev() {
            let j = (xorshift32(&mut self.rng_state) % (i as u32 + 1)) as usize;
            arr.swap(i, j);
        }
    }

    fn advance_screen(&mut self) {
        self.current_screen += 1;
        self.feedback = None;
        self.feedback_since = None;

        if self.current_screen >= TOTAL_SCREENS {
            self.completed_since = self.current_time;
            self.show_progress = false;
            self.clear_header = true;
            // Give transition full screen before adding success content
            self.content.set_constraints(self.size);
            self.content
                .switch_to(crate::any_of::AnyOf::new(SuccessContent::new()));
        } else {
            self.regenerate_content();
        }
    }

    fn handle_option_selected(&mut self, option_index: u8, current_time: crate::Instant) {
        let is_correct = option_index == self.correct_index;

        let (fill_color, text_color, kind) = if is_correct {
            (
                PALETTE.tertiary_container,
                PALETTE.on_background,
                FeedbackKind::Correct,
            )
        } else {
            (
                WRONG_ANSWER_COLOR,
                PALETTE.on_background,
                FeedbackKind::Wrong,
            )
        };

        if let Some(quiz) = self
            .content
            .current_widget_mut()
            .downcast_mut::<QuizButtons>()
        {
            quiz.set_button_style(option_index, fill_color, text_color);
            if !is_correct {
                if let Some(btn) = quiz.button_mut(option_index) {
                    btn.start_shake(current_time);
                }
            }
        }

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
                // Reset button back to normal
                if let Some(quiz) = self
                    .content
                    .current_widget_mut()
                    .downcast_mut::<QuizButtons>()
                {
                    quiz.set_button_style(button_index, PALETTE.surface, PALETTE.primary);
                }
                self.feedback = None;
                self.feedback_since = None;
            }
            _ => {}
        }
    }

    /// Total height consumed by title + progress (the fixed header area)
    fn header_height(&self) -> u32 {
        self.title_height + TITLE_GAP + self.progress_height
    }
}

impl DynWidget for CheckBackupScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.size = max_size;

        // Layout title — constrain height tightly so Center only centers horizontally
        self.title_height = TITLE_TOP_PAD + FONT_MED.line_height;
        self.title
            .set_constraints(Size::new(max_size.width, self.title_height));

        // Layout progress bar
        self.progress
            .set_constraints(Size::new(max_size.width, PROGRESS_AREA_HEIGHT));

        if self.show_progress {
            // Give remaining height to the transitioning content
            let content_height = max_size.height.saturating_sub(self.header_height());
            self.content
                .set_constraints(Size::new(max_size.width, content_height));
        } else {
            // Success screen uses full area
            self.content.set_constraints(max_size);
        }
    }

    fn sizing(&self) -> Sizing {
        self.size.into()
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

        // Only handle press, not release
        if lift_up {
            return None;
        }

        // Ignore presses during feedback delay
        if self.feedback.is_some() {
            return None;
        }

        self.current_time = Some(current_time);

        // Adjust point into content area
        let content_y = self.header_height() as i32;
        let content_point = Point::new(point.x, point.y - content_y);

        // Delegate to the quiz buttons
        self.content
            .current_widget_mut()
            .handle_touch(content_point, current_time, lift_up);

        // Check if a button was pressed
        if let Some(quiz) = self
            .content
            .current_widget_mut()
            .downcast_mut::<QuizButtons>()
        {
            if let Some(index) = quiz.take_pressed() {
                self.handle_option_selected(index, current_time);
            }
        }

        None
    }

    fn force_full_redraw(&mut self) {
        self.title.force_full_redraw();
        self.progress.force_full_redraw();
        self.content.force_full_redraw();
    }
}

impl Widget for CheckBackupScreen {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.current_time = Some(current_time);
        self.check_feedback_timeout();

        if self.show_progress {
            // Clear title area when text changed to avoid overwriting artifacts
            let title_area = embedded_graphics::primitives::Rectangle::new(
                Point::zero(),
                Size::new(self.size.width, self.title_height),
            );
            if self.title_dirty {
                target.fill_solid(&title_area, PALETTE.background)?;
                self.title_dirty = false;
            }

            // Draw title at top
            let mut title_target = target.clone().crop(title_area);
            self.title.draw(&mut title_target, current_time)?;

            // Draw progress bar below title
            let progress_y = self.title_height + TITLE_GAP;
            let mut progress_target =
                target.clone().translate(Point::new(0, progress_y as i32));
            self.progress.draw(&mut progress_target, current_time)?;

            // Draw transitioning content below header
            let content_y = self.header_height() as i32;
            let content_height = self.size.height.saturating_sub(self.header_height());
            let mut content_target = target.clone().crop(
                embedded_graphics::primitives::Rectangle::new(
                    Point::new(0, content_y),
                    Size::new(self.size.width, content_height),
                ),
            );
            self.content.draw(&mut content_target, current_time)?;
        } else {
            // Success screen: clear entire screen once, then use full screen.
            // Must clear everything because progress bars (8px tall) extend beyond
            // the header_height calculation, leaving green/grey line artifacts.
            if self.clear_header {
                target.fill_solid(
                    &embedded_graphics::primitives::Rectangle::new(
                        Point::zero(),
                        self.size,
                    ),
                    PALETTE.background,
                )?;
                self.clear_header = false;
            }

            self.content.draw(target, current_time)?;

            // Start checkmark only after slide-in transition completes,
            // otherwise the Fader redraws conflict with the checkmark animation
            if !self.checkmark_started && self.content.is_transition_complete() {
                if let Some(success) = self
                    .content
                    .current_widget_mut()
                    .downcast_mut::<SuccessContent>()
                {
                    let button = success.button_mut();
                    button.force_full_redraw();
                    button.checkmark_mut().start_drawing();
                    self.checkmark_started = true;
                }
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
