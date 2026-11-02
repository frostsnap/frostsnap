use super::{
    clear_rect, restore_review::page_of_box, BoxEntry, BoxEntryScreen, CardRestoreReview,
    CARD_BOXES,
};
use crate::{prelude::*, KeyTouch};
use alloc::boxed::Box;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};

enum RestoreView {
    /// Only between dropping one view and building the next.
    Empty,
    Entry(Box<BoxEntryScreen>),
    Review(Box<CardRestoreReview>),
}

/// R1, restore from a card: words 1-32 in order, then the review, where a row can be entered
/// again, until the hold "Done". The app takes words 33-64.
///
/// 🧠 The entry screen (its keyboard is 42 KB) and the review pages never exist at the same
/// time: one is dropped before the other is built.
pub struct CardRestoreEntry {
    entries: [Option<u16>; CARD_BOXES],
    view: RestoreView,
    /// The word being entered (1-based).
    box_number: u8,
    /// Entering one word from the review, rather than the whole half.
    editing_one: bool,
    finished: bool,
    needs_clear: bool,
    size: Size,
}

impl Default for CardRestoreEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl CardRestoreEntry {
    pub fn new() -> Self {
        Self {
            entries: [None; CARD_BOXES],
            view: RestoreView::Entry(Box::new(BoxEntryScreen::new(1))),
            box_number: 1,
            editing_one: false,
            finished: false,
            needs_clear: false,
            size: Size::zero(),
        }
    }

    /// Every word's entry (`None` for "Can't read this word"), once the user has held "Done".
    pub fn entries(&self) -> Option<[Option<u16>; CARD_BOXES]> {
        self.finished.then_some(self.entries)
    }

    /// The review is animating a page transition.
    pub fn is_sliding(&self) -> bool {
        matches!(&self.view, RestoreView::Review(review) if review.is_sliding())
    }

    /// Enter word `box_number` (1-based) again, then return to the review.
    pub fn edit(&mut self, box_number: u8) {
        let box_number = box_number.clamp(1, CARD_BOXES as u8);
        self.box_number = box_number;
        self.editing_one = true;
        self.finished = false;
        if let RestoreView::Entry(screen) = &mut self.view {
            screen.reset(box_number);
            return;
        }
        self.view = RestoreView::Empty;
        self.set_view(RestoreView::Entry(Box::new(BoxEntryScreen::new(
            box_number,
        ))));
    }

    /// Show the review with `entries`, e.g. to correct them after a failed decode.
    pub fn review(&mut self, entries: [Option<u16>; CARD_BOXES]) {
        self.entries = entries;
        self.finished = false;
        self.editing_one = false;
        self.show_review(0);
    }

    fn show_review(&mut self, page: usize) {
        self.view = RestoreView::Empty;
        self.set_view(RestoreView::Review(Box::new(CardRestoreReview::new(
            self.entries,
            page,
        ))));
    }

    fn set_view(&mut self, mut view: RestoreView) {
        match &mut view {
            RestoreView::Entry(screen) => screen.set_constraints(self.size),
            RestoreView::Review(review) => review.set_constraints(self.size),
            RestoreView::Empty => {}
        }
        self.view = view;
        self.needs_clear = true;
    }

    fn view_widget(&mut self) -> Option<&mut dyn DynWidget> {
        match &mut self.view {
            RestoreView::Entry(screen) => Some(screen.as_mut()),
            RestoreView::Review(review) => Some(review.as_mut()),
            RestoreView::Empty => None,
        }
    }

    fn step(&mut self) {
        match &mut self.view {
            RestoreView::Entry(screen) => {
                let Some(entry) = screen.result().filter(|_| screen.is_settled()) else {
                    return;
                };
                self.entries[self.box_number as usize - 1] = match entry {
                    BoxEntry::Word(word) => Some(word),
                    BoxEntry::CantRead => None,
                };
                if self.editing_one || self.box_number as usize == CARD_BOXES {
                    self.editing_one = false;
                    self.show_review(page_of_box(self.box_number));
                } else {
                    self.box_number += 1;
                    screen.reset(self.box_number);
                }
            }
            RestoreView::Review(review) => {
                if let Some(box_number) = review.edit_requested() {
                    self.edit(box_number);
                } else if review.is_confirmed() {
                    self.finished = true;
                }
            }
            RestoreView::Empty => {}
        }
    }
}

impl DynWidget for CardRestoreEntry {
    fn set_constraints(&mut self, max_size: Size) {
        self.size = max_size;
        if let Some(view) = self.view_widget() {
            view.set_constraints(max_size);
        }
    }

    fn sizing(&self) -> crate::Sizing {
        self.size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        self.view_widget()?
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if let Some(view) = self.view_widget() {
            view.handle_vertical_drag(prev_y, new_y, is_release);
        }
    }

    fn force_full_redraw(&mut self) {
        if let Some(view) = self.view_widget() {
            view.force_full_redraw();
        }
    }
}

impl Widget for CardRestoreEntry {
    type Color = Rgb565;

    #[inline(never)]
    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.step();

        if core::mem::take(&mut self.needs_clear) {
            clear_rect(target, Rectangle::new(Point::zero(), self.size))?;
            self.force_full_redraw();
        }

        match &mut self.view {
            RestoreView::Entry(screen) => screen.draw(target, current_time),
            RestoreView::Review(review) => review.draw(target, current_time),
            RestoreView::Empty => Ok(()),
        }
    }
}
