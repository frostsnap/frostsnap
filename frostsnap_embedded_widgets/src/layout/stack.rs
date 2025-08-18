use crate::super_draw_target::SuperDrawTarget;
use crate::{alignment::Alignment, widget_tuple::WidgetTuple, Instant, Widget};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
    primitives::Rectangle,
};

/// A positioned wrapper for widgets in a Stack
pub struct Positioned<W> {
    pub widget: W,
    pub position: Point,
}

impl<W> Positioned<W> {
    pub fn new(widget: W, x: i32, y: i32) -> Self {
        Self {
            widget,
            position: Point::new(x, y),
        }
    }
}

/// A widget that can be either positioned or non-positioned in a Stack
pub enum StackChild<W> {
    NonPositioned(W),
    Positioned(Positioned<W>),
}

/// A stack widget that layers its children on top of each other
///
/// The Stack widget allows you to overlay multiple widgets. Children can be:
/// - Non-positioned: aligned according to the stack's alignment setting
/// - Positioned: placed at specific coordinates
///
/// Children are drawn in order, so later children appear on top of earlier ones.
///
/// # Example
/// ```ignore
/// let stack = Stack::builder()
///     .push(background_widget)
///     .push_positioned(icon_widget, 10, 10)
///     .push(centered_text)
///     .with_alignment(Alignment::Center);
/// ```
#[derive(PartialEq)]
pub struct Stack<T: WidgetTuple> {
    pub children: T,
    pub alignment: Alignment,
    /// Tracks which children are positioned (true) vs non-positioned (false)
    is_positioned: T::Array<bool>,
    /// Positions for all children (only used for positioned children)
    positions: T::Array<Point>,
    /// Cached rectangles for each child
    child_rects: T::Array<Rectangle>,
    sizing: Option<crate::Sizing>,
}

/// Helper to start building a Stack with no children
impl Stack<()> {
    pub fn builder() -> Self {
        Self::new(())
    }
}

impl<T: WidgetTuple> Stack<T> {
    pub fn new(children: T) -> Self {
        Self {
            children,
            alignment: Alignment::default(),
            is_positioned: T::create_array_with(false),
            positions: T::create_array_with(Point::zero()),
            child_rects: T::create_array_with(Rectangle::zero()),
            sizing: None,
        }
    }

    /// Add a non-positioned widget to the stack
    pub fn push<W>(self, widget: W) -> Stack<T::Add<W>> {
        let new_children = self.children.add(widget);

        // Copy over existing arrays and add new entry
        let mut new_is_positioned = <T::Add<W>>::create_array_with(false);
        let mut new_positions = <T::Add<W>>::create_array_with(Point::zero());

        new_is_positioned.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.is_positioned.as_ref());
        new_positions.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.positions.as_ref());

        // New child is non-positioned
        new_is_positioned.as_mut()[T::TUPLE_LEN] = false;

        Stack {
            children: new_children,
            alignment: self.alignment,
            is_positioned: new_is_positioned,
            positions: new_positions,
            child_rects: <T::Add<W>>::create_array_with(Rectangle::zero()),
            sizing: None,
        }
    }

    /// Add a positioned widget to the stack at a specific location
    pub fn push_positioned<W>(self, widget: W, x: i32, y: i32) -> Stack<T::Add<W>> {
        let new_children = self.children.add(widget);

        // Copy over existing arrays and add new entry
        let mut new_is_positioned = <T::Add<W>>::create_array_with(false);
        let mut new_positions = <T::Add<W>>::create_array_with(Point::zero());

        new_is_positioned.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.is_positioned.as_ref());
        new_positions.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.positions.as_ref());

        // New child is positioned
        new_is_positioned.as_mut()[T::TUPLE_LEN] = true;
        new_positions.as_mut()[T::TUPLE_LEN] = Point::new(x, y);

        Stack {
            children: new_children,
            alignment: self.alignment,
            is_positioned: new_is_positioned,
            positions: new_positions,
            child_rects: <T::Add<W>>::create_array_with(Rectangle::zero()),
            sizing: None,
        }
    }

    /// Add a widget with alignment-based positioning
    /// The widget will be positioned according to the alignment within the Stack's bounds
    pub fn push_aligned<W>(self, widget: W, alignment: Alignment) -> Stack<T::Add<W>> {
        let new_children = self.children.add(widget);

        // Copy over existing arrays and add new entry
        let mut new_is_positioned = <T::Add<W>>::create_array_with(false);
        let mut new_positions = <T::Add<W>>::create_array_with(Point::zero());

        new_is_positioned.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.is_positioned.as_ref());
        new_positions.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.positions.as_ref());

        // Store alignment as a special position value (we'll interpret negative values as alignment)
        // This is a hack - ideally we'd have a separate array for alignments
        let alignment_encoded = match alignment {
            Alignment::TopLeft => Point::new(-1, -1),
            Alignment::TopCenter => Point::new(-2, -1),
            Alignment::TopRight => Point::new(-3, -1),
            Alignment::CenterLeft => Point::new(-1, -2),
            Alignment::Center => Point::new(-2, -2),
            Alignment::CenterRight => Point::new(-3, -2),
            Alignment::BottomLeft => Point::new(-1, -3),
            Alignment::BottomCenter => Point::new(-2, -3),
            Alignment::BottomRight => Point::new(-3, -3),
        };

        new_is_positioned.as_mut()[T::TUPLE_LEN] = true;
        new_positions.as_mut()[T::TUPLE_LEN] = alignment_encoded;

        Stack {
            children: new_children,
            alignment: self.alignment,
            is_positioned: new_is_positioned,
            positions: new_positions,
            child_rects: <T::Add<W>>::create_array_with(Rectangle::zero()),
            sizing: None,
        }
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

// Macro to implement Widget for Stack with tuples of different sizes
macro_rules! impl_stack_for_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: Widget<Color = C>),+, C: PixelColor> crate::DynWidget for Stack<($($t,)+)> {
            #[allow(unused_assignments)]
            fn set_constraints(&mut self, max_size: Size) {
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;

                let mut child_index = 0;
                let mut max_width = 0u32;
                let mut max_height = 0u32;

                $(
                    {
                        // Set constraints on each child with full available size
                        $t.set_constraints(max_size);
                        let sizing = $t.sizing();

                        // Calculate position based on whether it's positioned or not
                        let position = if self.is_positioned[child_index] {
                            let pos = self.positions[child_index];
                            // Check if this is an alignment-encoded position (negative values)
                            if pos.x < 0 || pos.y < 0 {
                                // Decode alignment from the encoded position
                                let alignment = match (pos.x, pos.y) {
                                    (-1, -1) => Alignment::TopLeft,
                                    (-2, -1) => Alignment::TopCenter,
                                    (-3, -1) => Alignment::TopRight,
                                    (-1, -2) => Alignment::CenterLeft,
                                    (-2, -2) => Alignment::Center,
                                    (-3, -2) => Alignment::CenterRight,
                                    (-1, -3) => Alignment::BottomLeft,
                                    (-2, -3) => Alignment::BottomCenter,
                                    (-3, -3) => Alignment::BottomRight,
                                    _ => Alignment::TopLeft, // Default fallback
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
                        self.child_rects[child_index] = Rectangle::new(position, sizing.into());

                        // Track maximum dimensions for the stack's sizing
                        let right = (position.x as u32).saturating_add(sizing.width);
                        let bottom = (position.y as u32).saturating_add(sizing.height);
                        max_width = max_width.max(right);
                        max_height = max_height.max(bottom);

                        child_index += 1;
                    }
                )+

                // Stack's size is the bounding box of all children
                self.sizing = Some(crate::Sizing {
                    width: max_width.min(max_size.width),
                    height: max_height.min(max_size.height),
                });
            }

            fn sizing(&self) -> crate::Sizing {
                self.sizing.expect("set_constraints must be called before sizing")
            }

            #[allow(unused_assignments)]
            fn handle_touch(
                &mut self,
                point: Point,
                current_time: Instant,
                is_release: bool,
            ) -> Option<crate::KeyTouch> {
                // Handle touches in reverse order (top-most children first)
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;

                // We need to check children in reverse order for proper z-ordering
                // For now, we'll just check all children (can be optimized later)
                let mut child_index = 0;
                $(
                    {
                        let area = self.child_rects[child_index];
                        if area.contains(point) || is_release {
                            let relative_point = Point::new(
                                point.x - area.top_left.x,
                                point.y - area.top_left.y
                            );
                            $t.handle_touch(relative_point, current_time, is_release);
                        }
                        child_index += 1;
                    }
                )+

                None
            }

            fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, _is_release: bool) {
                // Pass drag to all children
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;

                $(
                    $t.handle_vertical_drag(start_y, current_y, _is_release);
                )+
            }

            fn force_full_redraw(&mut self) {
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;

                $(
                    $t.force_full_redraw();
                )+
            }
        }

        impl<$($t: Widget<Color = C>),+, C: crate::WidgetColor> Widget for Stack<($($t,)+)> {
            type Color = C;

            #[allow(unused_assignments)]
            fn draw<D>(
                &mut self,
                target: &mut SuperDrawTarget<D, Self::Color>,
                current_time: Instant,
            ) -> Result<(), D::Error>
            where
                D: DrawTarget<Color = Self::Color>,
            {
                self.sizing.unwrap();

                // Get mutable references to children
                #[allow(non_snake_case, unused_variables)]
                let ($(ref mut $t,)+) = self.children;

                // Draw each child in order (first to last, so later children appear on top)
                let mut child_index = 0;
                $(
                    {
                        let rect = self.child_rects[child_index];
                        // Only draw if the rectangle is within bounds
                        if rect.size.width > 0 && rect.size.height > 0 {
                            $t.draw(&mut target.clone().crop(rect), current_time)?;
                        }
                        child_index += 1;
                    }
                )+

                Ok(())
            }
        }
    };
}

// Generate implementations for tuples up to 20 elements
impl_stack_for_tuple!(1, T1);
impl_stack_for_tuple!(2, T1, T2);
impl_stack_for_tuple!(3, T1, T2, T3);
impl_stack_for_tuple!(4, T1, T2, T3, T4);
impl_stack_for_tuple!(5, T1, T2, T3, T4, T5);
impl_stack_for_tuple!(6, T1, T2, T3, T4, T5, T6);
impl_stack_for_tuple!(7, T1, T2, T3, T4, T5, T6, T7);
impl_stack_for_tuple!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_stack_for_tuple!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_stack_for_tuple!(10, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_stack_for_tuple!(11, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_stack_for_tuple!(12, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_stack_for_tuple!(13, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_stack_for_tuple!(14, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_stack_for_tuple!(15, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_stack_for_tuple!(16, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);
impl_stack_for_tuple!(
    17, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17
);
impl_stack_for_tuple!(
    18, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18
);
impl_stack_for_tuple!(
    19, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19
);
impl_stack_for_tuple!(
    20, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20
);
