use super::{
    centred_in, clear_rect, draw_text_in, word_spelling, Dots, Taps, FLASH_MS, FONT_SMALL_LINE,
    FONT_TITLE, INPUT_GRACE_MS, SCREEN_WIDTH, SUB_FADE_IN_MS, SUB_FADE_OUT_MS, UNDO_CAP_PER_BOX,
};
use crate::{
    gray4_style::Gray4TextStyle, palette::PALETTE, prelude::*, FadeConfig, FadeSwitcher, Key,
    KeyTouch, FONT_HUGE_MONO,
};
use alloc::{format, string::String, vec::Vec};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{
        PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle, StrokeAlignment,
    },
};

/// Options per word: the true word and 2 of its 6 closest lookalikes, picked at random and
/// shuffled by core (design O11).
pub const OPTIONS: usize = 3;
/// Words in the device round, one dot each.
pub const DEVICE_ROUND_BOXES: u8 = 5;
const BUTTON_SIZE: Size = Size::new(200, 44);
const BUTTON_RADIUS: u32 = 8;
const TITLE_Y: i32 = 14;
const DOTS_Y: i32 = 39;
const DOT_DIAMETER: u32 = 8;
const DOT_GAP: u32 = 6;
const UNDO_Y: i32 = 250;
const EMPHASISED_LETTERS: usize = 4;
pub(crate) const NONE_OF_THESE: &str = "None of these";
/// Under the flash after a pick.
pub(crate) const UNDO_TEXT: &str = "Tap to undo";

/// The three word buttons, then "None of these": 200 x 44 px from y = 51 to 245 (spec
/// §0.6.2.3).
pub const OPTION_RECTS: [Rectangle; OPTIONS + 1] = [
    Rectangle::new(Point::new(20, 51), BUTTON_SIZE),
    Rectangle::new(Point::new(20, 101), BUTTON_SIZE),
    Rectangle::new(Point::new(20, 151), BUTTON_SIZE),
    Rectangle::new(Point::new(20, 201), BUTTON_SIZE),
];

/// What the answer screen reports, once per pick, after the flash.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeviceRoundEvent {
    /// The option tapped (`None` for "None of these"), on the display it was shown on. Only
    /// core knows which option is the card's word. Owner B12: the device round runs after N,
    /// so there's no Stop on it.
    Pick { display_id: u32, pick: Option<u8> },
}

impl core::fmt::Debug for DeviceRoundEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeviceRoundEvent::Pick { display_id, .. } => f
                .debug_struct("Pick")
                .field("display_id", display_id)
                .finish_non_exhaustive(),
        }
    }
}

pub(crate) fn answer_title(j: u8) -> String {
    format!("Select Word {}", j as u32 + 1)
}

type Word = Text<Gray4TextStyle, &'static str>;

fn blank_word() -> Word {
    Text::new_with("", Gray4TextStyle::new(FONT_HUGE_MONO, PALETTE.primary))
}

struct AnswerContent {
    title: Text<Gray4TextStyle>,
    dots: Dots,
    /// Each word's first four letters and the rest, as slices of the word list. A slice's
    /// address and width name the word as well as its index does, so they are blanked on
    /// drop.
    words: Vec<(Word, Word)>,
    none_of_these: Text<Gray4TextStyle>,
    undo: Text<Gray4TextStyle>,
    undo_rect: Rectangle,
    /// The button with the flash border.
    flashed: Option<u8>,
    undo_shown: bool,
    undo_drawn: bool,
    dirty: bool,
    redraw: [bool; OPTIONS + 1],
    taps: Taps,
}

impl AnswerContent {
    fn new(j: u8, k: u8, options: &[u16; OPTIONS]) -> Self {
        let mut dots = Dots::new(DEVICE_ROUND_BOXES, DOT_DIAMETER, DOT_GAP);
        dots.set_filled(k + 1);
        let words = options
            .iter()
            .map(|&word| {
                let spelling = word_spelling(word);
                let split = spelling.len().min(EMPHASISED_LETTERS);
                (
                    Text::new_with(
                        &spelling[..split],
                        Gray4TextStyle::new(FONT_HUGE_MONO, PALETTE.primary),
                    ),
                    Text::new_with(
                        &spelling[split..],
                        Gray4TextStyle::new(FONT_HUGE_MONO, PALETTE.text_secondary),
                    ),
                )
            })
            .collect();
        let undo = Text::new(
            String::from(UNDO_TEXT),
            Gray4TextStyle::new(FONT_SMALL_LINE, PALETTE.text_secondary),
        );
        let undo_size: Size = undo.sizing().into();
        Self {
            title: Text::new(
                answer_title(j),
                Gray4TextStyle::new(FONT_TITLE, PALETTE.on_background),
            ),
            dots,
            words,
            none_of_these: Text::new(
                String::from(NONE_OF_THESE),
                Gray4TextStyle::new(FONT_TITLE, PALETTE.on_background),
            ),
            undo,
            undo_rect: Rectangle::new(
                Point::new((SCREEN_WIDTH as i32 - undo_size.width as i32) / 2, UNDO_Y),
                undo_size,
            ),
            flashed: None,
            undo_shown: false,
            undo_drawn: false,
            dirty: true,
            redraw: [false; OPTIONS + 1],
            taps: Taps::default(),
        }
    }

    fn draw_button<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        i: usize,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        let rect = OPTION_RECTS[i];
        let fill = PALETTE.surface;
        RoundedRectangle::with_equal_corners(rect, Size::new(BUTTON_RADIUS, BUTTON_RADIUS))
            .into_styled(PrimitiveStyle::with_fill(fill))
            .draw(target)?;
        match self.words.get_mut(i) {
            Some((first, rest)) => {
                let first_size: Size = first.sizing().into();
                let rest_width = rest.sizing().width;
                let width = first_size.width + rest_width;
                let left = rect.top_left.x + (rect.size.width as i32 - width as i32) / 2;
                let top =
                    rect.top_left.y + (rect.size.height as i32 - first_size.height as i32) / 2;
                let first_rect = Rectangle::new(Point::new(left, top), first_size);
                draw_text_in(target, first, first_rect, fill, current_time)?;
                if rest_width > 0 {
                    let rest_rect = Rectangle::new(
                        Point::new(left + first_size.width as i32, top),
                        rest.sizing().into(),
                    );
                    draw_text_in(target, rest, rest_rect, fill, current_time)?;
                }
            }
            None => {
                let label_rect = centred_in(&self.none_of_these, rect);
                draw_text_in(
                    target,
                    &mut self.none_of_these,
                    label_rect,
                    fill,
                    current_time,
                )?;
            }
        }
        Ok(())
    }
}

impl Drop for AnswerContent {
    fn drop(&mut self) {
        for (first, rest) in self.words.iter_mut() {
            *first = blank_word();
            *rest = blank_word();
        }
        core::hint::black_box(&mut self.words);
    }
}

impl DynWidget for AnswerContent {
    fn set_constraints(&mut self, _max_size: Size) {}

    fn sizing(&self) -> crate::Sizing {
        Size::new(SCREEN_WIDTH, 280).into()
    }

    fn force_full_redraw(&mut self) {
        self.dirty = true;
        self.dots.force_full_redraw();
        self.undo_drawn = false;
    }
}

impl Widget for AnswerContent {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut redraw = core::mem::take(&mut self.redraw);
        self.taps.draw(target, current_time, |key| match key {
            Key::DeviceRoundOption(i) => redraw[i as usize] = true,
            _ => redraw[OPTIONS] = true,
        });
        let dirty = core::mem::take(&mut self.dirty);
        if dirty {
            let title_rect = Rectangle::new(
                Point::new(
                    (SCREEN_WIDTH as i32 - self.title.sizing().width as i32) / 2,
                    TITLE_Y,
                ),
                self.title.sizing().into(),
            );
            draw_text_in(
                target,
                &mut self.title,
                title_rect,
                PALETTE.background,
                current_time,
            )?;
        }
        let dots_left = (SCREEN_WIDTH as i32 - self.dots.size().width as i32) / 2;
        self.dots.draw(target, Point::new(dots_left, DOTS_Y))?;
        for (i, redraw) in redraw.into_iter().enumerate() {
            if dirty || redraw {
                self.draw_button(target, i, current_time)?;
            }
        }
        // the one flash style, the same for every option: never right or wrong
        if let Some(i) = self.flashed {
            RoundedRectangle::with_equal_corners(
                OPTION_RECTS[i as usize],
                Size::new(BUTTON_RADIUS, BUTTON_RADIUS),
            )
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .stroke_color(PALETTE.primary)
                    .stroke_width(2)
                    .stroke_alignment(StrokeAlignment::Inside)
                    .build(),
            )
            .draw(target)?;
        }
        if self.undo_shown && !self.undo_drawn {
            self.undo_drawn = true;
            draw_text_in(
                target,
                &mut self.undo,
                self.undo_rect,
                PALETTE.background,
                current_time,
            )?;
        } else if !self.undo_shown && self.undo_drawn {
            self.undo_drawn = false;
            clear_rect(target, self.undo_rect)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum State {
    Answering,
    Flash { button: u8, started: Instant },
    Waiting,
}

/// The device round's answer screen (D6): "Select Word 45", five dots with no number, three
/// word buttons and "None of these". A tap starts the same 1.2 s flash on whichever button
/// was tapped, with "Tap to undo"; only then does it report the pick.
///
/// 🔐 It shows words of 33-64, the 5 the device drew for its round, but is never told which
/// one is right, so it can't colour a pick. A screen's word buttons are blanked when it is
/// dropped, here or after the fade to the next word.
pub struct DeviceRoundAnswer {
    switcher: FadeSwitcher<AnswerContent>,
    j: u8,
    k: u8,
    display_id: u32,
    state: State,
    undos_left: u8,
    undo_pressed: bool,
    shown_at: Option<Instant>,
    event: Option<DeviceRoundEvent>,
}

impl DeviceRoundAnswer {
    /// Word `j + 1` (j in 32..64), the `k`th of the round (0..5).
    pub fn new(j: u8, k: u8, options: [u16; OPTIONS], display_id: u32) -> Self {
        let switcher = FadeSwitcher::new(AnswerContent::new(j, k, &options))
            .with_fade_in(FadeConfig::new(SUB_FADE_IN_MS))
            .with_fade_out(FadeConfig::new(SUB_FADE_OUT_MS));
        Self {
            switcher,
            j,
            k,
            display_id,
            state: State::Answering,
            undos_left: UNDO_CAP_PER_BOX,
            undo_pressed: false,
            shown_at: None,
            event: None,
        }
    }

    /// Start with `undos_left` (at most [`UNDO_CAP_PER_BOX`]) instead of a full set: the
    /// same word's second look, after the mismatch page, keeps what was left when the firmware
    /// builds a new answer screen for it.
    pub fn with_undos_left(mut self, undos_left: u8) -> Self {
        self.undos_left = undos_left.min(UNDO_CAP_PER_BOX);
        self
    }

    /// The next word, or the same word's second look (same options, reshuffled), in place. The
    /// second look keeps what is left of the word's undos.
    pub fn set_question(&mut self, j: u8, k: u8, options: [u16; OPTIONS], display_id: u32) {
        if j != self.j {
            self.undos_left = UNDO_CAP_PER_BOX;
        }
        self.switcher.switch_to(AnswerContent::new(j, k, &options));
        self.j = j;
        self.k = k;
        self.display_id = display_id;
        self.state = State::Answering;
        self.undo_pressed = false;
        self.shown_at = None;
        self.event = None;
    }

    pub fn take_event(&mut self) -> Option<DeviceRoundEvent> {
        self.event.take()
    }

    pub fn is_animating(&self) -> bool {
        matches!(self.state, State::Flash { .. }) || self.switcher.is_transitioning()
    }

    pub fn is_flashing(&self) -> bool {
        matches!(self.state, State::Flash { .. })
    }

    pub fn undos_left(&self) -> u8 {
        self.undos_left
    }

    pub fn box_index(&self) -> u8 {
        self.j
    }

    fn accepting(&self, current_time: Instant) -> bool {
        self.shown_at.is_some_and(|shown_at| {
            current_time.saturating_duration_since(shown_at) >= INPUT_GRACE_MS
        })
    }

    fn content(&mut self) -> &mut AnswerContent {
        self.switcher.current_mut()
    }

    /// The title, the widest word, "None of these" and where "Tap to undo" ends, for the fit
    /// test.
    #[cfg(test)]
    pub(crate) fn fit(&self) -> (u32, u32, u32, u32) {
        let content = self.switcher.current();
        let widest_word = content
            .words
            .iter()
            .map(|(first, rest)| first.sizing().width + rest.sizing().width)
            .max()
            .unwrap_or(0);
        (
            content.title.sizing().width,
            widest_word,
            content.none_of_these.sizing().width,
            (content.undo_rect.top_left.y + content.undo_rect.size.height as i32) as u32,
        )
    }
}

impl core::fmt::Debug for DeviceRoundAnswer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceRoundAnswer")
            .field("k", &self.k)
            .field("display_id", &self.display_id)
            .finish_non_exhaustive()
    }
}

impl DynWidget for DeviceRoundAnswer {
    fn set_constraints(&mut self, max_size: Size) {
        self.switcher.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.switcher.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        match self.state {
            State::Answering => {
                if !is_release {
                    if !self.accepting(current_time) {
                        return None;
                    }
                    if let Some(i) = OPTION_RECTS.iter().position(|r| r.contains(point)) {
                        let key = if i < OPTIONS {
                            Key::DeviceRoundOption(i as u8)
                        } else {
                            Key::NoneOfThese
                        };
                        self.content().taps.press(key, OPTION_RECTS[i]);
                    }
                    return None;
                }
                let button = match self.content().taps.release(current_time) {
                    Some(Key::DeviceRoundOption(i)) => i,
                    Some(Key::NoneOfThese) => OPTIONS as u8,
                    _ => return None,
                };
                self.state = State::Flash {
                    button,
                    started: current_time,
                };
                let undo_allowed = self.undos_left > 0;
                let content = self.content();
                content.flashed = Some(button);
                content.undo_shown = undo_allowed;
            }
            State::Flash { button, started } => {
                if !is_release {
                    // the whole screen is the undo target, after the grace
                    self.undo_pressed = self.undos_left > 0
                        && current_time.saturating_duration_since(started) >= INPUT_GRACE_MS;
                } else if core::mem::take(&mut self.undo_pressed) {
                    self.undos_left -= 1;
                    self.state = State::Answering;
                    self.shown_at = Some(current_time);
                    let content = self.content();
                    content.flashed = None;
                    content.undo_shown = false;
                    content.redraw[button as usize] = true;
                }
            }
            State::Waiting => {}
        }
        None
    }

    /// A drag picks nothing and undoes nothing. Owner B12: nor does a swipe down ask to stop.
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        self.content().taps.cancel();
        self.undo_pressed = false;
    }

    fn force_full_redraw(&mut self) {
        self.switcher.force_full_redraw();
    }
}

impl Widget for DeviceRoundAnswer {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // The input grace starts once the options are fully in, not while the last screen
        // fades out.
        if self.shown_at.is_none() && !self.switcher.is_transitioning() {
            self.shown_at = Some(current_time);
        }
        if let State::Flash { button, started } = self.state {
            if current_time.saturating_duration_since(started) >= FLASH_MS {
                self.state = State::Waiting;
                self.undo_pressed = false;
                self.content().undo_shown = false;
                let pick = (button < OPTIONS as u8).then_some(button);
                self.event = Some(DeviceRoundEvent::Pick {
                    display_id: self.display_id,
                    pick,
                });
            }
        }
        self.switcher.draw(target, current_time)
    }
}
