use super::{
    word_spelling, Para, TextBlock, BLOCK_GAP, CARD_BOXES, FONT_BODY, FONT_TITLE, SCREEN_WIDTH,
    TEXT_TOP, TEXT_TOP_HOLD, TEXT_WIDTH,
};
use crate::{
    any_of::AnyOf,
    gray4_style::Gray4TextStyle,
    page_slider::PageSlider,
    palette::PALETTE,
    prelude::*,
    widget_list::{WidgetList, WidgetListItem},
    HoldToConfirm, Key, KeyTouch, FONT_HUGE_MONO, FONT_MED, HOLD_TO_CONFIRM_TIME_MS,
};
use alloc::{format, string::String, vec::Vec};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Rectangle, StrokeAlignment},
};

const ROWS_PER_PAGE: usize = 3;
/// Pages of rows; the "Done" hold page follows them.
pub const REVIEW_ROW_PAGES: usize = CARD_BOXES.div_ceil(ROWS_PER_PAGE);
const ROW_HEIGHT: u32 = 40;
const ROW_GAP: u32 = 6;
const HOLD_BORDER: u32 = 5;
const ROW_LABEL_X: i32 = 12;
/// 📐 a fixed word column, wide enough for "32."
const ROW_WORD_X: i32 = 52;
const MISSING: &str = "----";
pub(crate) const REVIEW_TITLE: &str = "Review words 1-32";
pub(crate) const HOLD_TITLE: &str = "Done";
pub(crate) const HOLD_BODY: &str = "Hold when words 1-32 match your card.";

/// The review page that shows word `box_number` (1-based).
pub(crate) fn page_of_box(box_number: u8) -> usize {
    (box_number.max(1) as usize - 1) / ROWS_PER_PAGE
}

/// "13. SIZE", or "13. ----" for a word entered as unreadable. Tapping it enters the word
/// again.
struct ReviewRow {
    box_number: u8,
    rect: Rectangle,
    background: Container<SizedBox<Rgb565>>,
    label: Text<Gray4TextStyle>,
    word: Text<Gray4TextStyle>,
}

impl ReviewRow {
    fn new(box_number: u8, entry: Option<u16>, top: u32) -> Self {
        let size = Size::new(TEXT_WIDTH, ROW_HEIGHT);
        let mut background = Container::with_size(SizedBox::new(size), size)
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(8, 8));
        background.set_constraints(size);
        let label = Text::new(
            format!("{box_number}."),
            Gray4TextStyle::new(FONT_MED, PALETTE.text_secondary),
        );
        let word = match entry {
            Some(word) => Text::new(
                String::from(word_spelling(word)),
                Gray4TextStyle::new(FONT_HUGE_MONO, PALETTE.primary),
            ),
            None => Text::new(
                String::from(MISSING),
                Gray4TextStyle::new(FONT_HUGE_MONO, PALETTE.text_secondary),
            ),
        };
        Self {
            box_number,
            rect: Rectangle::new(
                Point::new(((SCREEN_WIDTH - TEXT_WIDTH) / 2) as i32, top as i32),
                size,
            ),
            background,
            label,
            word,
        }
    }

    fn force_full_redraw(&mut self) {
        self.background.force_full_redraw();
        self.label.force_full_redraw();
        self.word.force_full_redraw();
    }

    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        let mut row_target = target.clone().crop(self.rect);
        self.background.draw(&mut row_target, current_time)?;
        let on_surface = row_target.with_background_color(PALETTE.surface);
        // ⚖️ the label sits on the word's baseline
        let word_size: Size = self.word.sizing().into();
        let word_top = (ROW_HEIGHT as i32 - word_size.height as i32) / 2;
        let label_size: Size = self.label.sizing().into();
        let label_top = word_top + FONT_HUGE_MONO.baseline as i32 - FONT_MED.baseline as i32;
        self.label.draw(
            &mut on_surface.clone().crop(Rectangle::new(
                Point::new(ROW_LABEL_X, label_top),
                label_size,
            )),
            current_time,
        )?;
        self.word.draw(
            &mut on_surface.crop(Rectangle::new(Point::new(ROW_WORD_X, word_top), word_size)),
            current_time,
        )
    }
}

/// A page of up to 3 rows under "Review words 1-32".
pub struct ReviewRowsPage {
    title: TextBlock,
    rows: Vec<ReviewRow>,
}

impl ReviewRowsPage {
    pub(crate) fn new(page: usize, entries: &[Option<u16>; CARD_BOXES]) -> Self {
        let title = TextBlock::new(
            &[Para::new(REVIEW_TITLE, FONT_TITLE, PALETTE.on_background)],
            TEXT_TOP,
            SCREEN_WIDTH,
        );
        let mut top = title.bottom() + BLOCK_GAP;
        let first = page * ROWS_PER_PAGE;
        let last = (first + ROWS_PER_PAGE).min(CARD_BOXES);
        let rows = (first..last)
            .map(|index| {
                let row = ReviewRow::new(index as u8 + 1, entries[index], top);
                top += ROW_HEIGHT + ROW_GAP;
                row
            })
            .collect();
        Self { title, rows }
    }

    #[cfg(test)]
    pub(crate) fn content_bottom(&self) -> u32 {
        self.rows.last().map_or(self.title.bottom(), |row| {
            row.rect.bottom_right().unwrap().y as u32 + 1
        })
    }

    /// How far into a row the widest entry reaches: an 8-letter word.
    #[cfg(test)]
    pub(crate) fn widest_row_content() -> u32 {
        ROW_WORD_X as u32 + super::text_width("WWWWWWWW", FONT_HUGE_MONO)
    }
}

impl DynWidget for ReviewRowsPage {
    fn set_constraints(&mut self, max_size: Size) {
        self.title.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.title.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        if is_release {
            return None;
        }
        self.rows
            .iter()
            .find(|row| row.rect.contains(point))
            .map(|row| {
                // ✏️ outside the row, so the highlight fades to the background, not into the row
                KeyTouch::new(Key::EditWord(row.box_number as usize), row.rect)
                    .with_stroke_alignment(StrokeAlignment::Outside)
            })
    }

    fn force_full_redraw(&mut self) {
        self.title.force_full_redraw();
        for row in &mut self.rows {
            row.force_full_redraw();
        }
    }
}

impl Widget for ReviewRowsPage {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.title.draw(target, current_time)?;
        for row in &mut self.rows {
            row.draw(target, current_time)?;
        }
        Ok(())
    }
}

pub(crate) type ReviewHoldPage = HoldToConfirm<TextBlock>;

pub(crate) fn review_hold_page() -> ReviewHoldPage {
    let text = TextBlock::new(
        &[
            Para::new(HOLD_TITLE, FONT_TITLE, PALETTE.on_background),
            Para::new(HOLD_BODY, FONT_BODY, PALETTE.on_surface_variant),
        ],
        TEXT_TOP_HOLD - HOLD_BORDER,
        SCREEN_WIDTH - 2 * HOLD_BORDER,
    );
    HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_MS, text)
}

type ReviewPage = AnyOf<(ReviewRowsPage, ReviewHoldPage)>;

struct ReviewPages {
    entries: [Option<u16>; CARD_BOXES],
}

impl WidgetList for ReviewPages {
    type Widget = ReviewPage;

    fn len(&self) -> usize {
        REVIEW_ROW_PAGES + 1
    }

    fn get(&self, index: usize) -> Option<WidgetListItem<ReviewPage>> {
        if index < REVIEW_ROW_PAGES {
            Some(WidgetListItem::new(AnyOf::new(ReviewRowsPage::new(
                index,
                &self.entries,
            ))))
        } else if index == REVIEW_ROW_PAGES {
            Some(WidgetListItem::new(AnyOf::new(review_hold_page())))
        } else {
            None
        }
    }

    fn can_go_prev(&self, from_index: usize, current_widget: &ReviewPage) -> bool {
        let held = current_widget
            .downcast_ref::<ReviewHoldPage>()
            .is_some_and(|hold| hold.is_confirmed());
        from_index > 0 && !held
    }
}

/// The restore review: every entry, 3 per page; tap a row to enter that word again. The last
/// page is the hold "Done".
pub struct CardRestoreReview {
    slider: PageSlider<ReviewPages>,
    touch: Option<KeyTouch>,
    edit: Option<u8>,
}

impl CardRestoreReview {
    pub fn new(entries: [Option<u16>; CARD_BOXES], start_page: usize) -> Self {
        let mut slider = PageSlider::new_at(ReviewPages { entries }, start_page);
        slider.enable_swipe_up_chevron();
        Self {
            slider,
            touch: None,
            edit: None,
        }
    }

    /// A page transition is animating.
    pub fn is_sliding(&self) -> bool {
        self.slider.is_sliding()
    }

    /// The word (1-based) whose row the user tapped.
    pub fn edit_requested(&self) -> Option<u8> {
        self.edit
    }

    /// The user held "Done".
    pub fn is_confirmed(&mut self) -> bool {
        self.slider
            .current_widget()
            .downcast_ref::<ReviewHoldPage>()
            .is_some_and(|hold| hold.is_finished())
    }
}

impl DynWidget for CardRestoreReview {
    fn set_constraints(&mut self, max_size: Size) {
        self.slider.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.slider.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        if is_release {
            // the hold page needs to hear the release
            let _ = self.slider.handle_touch(point, current_time, true);
            if let Some(touch) = &mut self.touch {
                if let Some(Key::EditWord(box_number)) = touch.let_go(current_time) {
                    self.edit = Some(box_number as u8);
                }
            }
            return None;
        }
        if self.slider.is_sliding() || self.edit.is_some() {
            return None;
        }
        if let Some(key_touch) = self.slider.handle_touch(point, current_time, false) {
            if let Some(old) = &mut self.touch {
                old.cancel();
            }
            self.touch = Some(key_touch);
        }
        None
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if let Some(touch) = &mut self.touch {
            touch.cancel();
        }
        self.slider.handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        self.slider.force_full_redraw();
    }
}

impl Widget for CardRestoreReview {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.slider.draw(target, current_time)?;
        if let Some(touch) = &mut self.touch {
            touch.draw(target, current_time);
            if touch.is_finished() {
                self.touch = None;
            }
        }
        Ok(())
    }
}
