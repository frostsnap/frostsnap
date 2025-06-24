use crate::{Widget, Instant};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{PixelColor, Rgb565},
    prelude::*,
    primitives::Rectangle,
};

/// A column widget that arranges its children vertically
pub struct Column<T, C = Rgb565> {
    pub children: T,
    _phantom: core::marker::PhantomData<C>,
}

impl<T, C> Column<T, C> {
    pub fn new(children: T) -> Self {
        Self { 
            children,
            _phantom: core::marker::PhantomData,
        }
    }
}

// Macro to implement Widget for Column with tuples of different sizes
macro_rules! impl_column_for_tuple {
    // Base case for a single element
    ($t1:ident) => {
        impl<$t1: Widget<Color = C>, C: PixelColor> Widget for Column<($t1,), C> {
            type Color = C;
            
            fn draw<D: DrawTarget<Color = C>>(
                &mut self,
                target: &mut D,
                current_time: Instant,
            ) -> Result<(), D::Error> {
                let y_offset = 0;
                
                let size = self.children.0.size_hint()
                    .expect("Column requires all children to have size_hint");
                let area = Rectangle::new(Point::new(0, y_offset), size);
                let mut cropped = target.cropped(&area);
                self.children.0.draw(&mut cropped, current_time)?;
                
                Ok(())
            }
            
            fn handle_touch(
                &mut self,
                point: Point,
                current_time: Instant,
                is_release: bool,
            ) -> Option<crate::KeyTouch> {
                let y_offset = 0;
                
                let size = self.children.0.size_hint()
                    .expect("Column requires all children to have size_hint");
                let area = Rectangle::new(Point::new(0, y_offset), size);
                
                if area.contains(point) {
                    let relative_point = Point::new(point.x, point.y - y_offset);
                    return self.children.0.handle_touch(relative_point, current_time, is_release);
                }
                
                None
            }
            
            fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
                // For now, pass drag to all children - could be improved
                self.children.0.handle_vertical_drag(start_y, current_y, _is_release);
            }
            
            fn size_hint(&self) -> Option<Size> {
                let size = self.children.0.size_hint()?;
                Some(Size::new(size.width, size.height))
            }
        }
    };
    
    // Recursive case for multiple elements
    ($t1:ident, $($tn:ident),+) => {
        impl<$t1: Widget<Color = C>, $($tn: Widget<Color = C>),+, C: PixelColor> Widget for Column<($t1, $($tn,)+), C> {
            type Color = C;
            
            #[allow(unused_assignments)]
            fn draw<D: DrawTarget<Color = C>>(
                &mut self,
                target: &mut D,
                current_time: Instant,
            ) -> Result<(), D::Error> {
                let mut y_offset = 0;
                
                // Destructure the tuple
                #[allow(non_snake_case)]
                let (ref mut $t1, $(ref mut $tn,)+) = self.children;
                
                // Draw first child
                {
                    let size = $t1.size_hint()
                        .expect("Column requires all children to have size_hint");
                    let area = Rectangle::new(Point::new(0, y_offset), size);
                    let mut cropped = target.cropped(&area);
                    $t1.draw(&mut cropped, current_time)?;
                    y_offset += size.height as i32;
                }
                
                // Draw remaining children
                $(
                    {
                        let size = $tn.size_hint()
                            .expect("Column requires all children to have size_hint");
                        let area = Rectangle::new(Point::new(0, y_offset), size);
                        let mut cropped = target.cropped(&area);
                        $tn.draw(&mut cropped, current_time)?;
                        y_offset += size.height as i32;
                    }
                )+
                
                Ok(())
            }
            
            #[allow(unused_assignments)]
            fn handle_touch(
                &mut self,
                point: Point,
                current_time: Instant,
                is_release: bool,
            ) -> Option<crate::KeyTouch> {
                let mut y_offset = 0;
                
                // Destructure the tuple
                #[allow(non_snake_case)]
                let (ref mut $t1, $(ref mut $tn,)+) = self.children;
                
                // Check first child
                {
                    let size = $t1.size_hint()
                        .expect("Column requires all children to have size_hint");
                    let area = Rectangle::new(Point::new(0, y_offset), size);
                    
                    if area.contains(point) {
                        let relative_point = Point::new(point.x, point.y - y_offset);
                        return $t1.handle_touch(relative_point, current_time, is_release);
                    }
                    y_offset += size.height as i32;
                }
                
                // Check remaining children
                $(
                    {
                        let size = $tn.size_hint()
                            .expect("Column requires all children to have size_hint");
                        let area = Rectangle::new(Point::new(0, y_offset), size);
                        
                        if area.contains(point) {
                            let relative_point = Point::new(point.x, point.y - y_offset);
                            return $tn.handle_touch(relative_point, current_time, is_release);
                        }
                        y_offset += size.height as i32;
                    }
                )+
                
                None
            }
            
            fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
                // For now, pass drag to all children - could be improved to only send to relevant child
                #[allow(non_snake_case)]
                let (ref mut $t1, $(ref mut $tn,)+) = self.children;
                
                $t1.handle_vertical_drag(start_y, current_y, _is_release);
                $(
                    $tn.handle_vertical_drag(start_y, current_y, _is_release);
                )+
            }
            
            fn size_hint(&self) -> Option<Size> {
                #[allow(non_snake_case)]
                let (ref $t1, $(ref $tn,)+) = self.children;
                
                // Calculate total height and maximum width
                let mut total_height = 0;
                let mut max_width = 0;
                
                // First child
                let size = $t1.size_hint()?;
                total_height += size.height;
                max_width = max_width.max(size.width);
                
                // Remaining children
                $(
                    let size = $tn.size_hint()?;
                    total_height += size.height;
                    max_width = max_width.max(size.width);
                )+
                
                Some(Size::new(max_width, total_height))
            }
        }
    };
}

// Generate implementations for tuples up to 9 elements
impl_column_for_tuple!(T1);
impl_column_for_tuple!(T1, T2);
impl_column_for_tuple!(T1, T2, T3);
impl_column_for_tuple!(T1, T2, T3, T4);
impl_column_for_tuple!(T1, T2, T3, T4, T5);
impl_column_for_tuple!(T1, T2, T3, T4, T5, T6);
impl_column_for_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_column_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_column_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9);