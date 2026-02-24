use crate::address_framebuffer::{build_lut, draw_gray4_string, measure_string_width};
use crate::DefaultTextStyle;
use crate::HOLD_TO_CONFIRM_TIME_MS;
use crate::{
    page_slider::PageSlider, palette::PALETTE, prelude::*,
    share_index::ShareIndexWidget, widget_list::WidgetList, FadeSwitcher, HoldToConfirm,
    FONT_HUGE_MONO, FONT_MED,
};
use alloc::{boxed::Box, format, rc::Rc, string::String, string::ToString, vec, vec::Vec};
use embedded_graphics::{
    geometry::Size,
    pixelcolor::{Gray4, GrayColor, Rgb565},
    prelude::*,
    primitives::Rectangle,
    text::Alignment,
};
use frost_backup::{bip39_words::BIP39_WORDS, NUM_WORDS};
use frostsnap_fonts::NOTO_SANS_14_LIGHT;

const WORDS_PER_PAGE: usize = 3;

/// A single page showing the share index
#[derive(frostsnap_macros::Widget)]
pub struct ShareIndexPage {
    #[widget_delegate]
    center: Center<Column<(Text, ShareIndexWidget)>>,
}

impl ShareIndexPage {
    fn new(share_index: u16) -> Self {
        let label = Text::new(
            "Key number",
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        );

        let share_index_widget = ShareIndexWidget::new(share_index, FONT_HUGE_MONO);

        let column = Column::builder()
            .push(label)
            .gap(8)
            .push(share_index_widget)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        let center = Center::new(column);

        Self { center }
    }
}

/// A row showing a word number and the word itself
#[derive(frostsnap_macros::Widget)]
pub struct WordRow {
    #[widget_delegate]
    row: Row<(Text, SizedBox<Rgb565>, Text)>,
}

impl WordRow {
    fn new(word_number: usize, word: &str) -> Self {
        let number_text = Text::new(
            format!("{}.", word_number),
            DefaultTextStyle::new(FONT_MED, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Left);

        let word_text = Text::new(
            String::from(word),
            DefaultTextStyle::new(FONT_HUGE_MONO, PALETTE.primary),
        )
        .with_alignment(Alignment::Left);

        let spacer = SizedBox::width(10); // 10 pixels of space between number and word
        let row = Row::new((number_text, spacer, word_text));

        Self { row }
    }
}

/// Enum for different word page layouts
type WordsPageLayout = Center<
    crate::any_of::AnyOf<(
        Column<(WordRow,)>,
        Column<(WordRow, WordRow)>,
        Column<(WordRow, WordRow, WordRow)>,
    )>,
>;

/// A page showing up to 3 words
#[derive(frostsnap_macros::Widget)]
pub struct WordsPage {
    #[widget_delegate]
    layout: WordsPageLayout,
}

impl WordsPage {
    fn new(words: Vec<(usize, String)>) -> Self {
        // Build the layout based on how many words we have
        let layout = match words.len() {
            1 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::new((row1,)).with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
            2 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::builder()
                        .push(row1)
                        .gap(20)
                        .push(row2)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
            3 => {
                let row1 = WordRow::new(words[0].0, &words[0].1);
                let row2 = WordRow::new(words[1].0, &words[1].1);
                let row3 = WordRow::new(words[2].0, &words[2].1);
                Center::new(crate::any_of::AnyOf::new(
                    Column::builder()
                        .push(row1)
                        .gap(20)
                        .push(row2)
                        .gap(20)
                        .push(row3)
                        .with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
            _ => {
                // Should never happen but handle gracefully
                let row1 = WordRow::new(1, "error");
                Center::new(crate::any_of::AnyOf::new(
                    Column::new((row1,)).with_cross_axis_alignment(CrossAxisAlignment::Start),
                ))
            }
        };

        Self { layout }
    }
}

/// Screen dimensions for AllWordsPage
const ALL_WORDS_SCREEN_WIDTH: u32 = 240;
const ALL_WORDS_FONT: &frostsnap_fonts::Gray4Font = &NOTO_SANS_14_LIGHT;
const ALL_WORDS_LINE_HEIGHT: u32 = 15;
const ALL_WORDS_NUM_ROWS: u32 = 13;
const ALL_WORDS_HEIGHT: u32 = ALL_WORDS_NUM_ROWS * ALL_WORDS_LINE_HEIGHT;

/// Pre-rendered Rgb565 pixels for the all-words page, cached via Rc.
/// Two Gray4 framebuffers are used during construction for full anti-aliasing
/// with separate colors, then converted to Rgb565 and dropped.
type AllWordsPixels = Rc<Box<[Rgb565]>>;

fn render_all_words_pixels(word_indices: &[u16; 25], share_index: u16) -> AllWordsPixels {
    use crate::vec_framebuffer::VecFramebuffer;

    let font = ALL_WORDS_FONT;
    let line_height = ALL_WORDS_LINE_HEIGHT;
    let width = ALL_WORDS_SCREEN_WIDTH as usize;
    let height = ALL_WORDS_HEIGHT as usize;

    let number_width = measure_string_width(font, "25.");
    let number_gap = 3u32;
    let word_x_offset = number_width + number_gap;

    let column_width = ALL_WORDS_SCREEN_WIDTH / 2;
    let left_col_x = 8u32;
    let right_col_x = column_width + 4;

    let mut num_fb = VecFramebuffer::<Gray4>::new(width, height);
    let mut wrd_fb = VecFramebuffer::<Gray4>::new(width, height);

    // Row 0: share index "#." + value
    draw_gray4_string(&mut num_fb, font, "#.", Point::new(left_col_x as i32, 0), 15);
    let share_str = format!("{}", share_index);
    draw_gray4_string(
        &mut wrd_fb,
        font,
        &share_str,
        Point::new((left_col_x + word_x_offset) as i32, 0),
        15,
    );

    // Left column rows 1-12: words 1-12
    for i in 0..12 {
        let y = ((i + 1) as u32 * line_height) as i32;
        let num_str = format!("{}.", i + 1);
        draw_gray4_string(&mut num_fb, font, &num_str, Point::new(left_col_x as i32, y), 15);
        let word = BIP39_WORDS[word_indices[i] as usize];
        draw_gray4_string(
            &mut wrd_fb,
            font,
            word,
            Point::new((left_col_x + word_x_offset) as i32, y),
            15,
        );
    }

    // Right column: words 13-25
    for i in 12..25 {
        let y = ((i - 12) as u32 * line_height) as i32;
        let num_str = format!("{}.", i + 1);
        draw_gray4_string(&mut num_fb, font, &num_str, Point::new(right_col_x as i32, y), 15);
        let word = BIP39_WORDS[word_indices[i] as usize];
        draw_gray4_string(
            &mut wrd_fb,
            font,
            word,
            Point::new((right_col_x + word_x_offset) as i32, y),
            15,
        );
    }

    // Convert to Rgb565 — Gray4 buffers are dropped after this
    let secondary_lut = build_lut(PALETTE.text_secondary);
    let primary_lut = build_lut(PALETTE.primary);
    let total_pixels = width * height;
    let mut pixels = vec![PALETTE.background; total_pixels].into_boxed_slice();

    for (i, (num_color, wrd_color)) in num_fb
        .contiguous_pixels()
        .zip(wrd_fb.contiguous_pixels())
        .take(total_pixels)
        .enumerate()
    {
        let num_val = num_color.luma() as usize;
        if num_val > 0 {
            pixels[i] = secondary_lut[num_val];
        } else {
            let wrd_val = wrd_color.luma() as usize;
            if wrd_val > 0 {
                pixels[i] = primary_lut[wrd_val];
            }
        }
    }

    Rc::new(pixels)
}

/// A page showing all 25 words in two columns with anti-aliased text.
/// Pre-rendered to Rgb565 (~91 KB) at construction, cached via Rc for fast blitting.
pub struct AllWordsPage {
    pixels: AllWordsPixels,
}

impl AllWordsPage {
    pub fn new(word_indices: &[u16; 25], share_index: u16) -> Self {
        Self::from_cached(render_all_words_pixels(word_indices, share_index))
    }

    fn from_cached(pixels: AllWordsPixels) -> Self {
        Self { pixels }
    }
}

impl crate::DynWidget for AllWordsPage {
    fn set_constraints(&mut self, _max_size: Size) {}

    fn sizing(&self) -> crate::Sizing {
        Size::new(ALL_WORDS_SCREEN_WIDTH, ALL_WORDS_HEIGHT).into()
    }

    fn force_full_redraw(&mut self) {}
}

impl crate::Widget for AllWordsPage {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        target.fill_contiguous(
            &Rectangle::new(
                Point::zero(),
                Size::new(ALL_WORDS_SCREEN_WIDTH, ALL_WORDS_HEIGHT),
            ),
            self.pixels.iter().copied(),
        )
    }
}

/// Type alias for the content that can be shown in the confirmation screen
type ConfirmationContent = crate::any_of::AnyOf<(ConfirmContent, SafetyReminder)>;

/// A confirmation screen that shows after backup and fades to a security reminder
pub struct BackupConfirmationScreen {
    hold_confirm: HoldToConfirm<FadeSwitcher<Center<ConfirmationContent>>>,
    fade_triggered: bool,
}

use crate::gray4_style::Gray4TextStyle;
use frostsnap_fonts::{NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM};

const FONT_CONFIRM_TITLE: &frostsnap_fonts::Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CONFIRM_TEXT: &frostsnap_fonts::Gray4Font = &NOTO_SANS_17_REGULAR;

/// The initial confirmation content
#[derive(frostsnap_macros::Widget)]
pub struct ConfirmContent {
    #[widget_delegate]
    column: Column<(
        SizedBox<Rgb565>,
        Column<(Text<Gray4TextStyle>, SizedBox<Rgb565>, Text<Gray4TextStyle>, Text<Gray4TextStyle>)>,
        SizedBox<Rgb565>,
        Text<Gray4TextStyle>,
        SizedBox<Rgb565>,
    )>,
}

/// The completion message that fades in after confirmation
#[derive(frostsnap_macros::Widget)]
pub struct SafetyReminder {
    #[widget_delegate]
    content: Column<(
        Text<Gray4TextStyle>,
        SizedBox<Rgb565>,
        Text<Gray4TextStyle>,
        Text<Gray4TextStyle>,
    )>,
}

impl ConfirmContent {
    fn new() -> Self {
        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 40));

        let line1 = Text::new(
            "Verify you've recorded:".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );
        let list_spacer = SizedBox::<Rgb565>::new(Size::new(1, 4));
        let line2 = Text::new(
            "- Key number".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );
        let line3 = Text::new(
            "- All 25 words".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );
        let subtitle = Column::new((line1, list_spacer, line2, line3))
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 15));

        let title = Text::new(
            "Hold to Confirm".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let spacer3 = SizedBox::<Rgb565>::new(Size::new(1, 40));

        let column = Column::new((spacer1, subtitle, spacer2, title, spacer3))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self { column }
    }
}

impl SafetyReminder {
    fn new() -> Self {
        let title = Text::new(
            "Backup Completed".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let spacer = SizedBox::<Rgb565>::new(Size::new(1, 15));

        let line1 = Text::new(
            "Store it safely in a".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );
        let line2 = Text::new(
            "secure location".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );

        let column = Column::new((title, spacer, line1, line2))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);

        Self { content: column }
    }
}

impl BackupConfirmationScreen {
    fn new() -> Self {
        let confirm_content = ConfirmContent::new();
        let initial_content = ConfirmationContent::new(confirm_content);
        let centered_content = Center::new(initial_content);

        let fade_switcher = FadeSwitcher::new(
            centered_content,
            500, // 500ms fade duration
        );
        let hold_confirm =
            HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, fade_switcher).with_faded_out_button();

        Self {
            hold_confirm,
            fade_triggered: false,
        }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_completed()
    }
}

impl crate::DynWidget for BackupConfirmationScreen {
    fn set_constraints(&mut self, max_size: Size) {
        self.hold_confirm.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.hold_confirm.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.hold_confirm
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_confirm
            .handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.hold_confirm.force_full_redraw();
    }
}

impl crate::Widget for BackupConfirmationScreen {
    type Color = Rgb565;

    fn draw<D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Check if we should trigger the fade
        if !self.fade_triggered && self.hold_confirm.is_completed() {
            self.fade_triggered = true;
            // Switch to the safety reminder
            let safety_reminder = SafetyReminder::new();
            let safety_content = ConfirmationContent::new(safety_reminder);
            let centered_safety = Center::new(safety_content);
            self.hold_confirm.widget_mut().switch_to(centered_safety);
        }

        self.hold_confirm.draw(target, current_time)
    }
}

/// A type that can be either a ShareIndexPage, WordsPage, AllWordsPage, or BackupConfirmationScreen
type BackupPage = crate::any_of::AnyOf<(
    ShareIndexPage,
    WordsPage,
    Center<AllWordsPage>,
    BackupConfirmationScreen,
)>;

/// Widget list that generates backup pages
pub struct BackupPageList {
    word_indices: [u16; 25],
    share_index: u16,
    total_pages: usize,
    all_words_pixels: AllWordsPixels,
}

impl BackupPageList {
    fn new(word_indices: [u16; 25], share_index: u16) -> Self {
        // Calculate total pages: 1 share index page + word pages + 1 all words page + 1 hold to confirm page
        let word_pages = NUM_WORDS.div_ceil(WORDS_PER_PAGE);
        let total_pages = 1 + word_pages + 1 + 1; // share + word pages + all words + confirm
        let all_words_pixels = render_all_words_pixels(&word_indices, share_index);

        Self {
            word_indices,
            share_index,
            total_pages,
            all_words_pixels,
        }
    }
}

impl WidgetList<BackupPage> for BackupPageList {
    fn len(&self) -> usize {
        self.total_pages
    }

    fn get(&self, index: usize) -> Option<BackupPage> {
        if index >= self.total_pages {
            return None;
        }

        let page = if index == 0 {
            // Share index page
            BackupPage::new(ShareIndexPage::new(self.share_index))
        } else if index == self.total_pages - 1 {
            // Last page - Backup confirmation screen
            BackupPage::new(BackupConfirmationScreen::new())
        } else if index == self.total_pages - 2 {
            // Second to last page - All words summary
            BackupPage::new(Center::new(AllWordsPage::from_cached(self.all_words_pixels.clone())))
        } else {
            // Words page
            let word_start_index = (index - 1) * WORDS_PER_PAGE;
            let mut words = Vec::new();

            for i in 0..WORDS_PER_PAGE {
                let word_index = word_start_index + i;
                if word_index < NUM_WORDS {
                    let word_number = word_index + 1;
                    let word = BIP39_WORDS[self.word_indices[word_index] as usize];
                    words.push((word_number, String::from(word)));
                }
            }

            BackupPage::new(WordsPage::new(words))
        };

        Some(page)
    }

    fn can_go_prev(&self, from_index: usize, current_widget: &BackupPage) -> bool {
        if from_index == 0 {
            return false;
        }
        // If we're on the last page (confirmation screen)
        if from_index == self.total_pages - 1 {
            // Check if the confirmation screen has been confirmed
            if let Some(confirmation_screen) =
                current_widget.downcast_ref::<BackupConfirmationScreen>()
            {
                // Don't allow going back if confirmed
                return !confirmation_screen.is_confirmed();
            }
        }
        true // Allow navigation for all other cases
    }
}

/// How long to show the "Backup Completed" screen before signaling done
const SUCCESS_DISPLAY_MS: u64 = 2000;

/// Main widget that displays backup words using PageSlider
pub struct BackupDisplay {
    page_slider: PageSlider<BackupPageList, BackupPage>,
    confirmed_at: Option<crate::Instant>,
    current_time: Option<crate::Instant>,
}

impl BackupDisplay {
    pub fn new(word_indices: [u16; 25], share_index: u16) -> Self {
        let page_list = BackupPageList::new(word_indices, share_index);
        let page_slider = PageSlider::new(page_list)
            .with_on_page_ready(|page| {
                // Try to downcast to BackupConfirmationScreen
                if let Some(confirmation_screen) = page.downcast_mut::<BackupConfirmationScreen>() {
                    // Fade in the button when the confirmation page is ready
                    confirmation_screen.hold_confirm.fade_in_button();
                }
            })
            .with_swipe_up_chevron();

        Self {
            page_slider,
            confirmed_at: None,
            current_time: None,
        }
    }

    /// Check if the backup has been confirmed and the success screen has been shown long enough
    pub fn is_confirmed(&mut self) -> bool {
        if let (Some(confirmed_at), Some(now)) = (self.confirmed_at, self.current_time) {
            let elapsed = now.saturating_duration_since(confirmed_at);
            return elapsed >= SUCCESS_DISPLAY_MS;
        }
        false
    }

    fn check_confirmed(&mut self) {
        if self.confirmed_at.is_some() {
            return;
        }
        if self.page_slider.current_index() == self.page_slider.total_pages() - 1 {
            let current_widget = self.page_slider.current_widget();
            if let Some(confirmation_screen) =
                current_widget.downcast_ref::<BackupConfirmationScreen>()
            {
                if confirmation_screen.is_confirmed() {
                    self.confirmed_at = self.current_time;
                }
            }
        }
    }
}

impl crate::DynWidget for BackupDisplay {
    fn set_constraints(&mut self, max_size: Size) {
        self.page_slider.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.page_slider.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.page_slider.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.page_slider.handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.page_slider.force_full_redraw();
    }
}

impl crate::Widget for BackupDisplay {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut crate::super_draw_target::SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: embedded_graphics::draw_target::DrawTarget<Color = Self::Color>,
    {
        self.current_time = Some(current_time);
        self.check_confirmed();
        self.page_slider.draw(target, current_time)
    }
}
