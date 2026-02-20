use crate::palette::PALETTE;
use crate::{DynWidget as _, Key, KeyTouch};
use alloc::rc::Rc;
use core::cell::RefCell;
use embedded_graphics::{
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU4},
        Gray4, GrayColor, Rgb565,
    },
    prelude::*,
    primitives::Rectangle,
};

use super::{
    backup_status_bar::{BackupStatus, BackupStatusBar, STATUS_BAR_HEIGHT},
    input_preview::{Fb, FB_WIDTH, FONT_SIZE, TOP_PADDING, TOTAL_WORDS, VERTICAL_PAD},
    ViewState,
};
use crate::scroll_bar::{ScrollBar, SCROLLBAR_WIDTH};

const WORD_LIST_LEFT_PAD: i32 = 4; // Left padding for word list
/// Shift touch outline up to visually center on text. The font has 7px of descender
/// space below the baseline that most glyphs don't use, so the visual center of the
/// text is higher than the geometric center of the row.
const TOUCH_Y_ADJUST: i32 = -3;

pub struct EnteredWords {
    framebuffer: Rc<RefCell<Fb>>,
    view_state: ViewState,
    scroll_position: i32,
    visible_size: Size,
    needs_redraw: bool,
    status_bar: BackupStatusBar,
    scroll_bar: ScrollBar,
    first_draw: bool,
}

impl EnteredWords {
    /// Calculate the actual content height based on n_completed
    fn calculate_content_height(&self) -> u32 {
        // Show all rows up to and including the one being edited
        // view_state.row is the row currently being edited (0 = share index, 1+ = words)
        let visible_rows = (self.view_state.row + 1).min(TOTAL_WORDS + 1); // +1 to include current row
        TOP_PADDING + (visible_rows as u32 * (FONT_SIZE.height + VERTICAL_PAD))
    }

    pub fn new(framebuffer: Rc<RefCell<Fb>>, visible_size: Size, view_state: ViewState) -> Self {
        // Get status based on view_state
        use super::backup_model::MainViewState;

        let status = match &view_state.main_view {
            MainViewState::AllWordsEntered { success } => match success {
                Some(_) => BackupStatus::Valid,
                None => BackupStatus::InvalidChecksum,
            },
            _ => {
                // row 0 is share index, so completed words = row - 1 when row > 0
                let completed_words = if view_state.row > 0 {
                    view_state.row - 1
                } else {
                    0
                };
                BackupStatus::Incomplete {
                    words_entered: completed_words,
                }
            }
        };
        let mut status_bar = BackupStatusBar::new(status);
        use crate::DynWidget;
        status_bar.set_constraints(Size::new(visible_size.width, STATUS_BAR_HEIGHT));

        // Use a fixed thumb size for now
        let thumb_size = crate::Frac::from_ratio(1, 4); // 25% of scrollbar height
        let scroll_bar = ScrollBar::new(thumb_size);

        Self {
            framebuffer: framebuffer.clone(),
            view_state,
            scroll_position: 0,
            visible_size,
            needs_redraw: true,
            status_bar,
            scroll_bar,
            first_draw: true,
        }
    }

    pub fn scroll_to_word_at_top(&mut self, word_index: usize) {
        // Get the actual number of visible rows up to and including current
        let visible_rows = (self.view_state.row + 1).min(TOTAL_WORDS + 1);

        // Clamp the word index to what's actually visible
        let clamped_word_index = word_index.min(visible_rows.saturating_sub(1));

        let row_height = FONT_SIZE.height + VERTICAL_PAD;

        // Calculate scroll to show word at top of scrollable area
        let desired_scroll = clamped_word_index as i32 * row_height as i32;

        // Get dynamic content height
        let content_height = self.calculate_content_height();

        // Calculate max scroll based on dynamic content
        let scrollable_height = self.visible_size.height as i32 - STATUS_BAR_HEIGHT as i32;
        let max_scroll = (content_height as i32)
            .saturating_sub(scrollable_height)
            .max(0);
        self.scroll_position = desired_scroll.clamp(0, max_scroll);
        let fraction = if max_scroll > 0 {
            crate::Rat::from_ratio(self.scroll_position as u32, max_scroll as u32)
        } else {
            crate::Rat::ZERO
        };
        self.scroll_bar.set_scroll_position(fraction);
        self.needs_redraw = true;
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut crate::SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) {
        if !self.needs_redraw {
            return;
        }

        let bounds = target.bounding_box();

        // Clear entire screen on first draw
        if self.first_draw {
            let _ = target.clear(PALETTE.background);
        }

        // Create a cropped target that matches the framebuffer width, centered
        let cropped_rect = Rectangle::new(
            Point::new(WORD_LIST_LEFT_PAD, 0),
            Size::new(FB_WIDTH, bounds.size.height),
        );

        let mut cropped_target = target.clone().crop(cropped_rect);
        let cropped_bounds = cropped_target.bounding_box();

        // Get dynamic content height
        let content_height = self.calculate_content_height();

        // Draw words framebuffer in scrollable area only
        if self.scroll_position < content_height as i32 {
            let scrollable_height = (self.visible_size.height - STATUS_BAR_HEIGHT) as i32;
            let skip_pixels = (self.scroll_position.max(0) as usize) * FB_WIDTH as usize;
            let words_visible_height =
                (content_height as i32 - self.scroll_position).min(scrollable_height) as usize;
            let take_pixels = words_visible_height * FB_WIDTH as usize;

            {
                let fb = self.framebuffer.try_borrow().unwrap();

                let color_lut = {
                    use crate::{ColorInterpolate, Frac};
                    let mut lut = [PALETTE.background; 16];
                    for i in 1..16u8 {
                        let alpha = Frac::from_ratio(i as u32, 15);
                        lut[i as usize] =
                            PALETTE.background.interpolate(PALETTE.on_background, alpha);
                    }
                    lut
                };

                let framebuffer_pixels = RawDataSlice::<RawU4, LittleEndian>::new(fb.data())
                    .into_iter()
                    .skip(skip_pixels)
                    .take(take_pixels)
                    .map(|r| color_lut[Gray4::from(r).luma() as usize]);

                let words_rect = Rectangle::new(
                    Point::zero(),
                    Size::new(cropped_bounds.size.width, words_visible_height as u32),
                );
                let _ = cropped_target.fill_contiguous(&words_rect, framebuffer_pixels);
            } // fb borrow is dropped here
        }

        // Calculate status bar position
        let status_y = bounds.size.height as i32 - STATUS_BAR_HEIGHT as i32;

        // Draw status bar at fixed position at bottom of screen (full width)
        use crate::Widget;
        let status_area = Rectangle::new(
            Point::new(0, status_y),
            Size::new(bounds.size.width, STATUS_BAR_HEIGHT),
        );
        let _ = self
            .status_bar
            .draw(&mut target.clone().crop(status_area), current_time);

        // Draw scroll bar
        const SCROLLBAR_MARGIN: u32 = 0; // No margin from right edge
        const SCROLLBAR_TOP_MARGIN: u32 = 30; // Increased top margin
        const SCROLLBAR_BOTTOM_MARGIN: u32 = 2; // Bottom margin

        let scrollbar_x = bounds.size.width as i32 - (SCROLLBAR_WIDTH + SCROLLBAR_MARGIN) as i32;
        let scrollbar_y = SCROLLBAR_TOP_MARGIN as i32;
        let scrollbar_height = (bounds.size.height - STATUS_BAR_HEIGHT)
            - SCROLLBAR_TOP_MARGIN
            - SCROLLBAR_BOTTOM_MARGIN;

        let scrollbar_area = Rectangle::new(
            Point::new(scrollbar_x, scrollbar_y),
            Size::new(SCROLLBAR_WIDTH, scrollbar_height),
        );
        self.scroll_bar
            .draw(&mut target.clone().crop(scrollbar_area));

        self.needs_redraw = false;
        self.first_draw = false;
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        // Status bar is not interactive, so skip checking it
        let status_y = self.visible_size.height as i32 - STATUS_BAR_HEIGHT as i32;
        if point.y >= status_y {
            return None; // Touch is in status bar area
        }

        if point.x < WORD_LIST_LEFT_PAD || point.x >= WORD_LIST_LEFT_PAD + FB_WIDTH as i32 {
            return None; // Touch is outside content area
        }

        // Adjust point for content offset
        let content_point =
            Point::new(point.x - WORD_LIST_LEFT_PAD, point.y + self.scroll_position);

        // Calculate which word was touched using row height with padding
        // Account for TOP_PADDING in the framebuffer
        let row_height = (FONT_SIZE.height + VERTICAL_PAD) as i32;
        let adjusted_y = content_point.y - TOP_PADDING as i32;

        if adjusted_y < 0 {
            return None; // Touch is in the top padding area
        }

        let word_index = (adjusted_y / row_height) as usize;

        // Get the number of visible rows up to and including current
        let visible_rows = (self.view_state.row + 1).min(TOTAL_WORDS + 1);

        if word_index >= visible_rows {
            return None; // Word not visible
        }

        // Only completed rows and current row can be edited
        if word_index > self.view_state.row {
            return None;
        }

        // Create a rectangle for the touched word (includes padding)
        // Add TOP_PADDING since words are offset in the framebuffer
        let y = TOP_PADDING as i32 + (word_index as i32 * row_height) - self.scroll_position
            + TOUCH_Y_ADJUST;
        let status_y = self.visible_size.height as i32 - STATUS_BAR_HEIGHT as i32;

        // Clip the rectangle height if it would extend into the status area
        let max_height = (status_y - y).max(0) as u32;
        let rect_height = (FONT_SIZE.height + VERTICAL_PAD).min(max_height);

        // Only return a touch if the rectangle has some height
        if rect_height > 0 {
            // Calculate width excluding scrollbar area
            let touch_width = self.visible_size.width - SCROLLBAR_WIDTH;

            let rect = Rectangle::new(
                Point::new(0, y), // x=0 as requested
                Size::new(touch_width, rect_height),
            );
            Some(KeyTouch::new(Key::EditWord(word_index), rect))
        } else {
            None
        }
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);

        // Only scroll if there's a meaningful delta
        if delta.abs() > 0 {
            self.scroll(delta);
        }
    }

    fn scroll(&mut self, amount: i32) {
        // Get dynamic content height
        let content_height = self.calculate_content_height();

        // Scrollable area is screen height minus status bar height
        let scrollable_height = self.visible_size.height as i32 - STATUS_BAR_HEIGHT as i32;
        let max_scroll = (content_height as i32)
            .saturating_sub(scrollable_height)
            .max(0);
        let new_scroll_position = (self.scroll_position - amount).clamp(0, max_scroll);

        // Only redraw if position actually changed
        if new_scroll_position != self.scroll_position {
            self.scroll_position = new_scroll_position;
            let fraction = if max_scroll > 0 {
                crate::Rat::from_ratio(self.scroll_position as u32, max_scroll as u32)
            } else {
                crate::Rat::ZERO
            };
            self.scroll_bar.set_scroll_position(fraction);
            self.needs_redraw = true;
        }
    }

    pub fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
        self.scroll_bar.force_full_redraw();
        self.status_bar.force_full_redraw();
    }
}
