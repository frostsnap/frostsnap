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

/// Alignment options specific to Row widget  
pub use crate::column::{MainAxisAlignment, CrossAxisAlignment};

/// A row widget that arranges its children horizontally
pub struct Row<T, C = Rgb565> {
    pub children: T,
    pub cross_axis_alignment: crate::column::CrossAxisAlignment,
    pub main_axis_alignment: crate::column::MainAxisAlignment,
    child_rects: alloc::vec::Vec<Rectangle>,
    _phantom: core::marker::PhantomData<C>,
}

impl<T: WidgetTuple, C> Row<T, C> {
    pub fn new(children: T) -> Self {
        let mut row = Self {
            children,
            cross_axis_alignment: crate::column::CrossAxisAlignment::Center,
            main_axis_alignment: crate::column::MainAxisAlignment::Start,
            child_rects: vec![Rectangle::zero(); T::TUPLE_LEN],
            _phantom: core::marker::PhantomData,
        };
        
        // Extract and cache sizes
        let sizes = row.children.extract_sizes();
        
        // Initialize rectangles with sizes (zero for unsized children)
        for (i, size) in sizes.into_iter().enumerate() {
            row.child_rects[i].size = size;
            // Position will be set during draw
        }
        
        row
    }
    
    pub fn with_cross_axis_alignment(mut self, alignment: crate::column::CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }
    
    pub fn with_main_axis_alignment(mut self, alignment: crate::column::MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }
}

// Macro to implement Widget for Row with tuples of different sizes
macro_rules! impl_row_for_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: Widget<Color = C>),+, C: PixelColor> crate::DynWidget for Row<($($t,)+), C> {
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
                // Only Start alignment can provide a size hint
                // Other alignments depend on the parent's width for spacing calculations
                match self.main_axis_alignment {
                    crate::column::MainAxisAlignment::Start => {
                        #[allow(non_snake_case)]
                        let ($(ref $t,)+) = self.children;
                        
                        // Calculate total width and maximum height
                        let mut total_width = 0;
                        let mut max_height = 0;
                        
                        // All children
                        $(
                            let size = $t.size_hint()?;
                            total_width += size.width;
                            max_height = max_height.max(size.height);
                        )+
                        
                        Some(Size::new(total_width, max_height))
                    }
                    _ => None, // Other alignments need parent width to calculate layout
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
        
        impl<$($t: Widget<Color = C>),+, C: PixelColor> Widget for Row<($($t,)+), C> {
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
                let target_width = target_bounds.size.width;
                
                // Find first unsized child and calculate total width of sized children
                let mut first_unsized_index = None;
                let mut total_sized_width = 0u32;
                for (i, rect) in self.child_rects.iter().enumerate() {
                    if rect.size == Size::zero() && first_unsized_index.is_none() {
                        first_unsized_index = Some(i);
                    } else {
                        total_sized_width += rect.size.width;
                    }
                }
                
                // Calculate available space for unsized child
                let available_space_for_unsized = if first_unsized_index.is_some() {
                    target_width.saturating_sub(total_sized_width)
                } else {
                    0
                };
                
                // If we have an unsized child, assign it the available space
                if let Some(unsized_idx) = first_unsized_index {
                    self.child_rects[unsized_idx].size.width = available_space_for_unsized;
                    self.child_rects[unsized_idx].size.height = target_bounds.size.height; // Full height
                }
                
                // Recalculate total width now that unsized child has a size
                let total_width: u32 = self.child_rects.iter().map(|r| r.size.width).sum();
                
                // Calculate initial x_offset and spacing based on MainAxisAlignment
                let (mut x_offset, spacing) = match self.main_axis_alignment {
                    crate::column::MainAxisAlignment::Start => (0i32, 0i32),
                    crate::column::MainAxisAlignment::Center => {
                        let offset = (target_width as i32 - total_width as i32) / 2;
                        (offset.max(0), 0)
                    }
                    crate::column::MainAxisAlignment::End => {
                        let offset = target_width as i32 - total_width as i32;
                        (offset.max(0), 0)
                    }
                    crate::column::MainAxisAlignment::SpaceBetween => {
                        if NUM_CHILDREN > 1usize {
                            let available_space = target_width as i32 - total_width as i32;
                            (0, available_space / (NUM_CHILDREN as i32 - 1))
                        } else {
                            (0, 0)
                        }
                    }
                    crate::column::MainAxisAlignment::SpaceAround => {
                        let available_space = target_width as i32 - total_width as i32;
                        let spacing = available_space / (NUM_CHILDREN as i32);
                        (spacing / 2, spacing)
                    }
                    crate::column::MainAxisAlignment::SpaceEvenly => {
                        let available_space = target_width as i32 - total_width as i32;
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
                        let y_offset = match self.cross_axis_alignment {
                            crate::column::CrossAxisAlignment::Start => 0,
                            crate::column::CrossAxisAlignment::Center => {
                                let target_height = target_bounds.size.height as i32;
                                (target_height - size.height as i32) / 2
                            }
                        };
                        self.child_rects[child_index].top_left = Point::new(x_offset, y_offset);
                        let mut cropped = target.cropped(&self.child_rects[child_index]);
                        $t.draw(&mut cropped, current_time)?;
                        x_offset += size.width as i32 + spacing;
                        child_index += 1;
                    }
                )+
                
                Ok(())
            }
        }
    };
}

// Generate implementations for tuples up to 12 elements
impl_row_for_tuple!(1, T1);
impl_row_for_tuple!(2, T1, T2);
impl_row_for_tuple!(3, T1, T2, T3);
impl_row_for_tuple!(4, T1, T2, T3, T4);
impl_row_for_tuple!(5, T1, T2, T3, T4, T5);
impl_row_for_tuple!(6, T1, T2, T3, T4, T5, T6);
impl_row_for_tuple!(7, T1, T2, T3, T4, T5, T6, T7);
impl_row_for_tuple!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_row_for_tuple!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_row_for_tuple!(10, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_row_for_tuple!(11, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_row_for_tuple!(12, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
