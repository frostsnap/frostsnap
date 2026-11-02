use super::{
    button_rect, CardButton, Para, SwipeFooter, TextBlock, FONT_BODY, FONT_SMALL_LINE, FONT_TITLE,
    SCREEN_WIDTH, TEXT_TOP, TEXT_TOP_HOLD,
};
use crate::{palette::PALETTE, prelude::*, HoldToConfirm, Key, KeyTouch, HOLD_TO_CONFIRM_TIME_MS};
use alloc::{boxed::Box, format, string::String, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use frost_backup::verifiable_card::layout::WordMask;

/// How a card page is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Footer {
    /// Swipe up.
    Swipe,
    /// Not by the user: the next screen comes from elsewhere.
    None,
    /// Hold the circle. The label, if any, names what the hold confirms and is the last
    /// line above the circle.
    Hold(Option<&'static str>),
    /// Tap the full-width button with this label.
    Button(&'static str),
}

/// The title colour of a card page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Normal,
    Error,
    Warning,
    Confirm,
}

impl Tone {
    fn color(self) -> Rgb565 {
        match self {
            Tone::Normal => PALETTE.on_background,
            Tone::Error => PALETTE.error,
            Tone::Warning => PALETTE.warning,
            Tone::Confirm => PALETTE.confirm_progress,
        }
    }
}

/// The fixed text of a card page.
#[derive(Debug, Clone, Copy)]
pub struct CardCopy {
    pub title: &'static str,
    pub body: &'static str,
    pub small: &'static str,
    pub tone: Tone,
    /// The small line is a rule, so it takes the title colour instead of the dim one.
    pub small_in_title_colour: bool,
    /// The small line is the stop line of a rule: body size, in the title colour (spec
    /// §0.6.5.3, rule R).
    pub small_is_stop_line: bool,
    pub footer: Footer,
}

impl CardCopy {
    const fn new(
        title: &'static str,
        body: &'static str,
        small: &'static str,
        footer: Footer,
    ) -> Self {
        Self {
            title,
            body,
            small,
            tone: Tone::Normal,
            small_in_title_colour: false,
            small_is_stop_line: false,
            footer,
        }
    }

    const fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn page(&self) -> CardInfoPage {
        let mut page = CardInfoPage::new(self.title, self.body, self.small, self.footer);
        page.tone = self.tone;
        page.small_in_title_colour = self.small_in_title_colour;
        page.small_is_stop_line = self.small_is_stop_line;
        page.build();
        page
    }
}

// The device copy of the card flows (design §4.1). Each is checked against the layout rules by
// the fit test in `card/mod.rs`. Plain words, ASCII only ("1-32", not an en dash): the device
// fonts have no other glyphs.

/// "Look at your phone", until the phone has pinned the seal and sent `WriteStart`.
pub const COPY_WAITING_FOR_PHONE: CardCopy = CardCopy::new(
    "Look at your phone",
    "It checks this card before you start.",
    "To leave, unplug this device.",
    Footer::None,
);
/// D4: the device half is written and checked. The user confirms it in the app, which then
/// shows words 33-64 (owner B1). Nothing runs on the device here.
pub const COPY_PHONE_HALF_NEXT: CardCopy = CardCopy::new(
    "Now use your phone",
    "Confirm words 1-32 in the app. It then shows words 33-64.",
    "",
    Footer::None,
);
/// After the phone's `FinalBegin` is due: the phone starts the challenge.
pub const COPY_CARD_FINISHED_WAITING: CardCopy = CardCopy::new(
    "Look at your phone",
    "It starts the last check.",
    "",
    Footer::None,
);
/// D5. Owner B12: the device round runs after N, so it has no Stop.
pub const COPY_DEVICE_ROUND_INTRO: CardCopy = CardCopy::new(
    "Last check on this device",
    "5 words from your phone's half. Pick the word you wrote. If it isn't there, pick None of these.",
    "",
    Footer::Swipe,
);
/// D7, while the phone asks. Owner B12: there's no Stop after N, so the way out is to unplug.
pub const COPY_GATE: CardCopy = CardCopy::new(
    "Answer on your phone",
    "It asks about 5 of words 1-32. One dot per word.",
    "If it asks for more words than the dots, unplug this device.",
    Footer::None,
);
/// D7, while the next opening is computed
pub const COPY_GATE_PREPARING: CardCopy = CardCopy::new(
    "Preparing your phone's check",
    "About 25 seconds.",
    "",
    Footer::None,
);
/// After "Backup checked"
pub const COPY_CARD_DONE: CardCopy = CardCopy::new(
    "Card done",
    "Put your card away somewhere safe.",
    "",
    Footer::Swipe,
)
.tone(Tone::Confirm);
/// D8: the device round failed
pub const COPY_DEVICE_ROUND_FAILED: CardCopy = CardCopy::new(
    "Stop. Don't use this wallet",
    "Your card doesn't match this device's copy.",
    "If either screen says stop, stop. No screen can undo a stop.",
    Footer::Swipe,
)
.tone(Tone::Error);
/// D8, the next step after the device round failed
pub const COPY_DEVICE_ROUND_NEXT_STEP: CardCopy = CardCopy::new(
    "What to do",
    "Set this wallet up again with a different phone.",
    "Keep the card. Never type, show or say its words.",
    Footer::Swipe,
);
/// D8: the phone round failed
pub const COPY_PHONE_ROUND_FAILED: CardCopy = CardCopy::new(
    "Stop. Don't use this wallet",
    "Your phone's check of this card failed.",
    "If either screen says stop, stop. No screen can undo a stop.",
    Footer::Swipe,
)
.tone(Tone::Error);
/// D8, the next step after the phone round failed
pub const COPY_NEXT_STEP: CardCopy = CardCopy::new(
    "What to do",
    "Set this wallet up again without this device.",
    "If money is already in it, move it once the new cards pass.",
    Footer::Swipe,
);
/// A stop before N, on either screen
pub const COPY_STOPPED: CardCopy = CardCopy::new(
    "Check stopped",
    "This card is not checked. Don't use this wallet.",
    "Set this wallet up again.",
    Footer::Swipe,
)
.tone(Tone::Error);
/// The next step after a stop before N. Before N nobody knows the phone round's words, so
/// neither the phone nor this device is suspect.
pub const COPY_NEXT_STEP_BEFORE_N: CardCopy = CardCopy::new(
    "What to do",
    "Set this wallet up again.",
    "Keep the card. Never type, show or say its words.",
    Footer::Swipe,
);
/// The card is finished and can't be shown again
pub const COPY_REDISPLAY_REFUSED: CardCopy = CardCopy::new(
    "This card is finished",
    "It can't be shown again. Copy your paper card by hand.",
    "",
    Footer::Swipe,
);
/// `ThresholdOne`
pub const COPY_REFUSED_THRESHOLD_ONE: CardCopy = CardCopy::new(
    "No card on this device",
    "In this wallet any one key can spend.",
    "Only the device that shows Key 1 may make its card.",
    Footer::Swipe,
);
/// `NotKeygenOrigin`: no card origin recorded, so a restored key or one made before cards. As
/// in the app, it doesn't say only the creating device makes the card, which would blame the
/// device that did (review F3).
pub const COPY_REFUSED_NOT_KEYGEN: CardCopy = CardCopy::new(
    "No card on this device",
    "This key was restored, or made before backup cards.",
    "Keep the backup it has.",
    Footer::Swipe,
);
/// `NotCardKey`: the key was made with a 25-word backup
pub const COPY_REFUSED_NOT_CARD_KEY: CardCopy = CardCopy::new(
    "No card on this device",
    "This key was made with a 25-word backup, not a card.",
    "Keep its 25-word backup.",
    Footer::Swipe,
);
/// `NotDirectUsb`
pub const COPY_REFUSED_NOT_DIRECT_USB: CardCopy = CardCopy::new(
    "Plug this device straight into your phone or computer",
    "",
    "",
    Footer::Swipe,
);
/// `RunCap`: a word of 33-64 was opened too often (design §3.2)
pub const COPY_REFUSED_RUN_CAP: CardCopy = CardCopy::new(
    "This card can't be finished",
    "Make a new key.",
    "",
    Footer::Swipe,
);
pub const COPY_DEVICE_ERROR: CardCopy = CardCopy::new(
    "Device error",
    "The card was stopped. Don't use this wallet until it's checked again.",
    "",
    Footer::Swipe,
);
/// R2: words 1-32 are in; the app's half isn't
pub const COPY_WAITING_FOR_PHONE_HALF: CardCopy = CardCopy::new(
    "Now enter words 33-64 in the app",
    "The card is checked once both halves are in.",
    "",
    Footer::None,
);
/// R4: the card isn't what an honest device writes for the wallet record the phone sent (the
/// canonical check failed; spec L1.8.7, PK.8). Its masks passed: a card whose masks fail isn't
/// restored at all (owner B7).
pub const COPY_RECORD_DIFFERS: CardCopy = CardCopy::new(
    "Wallet record not confirmed",
    "This card doesn't match the wallet record your phone sent.",
    "Don't use this key until another device of this wallet confirms it.",
    Footer::Swipe,
)
.tone(Tone::Warning);
/// R4, last page
pub const COPY_HOLD_TO_RESTORE: CardCopy =
    CardCopy::new("Hold to restore this key", "", "", Footer::Hold(None));
/// R4: the card couldn't be decoded, or its masks failed (owner B7): one page for both, so
/// neither screen can tell them apart. The button reopens the review of words 1-32.
pub const COPY_RESTORE_FAILED: CardCopy = CardCopy::new(
    "Couldn't read the card",
    "Check every word on both screens, or try with a different phone.",
    "Words 1-32 here, words 33-64 in the app.",
    Footer::Button("Edit words 1-32"),
)
.tone(Tone::Error);
/// R4: more words were marked unreadable than the card can repair
pub const COPY_TOO_MANY_UNREADABLE: CardCopy = CardCopy::new(
    "Too many unreadable words",
    "At most 3 words can be repaired.",
    "",
    Footer::Button("Edit words 1-32"),
);
pub const COPY_NEEDS_NEWER_FIRMWARE: CardCopy =
    CardCopy::new("This card needs newer firmware", "", "", Footer::Swipe);
pub const COPY_ALREADY_HELD: CardCopy = CardCopy::new(
    "This device already holds this key",
    "Restore it on a blank device.",
    "",
    Footer::Swipe,
);
/// R4: a genuine card for another wallet (owner B11): it decoded and its self-checking masks
/// passed, but it isn't for the wallet the phone sent. The restore ends. A lying writer chip's
/// card (honest self masks, bad wallet-bound masks) gets this too, so the words are the app's
/// own, which cover both (spec PK-O15).
pub const COPY_WRONG_WALLET: CardCopy = CardCopy::new(
    "Doesn't match this wallet",
    "This card doesn't match this wallet. It may be another wallet's card, or it may have been \
     written wrong.",
    "Restore with this wallet's card.",
    Footer::Swipe,
);

/// Every fixed page, for demos and the fit test.
pub const ALL_CARD_COPY: &[CardCopy] = &[
    COPY_WAITING_FOR_PHONE,
    COPY_PHONE_HALF_NEXT,
    COPY_CARD_FINISHED_WAITING,
    COPY_DEVICE_ROUND_INTRO,
    COPY_GATE,
    COPY_GATE_PREPARING,
    COPY_CARD_DONE,
    COPY_DEVICE_ROUND_FAILED,
    COPY_DEVICE_ROUND_NEXT_STEP,
    COPY_PHONE_ROUND_FAILED,
    COPY_NEXT_STEP,
    COPY_STOPPED,
    COPY_NEXT_STEP_BEFORE_N,
    COPY_REDISPLAY_REFUSED,
    COPY_REFUSED_THRESHOLD_ONE,
    COPY_REFUSED_NOT_KEYGEN,
    COPY_REFUSED_NOT_CARD_KEY,
    COPY_REFUSED_NOT_DIRECT_USB,
    COPY_REFUSED_RUN_CAP,
    COPY_DEVICE_ERROR,
    COPY_WAITING_FOR_PHONE_HALF,
    COPY_RECORD_DIFFERS,
    COPY_HOLD_TO_RESTORE,
    COPY_RESTORE_FAILED,
    COPY_TOO_MANY_UNREADABLE,
    COPY_NEEDS_NEWER_FIRMWARE,
    COPY_ALREADY_HELD,
    COPY_WRONG_WALLET,
];

/// D1, the first time: the encode and the seal (spec PK.7 (a), ~15 s)
pub const BUSY_PREPARING_CARD: (&str, &str) = ("Preparing your card", "About 20 seconds.");
/// D1 on a resume or a later check: the encode alone
pub const BUSY_OPENING_CARD: (&str, &str) = ("Preparing your card", "A moment.");
/// PROVING: the bit proofs run while the user writes words 33-64 from the app. The device
/// takes no word input and shows no word until they are done (owner B2); the bar follows the
/// proof job, and the seal's recompute on a resume.
pub const BUSY_PROVING: (&str, &str) = (
    "Use your phone",
    "Carry on in the app while this device checks your card.",
);
/// R3
pub const BUSY_CHECKING_CARD: (&str, &str) = ("Checking your card...", "About 10 seconds.");

/// "7", "7 and 39", "7, 19 and 40": the words set in `mask` (bit 0 is word 1).
fn word_list(mask: WordMask) -> (String, usize) {
    debug_assert!(mask != 0, "no words to list; the caller skips the page");
    let words: Vec<u32> = (0..WordMask::BITS)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| bit + 1)
        .collect();
    let mut list = String::new();
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            list.push_str(if i + 1 == words.len() { " and " } else { ", " });
        }
        list.push_str(&format!("{w}"));
    }
    (list, words.len())
}

/// R4: the key is restored. The threshold is the one recorded on the card, which its proofs
/// don't bind: `Some(0)` when not recorded, `None` when it doesn't read (spec PK.3). The
/// canonical-check line is shown only when the card matched the wallet record.
pub fn key_restored_page(
    index: u16,
    threshold: Option<u8>,
    canonical_matched: bool,
) -> CardInfoPage {
    let body = match threshold {
        Some(0) => format!("Key {index}. The card records no threshold."),
        Some(threshold) => format!("Key {index}. The card says threshold {threshold}."),
        None => format!("Key {index}. The card's threshold doesn't read."),
    };
    let small = if canonical_matched {
        "This card matches the wallet record your phone sent."
    } else {
        ""
    };
    CardInfoPage::new("Key restored", &body, small, Footer::Swipe).with_tone(Tone::Confirm)
}

/// The label of the consolidation hold on a threshold warning.
pub const HOLD_RESTORE_ANYWAY: &str = "Hold to restore anyway";

/// The title and body of a threshold warning: the card's threshold (`None` when it doesn't
/// read) against the wallet's, when it is known.
fn threshold_warning_text(card: Option<u8>, wallet: Option<u16>) -> (&'static str, String) {
    let card_line = match card {
        Some(card) => format!("This card says {card}."),
        None => String::from("This card's threshold doesn't read."),
    };
    let body = match wallet {
        Some(wallet) => format!("{card_line} The wallet says {wallet}."),
        None => card_line,
    };
    let title = match card {
        Some(_) => "Threshold differs",
        None => "Threshold unreadable",
    };
    (title, body)
}

/// R4: the card's threshold isn't the wallet record's, or doesn't read. The share is restored
/// anyway: the threshold isn't bound by the proofs, so it only warns (spec PK.3).
pub fn threshold_warning_page(card: Option<u8>, wallet: Option<u16>) -> CardInfoPage {
    let (title, body) = threshold_warning_text(card, wallet);
    CardInfoPage::new(
        title,
        &body,
        "The device that wrote this card may be faulty.",
        Footer::Swipe,
    )
    .with_tone(Tone::Warning)
}

/// Consolidation of a card share whose threshold isn't the wallet's, or doesn't read: the user
/// holds to go on, and it never refuses (build plan A11).
pub fn threshold_hold_page(card: Option<u8>, wallet: u16) -> CardInfoPage {
    let (title, body) = threshold_warning_text(card, Some(wallet));
    CardInfoPage::new(title, &body, "", Footer::Hold(Some(HOLD_RESTORE_ANYWAY)))
        .with_tone(Tone::Warning)
}

/// R4: words whose entries the decoder repaired, by number only (a word of 33-64 is never
/// shown here). `mask` must not be 0: with no words, the caller skips the page.
pub fn words_didnt_fit_page(mask: WordMask) -> CardInfoPage {
    let (list, count) = word_list(mask);
    let (title, body) = if count == 1 {
        (
            format!("Word {list} didn't fit"),
            "It was repaired. Look at it on your card.",
        )
    } else {
        (
            format!("Words {list} didn't fit"),
            "They were repaired. Look at them on your card.",
        )
    };
    CardInfoPage::new(
        &title,
        body,
        "If it's hard to read, move your funds to a new wallet soon.",
        Footer::Swipe,
    )
}

/// R4: words entered as "Can't read this word". Numbers only, never their words. `mask` must
/// not be 0: with no words, the caller skips the page.
pub fn words_unreadable_page(mask: WordMask) -> CardInfoPage {
    let (list, count) = word_list(mask);
    let (title, body) = if count == 1 {
        (
            format!("Word {list} unreadable"),
            "Your key is restored. Its word isn't shown.",
        )
    } else {
        (
            format!("Words {list} unreadable"),
            "Your key is restored. Their words aren't shown.",
        )
    };
    CardInfoPage::new(
        &title,
        body,
        "Don't write on the card. It is damaged: move your funds to a new wallet soon.",
        Footer::Swipe,
    )
}

/// D6b: a pick that didn't match gets one more look. A swipe up shows the same options again,
/// reshuffled.
pub fn device_round_mismatch_page(j: u8) -> CardInfoPage {
    let title = format!("Look again at word {}", j as u32 + 1);
    let mut page = CardInfoPage::new(
        &title,
        "Check it letter by letter on your card.",
        "Don't change your card.",
        Footer::Swipe,
    );
    page.tone = Tone::Error;
    page.build();
    page
}

enum InfoBody {
    Plain(TextBlock),
    Swipe(TextBlock, Box<SwipeFooter>),
    Hold(Box<HoldToConfirm<TextBlock>>),
    Button {
        text: TextBlock,
        button: Box<CardButton>,
        rect: Rectangle,
        touch: Option<Box<KeyTouch>>,
        pressed: bool,
    },
}

/// A card page: a title, a body and a small line, centred, with one of the [`Footer`]s.
pub struct CardInfoPage {
    title: String,
    body_text: String,
    small: String,
    tone: Tone,
    small_in_title_colour: bool,
    small_is_stop_line: bool,
    footer: Footer,
    body: InfoBody,
}

impl CardInfoPage {
    pub fn new(title: &str, body: &str, small: &str, footer: Footer) -> Self {
        let mut page = Self {
            title: String::from(title),
            body_text: String::from(body),
            small: String::from(small),
            tone: Tone::Normal,
            small_in_title_colour: false,
            small_is_stop_line: false,
            footer,
            body: InfoBody::Plain(TextBlock::new(&[], 0, SCREEN_WIDTH)),
        };
        page.build();
        page
    }

    pub fn with_tone(mut self, tone: Tone) -> Self {
        if self.tone != tone {
            self.tone = tone;
            self.build();
        }
        self
    }

    pub fn with_small_in_title_colour(mut self, enabled: bool) -> Self {
        if self.small_in_title_colour != enabled {
            self.small_in_title_colour = enabled;
            self.build();
        }
        self
    }

    pub fn footer(&self) -> Footer {
        self.footer
    }

    /// Whether the user has left the page: swiped, held or tapped the button.
    pub fn is_done(&self) -> bool {
        match &self.body {
            InfoBody::Plain(_) => false,
            InfoBody::Swipe(_, footer) => footer.is_swiped(),
            InfoBody::Hold(hold) => hold.is_finished(),
            InfoBody::Button { pressed, .. } => *pressed,
        }
    }

    fn build(&mut self) {
        let title_color = self.tone.color();
        let small_color = if self.small_in_title_colour {
            title_color
        } else {
            PALETTE.text_secondary
        };
        let hold_label = match self.footer {
            Footer::Hold(Some(label)) => label,
            _ => "",
        };
        let small = if self.small_is_stop_line {
            Para::new(&self.small, FONT_BODY, title_color)
        } else {
            Para::new(&self.small, FONT_SMALL_LINE, small_color)
        };
        let paras = [
            Para::new(&self.title, FONT_TITLE, title_color),
            Para::new(&self.body_text, FONT_BODY, PALETTE.on_surface_variant),
            small,
            Para::new(hold_label, FONT_SMALL_LINE, title_color),
        ];
        let top = TEXT_TOP;
        self.body = match self.footer {
            Footer::None => InfoBody::Plain(TextBlock::new(&paras, top, SCREEN_WIDTH)),
            Footer::Swipe => InfoBody::Swipe(
                TextBlock::new(&paras, top, SCREEN_WIDTH),
                Box::new(SwipeFooter::new()),
            ),
            Footer::Hold(_) => {
                // 📐 the hold border insets its content, so start that much higher
                let inset = HOLD_BORDER;
                let text = TextBlock::new(&paras, TEXT_TOP_HOLD - inset, SCREEN_WIDTH - 2 * inset);
                InfoBody::Hold(Box::new(HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, text)))
            }
            Footer::Button(label) => InfoBody::Button {
                text: TextBlock::new(&paras, top, SCREEN_WIDTH),
                button: Box::new(CardButton::new(label)),
                rect: Rectangle::zero(),
                touch: None,
                pressed: false,
            },
        };
    }

    /// Every string the page prints.
    #[cfg(test)]
    pub(crate) fn texts(&self) -> [&str; 4] {
        let label = match self.footer {
            Footer::Hold(Some(label)) | Footer::Button(label) => label,
            _ => "",
        };
        [&self.title, &self.body_text, &self.small, label]
    }

    /// The y where the text ends, in screen coordinates.
    #[cfg(test)]
    pub(crate) fn text_bottom(&self) -> u32 {
        match &self.body {
            InfoBody::Plain(text) | InfoBody::Swipe(text, _) | InfoBody::Button { text, .. } => {
                text.bottom()
            }
            InfoBody::Hold(hold) => hold.widget().bottom() + HOLD_BORDER,
        }
    }

    #[cfg(test)]
    pub(crate) fn widest_line(&self) -> u32 {
        match &self.body {
            InfoBody::Plain(text) | InfoBody::Swipe(text, _) | InfoBody::Button { text, .. } => {
                text.widest_line()
            }
            InfoBody::Hold(hold) => hold.widget().widest_line(),
        }
    }

    #[cfg(test)]
    pub(crate) fn button_label_width(&self) -> Option<u32> {
        match &self.body {
            InfoBody::Button { button, .. } => Some(button.label_width()),
            _ => None,
        }
    }
}

/// The border `HoldToConfirm` draws around its content.
const HOLD_BORDER: u32 = 5;

impl DynWidget for CardInfoPage {
    fn set_constraints(&mut self, max_size: Size) {
        match &mut self.body {
            InfoBody::Plain(text) => text.set_constraints(max_size),
            InfoBody::Swipe(text, footer) => {
                text.set_constraints(max_size);
                footer.set_constraints(max_size);
            }
            InfoBody::Hold(hold) => hold.set_constraints(max_size),
            InfoBody::Button { text, rect, .. } => {
                text.set_constraints(max_size);
                *rect = button_rect(max_size);
            }
        }
    }

    fn sizing(&self) -> crate::Sizing {
        match &self.body {
            InfoBody::Plain(text) | InfoBody::Swipe(text, _) | InfoBody::Button { text, .. } => {
                text.sizing()
            }
            InfoBody::Hold(hold) => hold.sizing(),
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        match &mut self.body {
            InfoBody::Hold(hold) => hold.handle_touch(point, current_time, is_release),
            InfoBody::Button {
                rect,
                touch,
                pressed,
                ..
            } => {
                if is_release {
                    if let Some(touch) = touch {
                        if !touch.has_been_let_go()
                            && touch.let_go(current_time) == Some(Key::Continue)
                        {
                            *pressed = true;
                        }
                    }
                } else if !*pressed && rect.contains(point) {
                    *touch = Some(Box::new(KeyTouch::new(Key::Continue, *rect)));
                }
                None
            }
            _ => None,
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        match &mut self.body {
            InfoBody::Swipe(_, footer) => footer.handle_vertical_drag(prev_y, new_y, is_release),
            InfoBody::Button {
                touch: Some(touch), ..
            } => touch.cancel(),
            _ => {}
        }
    }

    fn force_full_redraw(&mut self) {
        match &mut self.body {
            InfoBody::Plain(text) => text.force_full_redraw(),
            InfoBody::Swipe(text, footer) => {
                text.force_full_redraw();
                footer.force_full_redraw();
            }
            InfoBody::Hold(hold) => hold.force_full_redraw(),
            InfoBody::Button { text, button, .. } => {
                text.force_full_redraw();
                button.force_full_redraw();
            }
        }
    }
}

impl Widget for CardInfoPage {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        match &mut self.body {
            InfoBody::Plain(text) => text.draw(target, current_time),
            InfoBody::Swipe(text, footer) => {
                text.draw(target, current_time)?;
                footer.draw(target, current_time)
            }
            InfoBody::Hold(hold) => hold.draw(target, current_time),
            InfoBody::Button {
                text,
                button,
                rect,
                touch,
                ..
            } => {
                text.draw(target, current_time)?;
                if let Some(t) = touch {
                    t.draw(target, current_time);
                    if t.is_finished() {
                        *touch = None;
                        button.force_full_redraw();
                    }
                }
                button.draw(&mut target.clone().crop(*rect), current_time)
            }
        }
    }
}
