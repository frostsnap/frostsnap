use super::{CrossAxisAlignment, MainAxisAlignment};
use crate::super_draw_target::SuperDrawTarget;
use crate::{widget_tuple::AssociatedArray, DynWidget, Instant, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
    primitives::Rectangle,
};

/// Implementation of AssociatedArray for Vec<W> to enable dynamic collections
impl<W: DynWidget> AssociatedArray for Vec<W> {
    type Array<T> = Vec<T>;

    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
        vec![value; self.len()]
    }
    
    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn DynWidget> {
        self.get_mut(index).map(|w| w as &mut dyn DynWidget)
    }
    
    fn len(&self) -> usize {
        self.len()
    }
}

/// Implementation of AssociatedArray for fixed-size arrays
impl<W: DynWidget, const N: usize> AssociatedArray for [W; N] {
    type Array<T> = [T; N];

    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T> {
        [value; N]
    }
    
    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn DynWidget> {
        self.get_mut(index).map(|w| w as &mut dyn DynWidget)
    }
    
    fn len(&self) -> usize {
        N
    }
}


// Generic implementation for any type with AssociatedArray
impl<T> crate::DynWidget for super::Column<T>
where
    T: AssociatedArray
{
    fn set_constraints(&mut self, max_size: Size) {
        let len = self.children.len();
        
        if len == 0 {
            self.sizing = Some(crate::Sizing { width: 0, height: 0 });
            return;
        }

        let mut remaining_height = max_size.height;
        let mut max_child_width = 0u32;

        // Account for all spacing in remaining height
        let total_spacing: u32 = self.spacing_before.as_ref().iter().sum();
        remaining_height = remaining_height.saturating_sub(total_spacing);
        
        // Get flex scores and calculate total flex
        let flex_scores = self.flex_scores.as_ref();
        let total_flex: u32 = flex_scores.iter().sum();

        // First pass: set constraints on non-flex children
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                if flex_scores[i] == 0 {
                    // Set constraints on non-flex child with remaining available height
                    child.set_constraints(Size::new(max_size.width, remaining_height));
                    let sizing = child.sizing();
                    remaining_height = remaining_height.saturating_sub(sizing.height);
                    max_child_width = max_child_width.max(sizing.width);
                }
            }
        }

        // Second pass: set constraints on flex children and update cached rects with sizes
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                if flex_scores[i] > 0 {
                    // Calculate height for this flex child based on its flex score
                    let flex_height = if total_flex > 0 {
                        (remaining_height * flex_scores[i]) / total_flex
                    } else {
                        0
                    };
                    
                    // Set constraints on flex child with calculated height
                    child.set_constraints(Size::new(max_size.width, flex_height));
                    let sizing = child.sizing();
                    self.child_rects.as_mut()[i].size = sizing.into();
                    max_child_width = max_child_width.max(sizing.width);
                } else {
                    // Non-flex child already has constraints set, just get final size
                    let sizing = child.sizing();
                    self.child_rects.as_mut()[i].size = sizing.into();
                }
            }
        }

        // Now compute positions based on alignment
        let (mut y_offset, spacing) = match self.main_axis_alignment {
            MainAxisAlignment::Start => (0u32, 0u32),
            MainAxisAlignment::Center => (remaining_height / 2, 0),
            MainAxisAlignment::End => (remaining_height, 0),
            MainAxisAlignment::SpaceBetween => {
                if len > 1 {
                    (0, remaining_height / (len as u32 - 1))
                } else {
                    (0, 0)
                }
            }
            MainAxisAlignment::SpaceAround => {
                let spacing = remaining_height / (len as u32);
                (spacing / 2, spacing)
            }
            MainAxisAlignment::SpaceEvenly => {
                let spacing = remaining_height / (len as u32 + 1);
                (spacing, spacing)
            }
        };

        // Set positions for all children
        let mut total_child_height = 0u32;
        let spacing_before = self.spacing_before.as_ref();
        let child_rects = self.child_rects.as_mut();
        
        for i in 0..len {
            // Add spacing BEFORE this child
            y_offset = y_offset.saturating_add(spacing_before[i]);

            let size = child_rects[i].size;
            total_child_height += size.height;

            let x_offset = match self.cross_axis_alignment {
                CrossAxisAlignment::Start => 0,
                CrossAxisAlignment::Center => {
                    // Center within the column's actual width, not the constraint
                    let available_width = max_child_width.saturating_sub(size.width);
                    (available_width / 2) as i32
                }
                CrossAxisAlignment::End => {
                    // Align to the end of the column's actual width, not the constraint
                    let available_width = max_child_width.saturating_sub(size.width);
                    available_width as i32
                }
            };
            child_rects[i].top_left = Point::new(x_offset, y_offset as i32);
            // Add the child height and alignment spacing
            y_offset = y_offset.saturating_add(size.height)
                .saturating_add(spacing);
        }

        // Compute and store sizing
        let width = max_child_width;
        let height = match self.main_axis_alignment {
            MainAxisAlignment::Start => total_child_height + total_spacing,
            _ => max_size.height,
        };

        self.sizing = Some(crate::Sizing { width, height });
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing.expect("set_constraints must be called before sizing")
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let child_rects = self.child_rects.as_ref();
        let len = self.children.len();
        
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                let area = child_rects[i];
                if area.contains(point) || is_release {
                    let relative_point = Point::new(
                        point.x - area.top_left.x,
                        point.y - area.top_left.y
                    );
                    child.handle_touch(relative_point, current_time, is_release);
                }
            }
        }
        None
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.handle_vertical_drag(start_y, current_y, is_release);
            }
        }
    }

    fn force_full_redraw(&mut self) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.force_full_redraw();
            }
        }
    }
}

// Widget implementation for Column<Vec<W>>
impl<W, C> Widget for super::Column<Vec<W>>
where
    W: Widget<Color = C>,
    C: crate::WidgetColor,
{
    type Color = C;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.sizing.unwrap();

        // Draw each child in its pre-computed rectangle
        for (i, child) in self.children.iter_mut().enumerate() {
            child.draw(&mut target.clone().crop(self.child_rects[i]), current_time)?;

            // Draw debug border if enabled
            if self.debug_borders {
                use embedded_graphics::primitives::PrimitiveStyle;
                use embedded_graphics::pixelcolor::{Rgb565, Gray4, Gray2};
                use embedded_graphics::pixelcolor::raw::RawData;

                let rect = self.child_rects[i];

                if let Some(debug_color) = match C::Raw::BITS_PER_PIXEL {
                    16 => {
                        // Assume Rgb565 for 16-bit
                        Some(unsafe {
                            *(&Rgb565::WHITE as *const Rgb565 as *const C)
                        })
                    },
                    4 => {
                        // Assume Gray4 for 4-bit
                        Some(unsafe {
                            *(&Gray4::WHITE as *const Gray4 as *const C)
                        })
                    },
                    2 => {
                        // Assume Gray2 for 2-bit
                        Some(unsafe {
                            *(&Gray2::WHITE as *const Gray2 as *const C)
                        })
                    },
                    _ => None,
                } {
                    rect.into_styled(PrimitiveStyle::with_stroke(debug_color, 1))
                        .draw(target)?;
                }
            }
        }

        Ok(())
    }
}

// Widget implementation for Column<[W; N]>
impl<W, C, const N: usize> Widget for super::Column<[W; N]>
where
    W: Widget<Color = C>,
    C: crate::WidgetColor,
{
    type Color = C;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.sizing.unwrap();

        // Draw each child in its pre-computed rectangle
        for (i, child) in self.children.iter_mut().enumerate() {
            child.draw(&mut target.clone().crop(self.child_rects[i]), current_time)?;

            // Draw debug border if enabled
            if self.debug_borders {
                use embedded_graphics::primitives::PrimitiveStyle;
                use embedded_graphics::pixelcolor::{Rgb565, Gray4, Gray2};
                use embedded_graphics::pixelcolor::raw::RawData;

                let rect = self.child_rects[i];

                if let Some(debug_color) = match C::Raw::BITS_PER_PIXEL {
                    16 => {
                        // Assume Rgb565 for 16-bit
                        Some(unsafe {
                            *(&Rgb565::WHITE as *const Rgb565 as *const C)
                        })
                    },
                    4 => {
                        // Assume Gray4 for 4-bit
                        Some(unsafe {
                            *(&Gray4::WHITE as *const Gray4 as *const C)
                        })
                    },
                    2 => {
                        // Assume Gray2 for 2-bit
                        Some(unsafe {
                            *(&Gray2::WHITE as *const Gray2 as *const C)
                        })
                    },
                    _ => None,
                } {
                    rect.into_styled(PrimitiveStyle::with_stroke(debug_color, 1))
                        .draw(target)?;
                }
            }
        }

        Ok(())
    }
}

// Generic DynWidget implementation for Row
impl<T> crate::DynWidget for super::Row<T>
where
    T: AssociatedArray
{
    fn set_constraints(&mut self, max_size: Size) {
        let len = self.children.len();
        
        if len == 0 {
            self.sizing = Some(crate::Sizing { width: 0, height: 0 });
            return;
        }

        let mut remaining_width = max_size.width;
        let mut max_child_height = 0u32;

        // Account for all spacing in remaining width
        let total_spacing: u32 = self.spacing_before.as_ref().iter().sum();
        remaining_width = remaining_width.saturating_sub(total_spacing);
        
        // Get flex scores and calculate total flex
        let flex_scores = self.flex_scores.as_ref();
        let total_flex: u32 = flex_scores.iter().sum();

        // First pass: set constraints on non-flex children
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                if flex_scores[i] == 0 {
                    // Set constraints on non-flex child with remaining available width
                    child.set_constraints(Size::new(remaining_width, max_size.height));
                    let sizing = child.sizing();
                    remaining_width = remaining_width.saturating_sub(sizing.width);
                    max_child_height = max_child_height.max(sizing.height);
                }
            }
        }

        // Second pass: set constraints on flex children and update cached rects with sizes
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                if flex_scores[i] > 0 {
                    // Calculate width for this flex child based on its flex score
                    let flex_width = if total_flex > 0 {
                        (remaining_width * flex_scores[i]) / total_flex
                    } else {
                        0
                    };
                    
                    // Set constraints on flex child with calculated width
                    child.set_constraints(Size::new(flex_width, max_size.height));
                    let sizing = child.sizing();
                    self.child_rects.as_mut()[i].size = sizing.into();
                    max_child_height = max_child_height.max(sizing.height);
                } else {
                    // Non-flex child already has constraints set, just get final size
                    let sizing = child.sizing();
                    self.child_rects.as_mut()[i].size = sizing.into();
                }
            }
        }

        // Now compute positions based on alignment
        let (mut x_offset, spacing) = match self.main_axis_alignment {
            super::MainAxisAlignment::Start => (0u32, 0u32),
            super::MainAxisAlignment::Center => (remaining_width / 2, 0),
            super::MainAxisAlignment::End => (remaining_width, 0),
            super::MainAxisAlignment::SpaceBetween => {
                if len > 1 {
                    (0, remaining_width / (len as u32 - 1))
                } else {
                    (0, 0)
                }
            }
            super::MainAxisAlignment::SpaceAround => {
                let spacing = remaining_width / (len as u32);
                (spacing / 2, spacing)
            }
            super::MainAxisAlignment::SpaceEvenly => {
                let spacing = remaining_width / (len as u32 + 1);
                (spacing, spacing)
            }
        };

        // Set positions for all children
        let mut total_child_width = 0u32;
        let spacing_before = self.spacing_before.as_ref();
        let child_rects = self.child_rects.as_mut();
        
        for i in 0..len {
            // Add spacing BEFORE this child
            x_offset = x_offset.saturating_add(spacing_before[i]);

            let size = child_rects[i].size;
            total_child_width += size.width;

            let y_offset = match self.cross_axis_alignment {
                super::CrossAxisAlignment::Start => 0,
                super::CrossAxisAlignment::Center => {
                    let available_height = max_child_height.saturating_sub(size.height);
                    (available_height / 2) as i32
                }
                super::CrossAxisAlignment::End => {
                    let available_height = max_child_height.saturating_sub(size.height);
                    available_height as i32
                }
            };
            child_rects[i].top_left = Point::new(x_offset as i32, y_offset);
            // Add the child width and alignment spacing
            x_offset = x_offset.saturating_add(size.width)
                .saturating_add(spacing);
        }

        // Compute and store sizing
        let width = match self.main_axis_alignment {
            super::MainAxisAlignment::Start => total_child_width + total_spacing,
            _ => max_size.width,
        };

        self.sizing = Some(crate::Sizing { width, height: max_child_height });
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing.expect("set_constraints must be called before sizing")
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let child_rects = self.child_rects.as_ref();
        let len = self.children.len();
        
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                let area = child_rects[i];
                if area.contains(point) || is_release {
                    let relative_point = Point::new(
                        point.x - area.top_left.x,
                        point.y - area.top_left.y
                    );
                    child.handle_touch(relative_point, current_time, is_release);
                }
            }
        }
        None
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.handle_vertical_drag(start_y, current_y, is_release);
            }
        }
    }

    fn force_full_redraw(&mut self) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.force_full_redraw();
            }
        }
    }
}

// Generic DynWidget implementation for Stack
impl<T> crate::DynWidget for super::Stack<T>
where
    T: AssociatedArray
{
    fn set_constraints(&mut self, max_size: Size) {
        let len = self.children.len();
        
        if len == 0 {
            self.sizing = Some(crate::Sizing { width: 0, height: 0 });
            return;
        }

        let mut max_width = 0u32;
        let mut max_height = 0u32;

        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                // Set constraints on each child with full available size
                child.set_constraints(max_size);
                let sizing = child.sizing();

            // Calculate position based on whether it's positioned or not
            let position = if self.is_positioned.as_ref()[i] {
                let pos = self.positions.as_ref()[i];
                // Check if this is an alignment-encoded position (negative values)
                if pos.x < 0 || pos.y < 0 {
                    // Decode alignment from the encoded position
                    let alignment = match (pos.x, pos.y) {
                        (-1, -1) => crate::Alignment::TopLeft,
                        (-2, -1) => crate::Alignment::TopCenter,
                        (-3, -1) => crate::Alignment::TopRight,
                        (-1, -2) => crate::Alignment::CenterLeft,
                        (-2, -2) => crate::Alignment::Center,
                        (-3, -2) => crate::Alignment::CenterRight,
                        (-1, -3) => crate::Alignment::BottomLeft,
                        (-2, -3) => crate::Alignment::BottomCenter,
                        (-3, -3) => crate::Alignment::BottomRight,
                        _ => crate::Alignment::TopLeft, // Default fallback
                    };

                    // Use the alignment helper methods
                    alignment.offset(max_size, sizing.into())
                } else {
                    // Use the absolute position
                    pos
                }
            } else {
                // Use the alignment helper methods
                self.alignment.offset(max_size, sizing.into())
            };

            // Store the child's rectangle
            self.child_rects.as_mut()[i] = Rectangle::new(position, sizing.into());

                // Track maximum dimensions for the stack's sizing
                let right = (position.x as u32).saturating_add(sizing.width);
                let bottom = (position.y as u32).saturating_add(sizing.height);
                max_width = max_width.max(right);
                max_height = max_height.max(bottom);
            }
        }

        // Stack's size is the bounding box of all children
        self.sizing = Some(crate::Sizing {
            width: max_width.min(max_size.width),
            height: max_height.min(max_size.height),
        });
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing.expect("set_constraints must be called before sizing")
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let child_rects = self.child_rects.as_ref();
        let len = self.children.len();
        
        // Handle touches in reverse order (top-most children first)
        // For now, we'll just check all children (can be optimized later)
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                let area = child_rects[i];
                if area.contains(point) || is_release {
                    let relative_point = Point::new(
                        point.x - area.top_left.x,
                        point.y - area.top_left.y
                    );
                    child.handle_touch(relative_point, current_time, is_release);
                }
            }
        }
        None
    }

    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.handle_vertical_drag(start_y, current_y, is_release);
            }
        }
    }

    fn force_full_redraw(&mut self) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.force_full_redraw();
            }
        }
    }
}