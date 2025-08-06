use crate::{Widget, Instant, widget_tuple::WidgetTuple};
use alloc::vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{PixelColor, Rgb565},
    prelude::*,
    primitives::Rectangle,
};

// Helper macro to count arguments
macro_rules! count_args {
    () => (0usize);
    ($head:ident) => (1usize);
    ($head:ident, $($tail:ident),*) => (1usize + count_args!($($tail),*));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    /// Align children to the start (left) of the cross axis
    Start,
    /// Center children along the cross axis
    Center,
}

/// Defines how children are distributed along the main (vertical) axis of a Column
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisAlignment {
    /// Place children at the start (top) of the column with no spacing between them
    Start,
    /// Center children vertically in the column with no spacing between them
    Center,
    /// Place children at the end (bottom) of the column with no spacing between them
    End,
    /// Place children with equal spacing between them, with no space before the first or after the last child
    /// Example with 3 children: [Child1]--space--[Child2]--space--[Child3]
    SpaceBetween,
    /// Place children with equal spacing around them, with half spacing before the first and after the last child
    /// Example with 3 children: -half-[Child1]-full-[Child2]-full-[Child3]-half-
    SpaceAround,
    /// Place children with equal spacing between and around them
    /// Example with 3 children: --space--[Child1]--space--[Child2]--space--[Child3]--space--
    SpaceEvenly,
}

/// A column widget that arranges its children vertically
/// 
/// The Column widget takes a tuple of child widgets and arranges them vertically.
/// You can control the distribution of children along the vertical axis using `MainAxisAlignment`
/// and their horizontal alignment using `CrossAxisAlignment`.
/// 
/// # Example
/// ```ignore
/// let column = Column::new((widget1, widget2, widget3))
///     .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
///     .with_cross_axis_alignment(CrossAxisAlignment::Center);
/// ```
pub struct Column<T, C = Rgb565> {
    pub children: T,
    pub cross_axis_alignment: CrossAxisAlignment,
    pub main_axis_alignment: MainAxisAlignment,
    child_rects: alloc::vec::Vec<Rectangle>,
    _phantom: core::marker::PhantomData<C>,
}

impl<T: WidgetTuple, C> Column<T, C> {
    pub fn new(children: T) -> Self {
        let mut column = Self {
            children,
            cross_axis_alignment: CrossAxisAlignment::Center,
            main_axis_alignment: MainAxisAlignment::Start,
            child_rects: vec![Rectangle::zero(); T::TUPLE_LEN],
            _phantom: core::marker::PhantomData,
        };
        
        // Extract and cache sizes
        let sizes = column.children.extract_sizes();
        
        // Initialize rectangles with sizes (zero for unsized children)
        for (i, size) in sizes.into_iter().enumerate() {
            column.child_rects[i].size = size;
            // Position will be set during draw
        }
        
        column
    }
    
    pub fn with_cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }
    
    pub fn with_main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }
}

// Macro to implement Widget for Column with tuples of different sizes
// Macro to implement Widget for Column with tuples of different sizes
macro_rules! impl_column_for_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: Widget<Color = C>),+, C: PixelColor> crate::DynWidget for Column<($($t,)+), C> {
            #[allow(unused_assignments)]
            fn handle_touch(
                &mut self,
                point: Point,
                current_time: Instant,
                is_release: bool,
            ) -> Option<crate::KeyTouch> {
                // Use cached rectangles
                if !self.child_rects.is_empty() {
                    #[allow(non_snake_case)]
                    let ($(ref mut $t,)+) = self.children;
                    
                    let mut child_index = 0;
                    $(
                        {
                            let area = self.child_rects[child_index];
                            if area.contains(point) {
                                let relative_point = Point::new(
                                    point.x - area.top_left.x,
                                    point.y - area.top_left.y
                                );
                                return $t.handle_touch(relative_point, current_time, is_release);
                            }
                            child_index += 1;
                        }
                    )+
                }
                
                None
            }
            
            fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
                // For now, pass drag to all children - could be improved to only send to relevant child
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;
                
                $(
                    $t.handle_vertical_drag(start_y, current_y, _is_release);
                )+
            }
            
            fn size_hint(&self) -> Option<Size> {
                #[allow(non_snake_case)]
                let ($(ref $t,)+) = self.children;

                // Calculate total height and maximum width
                let mut total_height = 0;
                let mut max_width = 0;

                // All children
                $(
                    let size = $t.size_hint()?;
                    total_height += size.height;
                    max_width = max_width.max(size.width);
                )+
                match self.main_axis_alignment {
                    MainAxisAlignment::Start => {

                        Some(Size::new(max_width, total_height))
                    }
                    _ => Some(Size::new(max_width, 0)),
                }
            }
            
            fn force_full_redraw(&mut self) {
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;
                
                $(
                    $t.force_full_redraw();
                )+
            }
        }
        
        impl<$($t: Widget<Color = C>),+, C: PixelColor> Widget for Column<($($t,)+), C> {
            type Color = C;
            
            #[allow(unused_assignments)]
            fn draw<D: DrawTarget<Color = C>>(
                &mut self,
                target: &mut D,
                current_time: Instant,
            ) -> Result<(), D::Error> {
                // Count the number of children at compile time
                const NUM_CHILDREN: usize = count_args!($($t),+);
                
                
                // Get mutable references to children
                #[allow(non_snake_case, unused_variables)]
                let ($(ref mut $t,)+) = self.children;
                
                let target_bounds = target.bounding_box();
                let target_height = target_bounds.size.height;

                // Find first unsized child and calculate total height of sized children
                let mut first_unsized_index = None;
                let mut total_sized_height = 0u32;
                for (i, rect) in self.child_rects.iter().enumerate() {
                    if rect.size.height == 0 && first_unsized_index.is_none() {
                        first_unsized_index = Some(i);
                    } else {
                        total_sized_height += rect.size.height;
                    }
                }
                
                // Calculate available space for unsized child
                let available_space_for_unsized = if first_unsized_index.is_some() {
                    target_height.saturating_sub(total_sized_height)
                } else {
                    0
                };
                
                // If we have an unsized child, assign it the available space
                if let Some(unsized_idx) = first_unsized_index {
                    self.child_rects[unsized_idx].size.height = available_space_for_unsized;
                }
                
                // Recalculate total height now that unsized child has a size
                let total_height: u32 = self.child_rects.iter().map(|r| r.size.height).sum();
                
                // Calculate initial y_offset and spacing based on MainAxisAlignment
                let (mut y_offset, spacing) = match self.main_axis_alignment {
                    MainAxisAlignment::Start => (0i32, 0i32),
                    MainAxisAlignment::Center => {
                        let offset = (target_height as i32 - total_height as i32) / 2;
                        (offset.max(0), 0)
                    }
                    MainAxisAlignment::End => {
                        let offset = target_height as i32 - total_height as i32;
                        (offset.max(0), 0)
                    }
                    MainAxisAlignment::SpaceBetween => {
                        if NUM_CHILDREN > 1usize {
                            let available_space = target_height as i32 - total_height as i32;
                            (0, available_space / (NUM_CHILDREN as i32 - 1))
                        } else {
                            (0, 0)
                        }
                    }
                    MainAxisAlignment::SpaceAround => {
                        let available_space = target_height as i32 - total_height as i32;
                        let spacing = available_space / (NUM_CHILDREN as i32);
                        (spacing / 2, spacing)
                    }
                    MainAxisAlignment::SpaceEvenly => {
                        let available_space = target_height as i32 - total_height as i32;
                        let spacing = available_space / (NUM_CHILDREN as i32 + 1);
                        (spacing, spacing)
                    }
                };
                
                // Get mutable references to children
                #[allow(non_snake_case, unused_variables)]
                let ($(ref mut $t,)+) = self.children;
                
                // Update positions and draw
                let mut child_index = 0;
                $(
                    {
                        let size = self.child_rects[child_index].size;
                        let x_offset = match self.cross_axis_alignment {
                            CrossAxisAlignment::Start => 0,
                            CrossAxisAlignment::Center => {
                                let target_width = target_bounds.size.width as i32;
                                (target_width - size.width as i32) / 2
                            }
                        };
                        self.child_rects[child_index].top_left = Point::new(x_offset, y_offset);
                        let mut cropped = target.cropped(&self.child_rects[child_index]);
                        $t.draw(&mut cropped, current_time)?;
                        y_offset += size.height as i32 + spacing;
                        child_index += 1;
                    }
                )+
                
                Ok(())
            }
        }
    };
}

// Generate implementations for tuples up to 9 elements
impl_column_for_tuple!(1, T1);
impl_column_for_tuple!(2, T1, T2);
impl_column_for_tuple!(3, T1, T2, T3);
impl_column_for_tuple!(4, T1, T2, T3, T4);
impl_column_for_tuple!(5, T1, T2, T3, T4, T5);
impl_column_for_tuple!(6, T1, T2, T3, T4, T5, T6);
impl_column_for_tuple!(7, T1, T2, T3, T4, T5, T6, T7);
impl_column_for_tuple!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_column_for_tuple!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
