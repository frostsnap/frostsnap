use crate::{palette::PALETTE, DynWidget, Fader, PageByPage, Rat, ScrollBar, SwipeUpChevron, Widget, SCROLLBAR_WIDTH};
use crate::prelude::FreeCrop;
use embedded_graphics::{
    draw_target::DrawTarget, geometry::AnchorX, pixelcolor::Rgb565, prelude::*, primitives::Rectangle
};

const SCROLLBAR_PADDING: u32 = 2;
const SCROLLBAR_TOP_OFFSET: u32 = 35;  // Account for rounded top edge
const SCROLLBAR_BOTTOM_OFFSET: u32 = 35;  // Account for rounded bottom edge
const FADE_DURATION_MS: u64 = 400;
const FADE_REDRAW_INTERVAL_MS: u64 = 40;

/// A widget that wraps a PageByPage widget and adds a scroll bar on the right side
pub struct PaginatorWithScrollBar<W, F> {
    pub child: Fader<W>,
    pub final_page: Fader<F>,
    scrollbar: Fader<ScrollBar>,
    swipe_hint: Option<Fader<SwipeUpChevron<Rgb565>>>,
    child_was_ready: bool,
    drag_start: Option<u32>,
    showing_virtual_page: bool,
}

impl<W: PageByPage<Color=Rgb565>, F: Widget<Color = Rgb565>> PaginatorWithScrollBar<W, F> {
    pub fn new(child: W, final_page: F) -> Self {
        let total_pages_with_virtual = child.total_pages() + 1; // +1 for virtual page
        // Calculate thumb size based on pages
        let thumb_size = crate::Frac::from_ratio(1, total_pages_with_virtual as u32);
        let scrollbar = ScrollBar::new(thumb_size);
        
        // Create swipe hint if on first page with navigation
        let swipe_hint = if  child.has_next_page() {
            Some(Fader::new_faded_out(SwipeUpChevron::new(PALETTE.on_surface_variant, PALETTE.background)))
        } else {
            None
        };

        let mut self_ = Self {
            child: Fader::new_faded_out(child),
            final_page: Fader::new_faded_out(final_page),
            scrollbar: Fader::new_faded_out(scrollbar), 
            swipe_hint, 
            child_was_ready: false,
            drag_start: None,
            showing_virtual_page: false,
        };

        self_.set_scroll_position();
        self_
    }

    fn set_scroll_position(&mut self) {
        let current_page = self.child.child.current_page() as u32 + self.showing_virtual_page as u32;
        let position = Rat::from_ratio(current_page, self.child.child.total_pages() as u32);
        self.scrollbar.child.set_scroll_position(position);
    }
}

impl<W, F> crate::DynWidget for PaginatorWithScrollBar<W, F>
where
    W: PageByPage<Color = Rgb565>,
    F: Widget<Color = Rgb565>,
{
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        if self.showing_virtual_page {
            self.final_page.handle_touch(point, current_time, is_release)
        } else {
            self.child.handle_touch(point, current_time, is_release)
        }
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if is_release {
            if let Some(drag_start) = self.drag_start.take() {
                // Determine swipe direction based on drag distance
                if new_y > drag_start {
                    // Swiping down
                    if self.showing_virtual_page {
                        // Go back to last child page
                        self.showing_virtual_page = false;
                        self.final_page.start_fade(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
                    } else if self.child.has_prev_page() {
                        self.child.prev_page();
                    }
                } else if drag_start > new_y {
                    if self.child.has_next_page() {
                        self.child.next_page();
                    } else if !self.showing_virtual_page {
                        // Navigate to virtual page
                        self.showing_virtual_page = true;
                        self.child.start_fade(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
                        self.scrollbar.start_fade(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
                    }
                }
            }
            self.set_scroll_position();
        } else {
            // Start of drag
            if self.drag_start.is_none() {
                self.drag_start = Some(new_y);
            }
        }
    }

    fn size_hint(&self) -> Option<Size> {
        // Get child's size hint
        let child_size = self.child.size_hint()?;
        
        // Add swipe hint height if present
        let swipe_hint_height = if let Some(swipe_hint) = &self.swipe_hint {
            swipe_hint.size_hint().unwrap().height
        } else {
            0
        };
        
        Some(Size::new(child_size.width, child_size.height + swipe_hint_height))
    }

    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw();
        self.final_page.force_full_redraw();
        self.scrollbar.force_full_redraw();
    }
}

impl<W, F> Widget for PaginatorWithScrollBar<W, F>
where
    W: PageByPage<Color = Rgb565>,
    F: Widget<Color = Rgb565>,
{
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        let bounds = target.bounding_box();
        
        // Get swipe hint height if present
        let swipe_hint_height = if let Some(swipe_hint) = &self.swipe_hint {
            swipe_hint.size_hint().unwrap().height
        } else {
            0
        };
        
        // Calculate content area within child area (excluding scrollbar)
        let content_width = bounds.size.width - SCROLLBAR_WIDTH - SCROLLBAR_PADDING;
        let content_area = bounds.resized_width(content_width, AnchorX::Left);

        // Handle initial state or transitions
        if self.child.is_faded_out() && self.final_page.is_faded_out() {
            if self.showing_virtual_page {
                self.final_page.start_fade_in(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
            } else {
                self.child.start_fade_in(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
                self.scrollbar.start_fade_in(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
            }
        }

        // Draw child/final page in content area (cropped to child_area)
        let mut child_target = target.free_cropped(&content_area);
        let mut child_target = child_target.clipped(&content_area); // to make sure we don't draw over scrollbar
        self.child.draw(&mut child_target, current_time)?;
        self.final_page.draw(target, current_time)?;
        
        // Draw scrollbar in its area (full height of widget, not affected by swipe hint)
        let scrollbar_x = bounds.top_left.x + (bounds.size.width - SCROLLBAR_WIDTH) as i32;
        let scrollbar_height = bounds.size.height - SCROLLBAR_TOP_OFFSET - SCROLLBAR_BOTTOM_OFFSET;
        let scrollbar_area = Rectangle::new(
            Point::new(scrollbar_x, bounds.top_left.y + SCROLLBAR_TOP_OFFSET as i32),
            Size::new(SCROLLBAR_WIDTH, scrollbar_height)
        );
        self.scrollbar.draw(&mut target.free_cropped(&scrollbar_area), current_time)?;

        let child_is_ready = !self.child.is_transitioning() && self.child.is_not_faded();

        if let Some(swipe_hint) = &mut self.swipe_hint {
            if !child_is_ready && self.child_was_ready {
                swipe_hint.instant_fade(PALETTE.background);
            } else if child_is_ready && !self.child_was_ready {
                swipe_hint.start_fade_in(FADE_DURATION_MS, FADE_REDRAW_INTERVAL_MS, PALETTE.background);
            }

            // Draw swipe hint at the bottom
            let hint_area = Rectangle::new(
                Point::new(0, (bounds.size.height - swipe_hint_height) as i32),
                Size::new(bounds.size.width, swipe_hint_height)
            );
            swipe_hint.draw(&mut target.free_cropped(&hint_area), current_time)?;
        }

        self.child_was_ready = child_is_ready;
        
        Ok(())
    }
    
}

