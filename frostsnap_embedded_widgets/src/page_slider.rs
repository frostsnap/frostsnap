use crate::{Widget, WidgetList, SlideInTransition, Instant, palette::PALETTE, DynWidget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
};

const ANIMATION_DURATION_MS: u64 = 1_500;
const MIN_SWIPE_DISTANCE: u32 = 30;

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
    transition: SlideInTransition<T>,
    drag_start: Option<u32>,
    height: u32,
}

impl<L, T> PageSlider<L, T>
where 
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>,
{
    pub fn new(list: L, height: u32) -> Self {
        // Get the initial widget (index 0)
        let initial_widget = list.get(0)
            .expect("PageSlider requires at least one widget in the list");
        
        let transition = SlideInTransition::new(
            initial_widget,
            ANIMATION_DURATION_MS,
            Point::new(0, 0), // Start at rest position for initial widget
            PALETTE.background,
        );
        
        Self {
            list,
            current_index: 0,
            transition,
            drag_start: None,
            height,
        }
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
    
    pub fn start_transition(&mut self, direction: Direction) {
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
                Direction::Up => Point::new(0, height),  // Slide from bottom
                Direction::Down => Point::new(0, -height), // Slide from top
            };
            
            // Update the slide-from position and switch to the new widget
            self.transition.set_slide_from(slide_from);
            self.transition.switch_to(new_widget);
            
            self.current_index = target_index;
        }
    }
}

impl<L, T> DynWidget for PageSlider<L, T>
where 
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>
{
    fn set_constraints(&mut self, max_size: Size) {
        self.transition.set_constraints(max_size);
    }
    
    fn sizing(&self) -> crate::Sizing {
        self.transition.sizing()
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Pass through to transition
        self.transition.handle_touch(point, current_time, is_release)
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
    
    fn size_hint(&self) -> Option<Size> {
        // Use the size of the child widget in the transition
        self.transition.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.transition.force_full_redraw();
    }
}

impl<L, T> Widget for PageSlider<L, T>
where 
    L: WidgetList<T>,
    T: Widget<Color = Rgb565>
{
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Draw the transition
        self.transition.draw(target, current_time)?;
        
        Ok(())
    }
}
