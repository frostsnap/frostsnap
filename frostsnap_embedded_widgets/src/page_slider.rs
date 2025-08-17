use crate::super_draw_target::SuperDrawTarget;
use crate::{
    palette::PALETTE, DynWidget, Fader, Instant, SlideInTransition, Stack, StackAlignment,
    SwipeUpChevron, Widget, WidgetList,
};
use alloc::boxed::Box;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
};

const ANIMATION_DURATION_MS: u64 = 500;
const MIN_SWIPE_DISTANCE: u32 = 0;

// Type aliases to reduce complexity
type PageStack<T> = Stack<(SlideInTransition<T>, Option<Fader<SwipeUpChevron<Rgb565>>>)>;
type PageReadyCallback<T> = Box<dyn FnMut(&mut T)>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Up,
    Down,
}

/// A page slider that uses SlideInTransition for smooth page transitions
pub struct PageSlider<L, T>
where
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>,
{
    list: L,
    current_index: usize,
    stack: PageStack<T>,
    drag_start: Option<u32>,
    height: u32,
    on_page_ready: Option<PageReadyCallback<T>>,
    page_ready_triggered: bool,
    screen_size: Option<Size>,
}

impl<L, T> PageSlider<L, T>
where
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>,
{
    pub fn new(list: L, height: u32) -> Self {
        // Get the initial widget (index 0)
        let initial_widget = list
            .get(0)
            .expect("PageSlider requires at least one widget in the list");

        let transition = SlideInTransition::new(
            initial_widget,
            ANIMATION_DURATION_MS,
            Point::new(0, 0), // Start at rest position for initial widget
            PALETTE.background,
        );

        // Build stack with transition and optional chevron aligned at bottom center
        let stack = Stack::builder().push(transition).push_aligned(
            None::<Fader<SwipeUpChevron<Rgb565>>>,
            StackAlignment::BottomCenter,
        );

        Self {
            list,
            current_index: 0,
            stack,
            drag_start: None,
            height,
            on_page_ready: None,
            page_ready_triggered: false,
            screen_size: None,
        }
    }

    /// Builder method to set a callback that's called when a page is ready (animation complete)
    pub fn with_on_page_ready<F>(mut self, callback: F) -> Self
    where
        F: FnMut(&mut T) + 'static,
    {
        self.on_page_ready = Some(Box::new(callback));
        self
    }

    /// Builder method to enable swipe up chevron indicator
    pub fn with_swipe_up_chevron(mut self) -> Self {
        // Create chevron
        let chevron = SwipeUpChevron::new(PALETTE.on_surface, PALETTE.background);
        let fader = Fader::new_faded_out(chevron);

        // Set the chevron in the stack (it's already positioned with BottomCenter alignment)
        self.stack.children.1 = Some(fader);
        self
    }

    pub fn current_index(&self) -> usize {
        self.current_index
    }

    pub fn total_pages(&self) -> usize {
        self.list.len()
    }

    pub fn has_next(&self) -> bool {
        self.current_index + 1 < self.list.len()
    }

    pub fn has_prev(&self) -> bool {
        self.current_index > 0
    }

    /// Get a reference to the current widget
    pub fn current_widget(&mut self) -> &mut T {
        self.stack.children.0.current_widget_mut()
    }

    pub fn start_transition(&mut self, direction: Direction) {
        // First check if navigation is allowed based on the current widget
        let current_widget = self.stack.children.0.current_widget_mut();
        let allowed = match direction {
            Direction::Up => self.list.can_go_next(self.current_index, current_widget),
            Direction::Down => self.list.can_go_prev(self.current_index, current_widget),
        };

        if !allowed {
            return; // Navigation blocked by the widget list
        }

        // Instantly fade out the chevron when starting a transition
        if let Some(ref mut chevron) = &mut self.stack.children.1 {
            chevron.instant_fade(PALETTE.background);
        }

        // Calculate target index
        let target_index = match direction {
            Direction::Up => {
                if self.has_next() {
                    self.current_index + 1
                } else {
                    return; // Can't go forward
                }
            }
            Direction::Down => {
                if self.has_prev() {
                    self.current_index - 1
                } else {
                    return; // Can't go back
                }
            }
        };

        // Get the new widget
        if let Some(new_widget) = self.list.get(target_index) {
            // Set slide direction based on height
            let height = self.height as i32;
            let slide_from = match direction {
                Direction::Up => Point::new(0, height), // Slide from bottom
                Direction::Down => Point::new(0, -height), // Slide from top
            };

            // Update the slide-from position and switch to the new widget
            let transition = &mut self.stack.children.0;
            transition.set_slide_from(slide_from);
            transition.switch_to(new_widget);

            self.current_index = target_index;
            // Reset the ready flag for the new page
            self.page_ready_triggered = false;
        }
    }
}

impl<L, T> DynWidget for PageSlider<L, T>
where
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.screen_size = Some(max_size);
        // Just propagate to the stack - it handles all positioning
        self.stack.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.stack.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Pass through to stack
        self.stack.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if is_release {
            if let Some(drag_start) = self.drag_start.take() {
                // Determine swipe direction based on drag distance
                if new_y > drag_start + MIN_SWIPE_DISTANCE {
                    // Swiped down - go to previous page
                    self.start_transition(Direction::Down);
                } else if drag_start > new_y + MIN_SWIPE_DISTANCE {
                    // Swiped up - go to next page
                    self.start_transition(Direction::Up);
                }
            }
        } else {
            // Start tracking drag
            if self.drag_start.is_none() {
                self.drag_start = Some(new_y);
            }
        }
    }

    fn force_full_redraw(&mut self) {
        self.stack.force_full_redraw();
    }
}

impl<L, T> Widget for PageSlider<L, T>
where
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Check if transition is complete and trigger callback if not already triggered
        if !self.page_ready_triggered && self.stack.children.0.is_transition_complete() {
            self.page_ready_triggered = true;

            // Call the on_page_ready callback if set
            if let Some(ref mut callback) = self.on_page_ready {
                // Get mutable access to the current widget
                let current_widget = self.stack.children.0.current_widget_mut();
                callback(current_widget);
            }

            // Fade in the swipe chevron if present
            if let Some(ref mut chevron) = &mut self.stack.children.1 {
                let current_widget = self.stack.children.0.current_widget_mut();

                if self.list.can_go_next(self.current_index, current_widget) {
                    chevron.start_fade_in(400, 20, PALETTE.background);
                }
            }
        }

        // Draw the stack (it handles drawing both transition and chevron overlay)
        self.stack.draw(target, current_time)
    }
}
