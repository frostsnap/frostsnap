use super::{AddressPage, SilentPaymentPage};
use crate::{
    any_of::AnyOf,
    gray4_style::Gray4TextStyle,
    palette::PALETTE,
    prelude::*,
    scroll_bar::{ScrollBar, SCROLLBAR_WIDTH},
    Frac, KeyTouch, Rat,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
};
use frostsnap_fonts::{NOTO_SANS_17_REGULAR, NOTO_SANS_18_MEDIUM};

#[derive(Clone, Debug, PartialEq)]
pub enum RecipientItem {
    Foreign {
        index: usize,
        address: bitcoin::Address,
        amount: bitcoin::Amount,
    },
    SilentPayment {
        index: usize,
        address: String,
        amount: bitcoin::Amount,
    },
}

impl RecipientItem {
    pub fn index(&self) -> usize {
        match self {
            RecipientItem::Foreign { index, .. } => *index,
            RecipientItem::SilentPayment { index, .. } => *index,
        }
    }

    pub fn amount(&self) -> bitcoin::Amount {
        match self {
            RecipientItem::Foreign { amount, .. } => *amount,
            RecipientItem::SilentPayment { amount, .. } => *amount,
        }
    }
}

const RECIPIENT_ROW_HEIGHT: i32 = 34;
const RECIPIENT_LIST_TOP_PAD: i32 = 6;
// Leaves room under the last row for the page slider's swipe up chevron.
const RECIPIENT_LIST_BOTTOM_PAD: i32 = 56;

pub struct RecipientListPage {
    recipients: Vec<RecipientItem>,
    rand_seed: u32,
    scroll_position: i32,
    scroll_bar: ScrollBar,
    drag_in_progress: bool,
    detail_page: Option<AnyOf<(AddressPage, SilentPaymentPage)>>,
    detail_needs_clear: bool,
    /// Set when the list must be cleared and redrawn: scrolled, resized, or returning from a
    /// recipient. Redrawing only then keeps the screen from flickering.
    list_dirty: bool,
    size: Size,
}

impl RecipientListPage {
    pub fn new(recipients: Vec<RecipientItem>, rand_seed: u32) -> Self {
        let scroll_bar = ScrollBar::new(Frac::from_ratio(1, 4));
        Self {
            recipients,
            rand_seed,
            scroll_position: 0,
            scroll_bar,
            drag_in_progress: false,
            detail_page: None,
            detail_needs_clear: true,
            list_dirty: true,
            size: Size::zero(),
        }
    }

    pub fn can_go_next(&self) -> bool {
        self.detail_page.is_none() && self.scroll_position >= self.max_scroll().saturating_sub(10)
    }

    pub fn can_go_prev(&self) -> bool {
        self.detail_page.is_none() && self.scroll_position <= 10
    }

    fn max_scroll(&self) -> i32 {
        let total_content_height = RECIPIENT_LIST_TOP_PAD
            + (self.recipients.len() as i32 * RECIPIENT_ROW_HEIGHT)
            + RECIPIENT_LIST_BOTTOM_PAD;
        (total_content_height - self.size.height as i32).max(0)
    }

    fn scroll(&mut self, delta: i32) {
        let max_scroll = self.max_scroll();
        let new_pos = (self.scroll_position - delta).clamp(0, max_scroll);
        if new_pos != self.scroll_position {
            self.scroll_position = new_pos;
            self.list_dirty = true;
            let fraction = if max_scroll > 0 {
                Rat::from_ratio(self.scroll_position as u32, max_scroll as u32)
            } else {
                Rat::ZERO
            };
            self.scroll_bar.set_scroll_position(fraction);
        }
    }

    fn open_recipient(&mut self, row_idx: usize) {
        if let Some(item) = self.recipients.as_slice().get(row_idx) {
            let mut page: AnyOf<(AddressPage, SilentPaymentPage)> = match item {
                RecipientItem::Foreign { index, address, .. } => {
                    AnyOf::new(AddressPage::new_with_seed(*index, address, self.rand_seed))
                }
                RecipientItem::SilentPayment {
                    index,
                    address,
                    amount,
                    ..
                } => AnyOf::new(SilentPaymentPage::new_with_seed(
                    *index,
                    address,
                    *amount,
                    self.rand_seed,
                )),
            };
            page.set_constraints(self.size);
            page.force_full_redraw();
            self.detail_page = Some(page);
            self.detail_needs_clear = true;
        }
    }
}

impl DynWidget for RecipientListPage {
    fn set_constraints(&mut self, max_size: Size) {
        self.size = max_size;
        self.list_dirty = true;
        let total_content_height = RECIPIENT_LIST_TOP_PAD
            + (self.recipients.len() as i32 * RECIPIENT_ROW_HEIGHT)
            + RECIPIENT_LIST_BOTTOM_PAD;
        let thumb_frac = if total_content_height > 0 {
            Frac::from_ratio(
                max_size.height.min(total_content_height as u32),
                total_content_height as u32,
            )
        } else {
            Frac::ONE
        };
        let mut scroll_bar = ScrollBar::new(thumb_frac);
        let scrollbar_height = max_size.height.saturating_sub(8);
        scroll_bar.set_constraints(Size::new(SCROLLBAR_WIDTH, scrollbar_height));
        self.scroll_bar = scroll_bar;
        if let Some(detail) = &mut self.detail_page {
            detail.set_constraints(max_size);
        }
    }

    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: self.size.width,
            height: self.size.height,
            ..Default::default()
        }
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: Instant,
        is_release: bool,
    ) -> Option<KeyTouch> {
        if self.detail_page.is_some() {
            if is_release {
                self.detail_page = None;
                self.detail_needs_clear = true;
                self.force_full_redraw();
            }
            return None;
        }

        if is_release {
            if self.drag_in_progress {
                self.drag_in_progress = false;
                return None;
            }

            // Scrollbar scrubbing area (right 24 pixels)
            if point.x >= (self.size.width as i32 - 24) {
                let max_scroll = self.max_scroll();
                if self.size.height > 0 && max_scroll > 0 {
                    let target_scroll = ((max_scroll as u64 * point.y.max(0) as u64)
                        / self.size.height as u64) as i32;
                    self.scroll_position = target_scroll.clamp(0, max_scroll);
                    self.list_dirty = true;
                    let fraction = Rat::from_ratio(self.scroll_position as u32, max_scroll as u32);
                    self.scroll_bar.set_scroll_position(fraction);
                }
                return None;
            }

            // Row touch
            let content_y = point.y + self.scroll_position - RECIPIENT_LIST_TOP_PAD;
            if content_y >= 0 {
                let row_idx = (content_y / RECIPIENT_ROW_HEIGHT) as usize;
                if row_idx < self.recipients.len() {
                    self.open_recipient(row_idx);
                }
            }
        }
        None
    }
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if self.detail_page.is_some() {
            return;
        }
        if is_release {
            return;
        }
        self.drag_in_progress = true;
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);
        if delta.abs() > 0 {
            self.scroll(delta);
        }
    }

    fn force_full_redraw(&mut self) {
        self.list_dirty = true;
        self.scroll_bar.force_full_redraw();
        if let Some(detail) = &mut self.detail_page {
            detail.force_full_redraw();
        }
    }
}

impl Widget for RecipientListPage {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        if let Some(detail) = &mut self.detail_page {
            if self.detail_needs_clear {
                let bounds = target.bounding_box();
                let bg_style = embedded_graphics::primitives::PrimitiveStyleBuilder::new()
                    .fill_color(PALETTE.background)
                    .build();
                bounds.into_styled(bg_style).draw(target)?;
                detail.force_full_redraw();
                self.detail_needs_clear = false;
            }

            detail.draw(target, current_time)?;
            let mut back_text = Text::new(
                "< Tap to return to list".to_string(),
                Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
            );
            let back_y = self.size.height as i32 - 25;
            let mut back_target = target.clone().translate(Point::new(12, back_y));
            back_text.draw(&mut back_target, current_time)?;
            return Ok(());
        }

        if self.list_dirty {
            let bounds = target.bounding_box();
            let bg_style = embedded_graphics::primitives::PrimitiveStyleBuilder::new()
                .fill_color(PALETTE.background)
                .build();
            bounds.into_styled(bg_style).draw(target)?;
            let screen_height = self.size.height as i32;
            let screen_width = self.size.width as i32;

            for (i, item) in self.recipients.iter().enumerate() {
                let row_y = RECIPIENT_LIST_TOP_PAD + (i as i32 * RECIPIENT_ROW_HEIGHT)
                    - self.scroll_position;
                if row_y + RECIPIENT_ROW_HEIGHT < 0 || row_y >= screen_height {
                    continue;
                }

                // Separator line
                let sep_style = embedded_graphics::primitives::PrimitiveStyleBuilder::new()
                    .stroke_color(PALETTE.surface_variant)
                    .stroke_width(1)
                    .build();
                embedded_graphics::primitives::Line::new(
                    Point::new(8, row_y + RECIPIENT_ROW_HEIGHT - 1),
                    Point::new(screen_width - 16, row_y + RECIPIENT_ROW_HEIGHT - 1),
                )
                .into_styled(sep_style)
                .draw(target)?;

                let text_y = row_y + (RECIPIENT_ROW_HEIGHT - 19) / 2;

                // Index "#1"
                let mut index_text = Text::new(
                    format!("#{}", item.index() + 1),
                    Gray4TextStyle::new(&NOTO_SANS_18_MEDIUM, PALETTE.primary),
                );
                index_text.draw(
                    &mut target.clone().translate(Point::new(12, text_y)),
                    current_time,
                )?;

                // Amount
                let mut amount_text = Text::new(
                    format!("{}", item.amount()),
                    Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.on_background),
                );
                let amount_width = amount_text.sizing().width as i32;
                let amount_x = screen_width - 28 - amount_width;
                amount_text.draw(
                    &mut target.clone().translate(Point::new(amount_x, text_y)),
                    current_time,
                )?;

                // Right chevron ">"
                let mut chevron_text = Text::new(
                    ">".to_string(),
                    Gray4TextStyle::new(&NOTO_SANS_17_REGULAR, PALETTE.text_secondary),
                );
                chevron_text.draw(
                    &mut target
                        .clone()
                        .translate(Point::new(screen_width - 20, text_y)),
                    current_time,
                )?;
            }

            // The clear above wiped the scrollbar, so it has to redraw too.
            self.scroll_bar.force_full_redraw();
            self.list_dirty = false;
        }

        // Scrollbar
        let scrollbar_x = self.size.width as i32 - SCROLLBAR_WIDTH as i32 - 2;
        let scrollbar_area = Rectangle::new(
            Point::new(scrollbar_x, 4),
            Size::new(SCROLLBAR_WIDTH, self.size.height.saturating_sub(8)),
        );
        let mut scrollbar_target = target.clone().crop(scrollbar_area);
        self.scroll_bar.draw(&mut scrollbar_target, current_time)?;

        Ok(())
    }
}
