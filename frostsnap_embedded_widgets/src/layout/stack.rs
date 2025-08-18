use crate::super_draw_target::SuperDrawTarget;
use crate::{alignment::Alignment, widget_tuple::{AssociatedArray, WidgetTuple}, Instant, Widget};
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
pub struct Stack<T: AssociatedArray> {
    pub children: T,
    pub alignment: Alignment,
    /// Tracks which children are positioned (true) vs non-positioned (false)
    pub(crate) is_positioned: T::Array<bool>,
    /// Positions for all children (only used for positioned children)
    pub(crate) positions: T::Array<Point>,
    /// Cached rectangles for each child
    pub(crate) child_rects: T::Array<Rectangle>,
    pub(crate) sizing: Option<crate::Sizing>,
}

/// Helper to start building a Stack with no children
impl Stack<()> {
    pub fn builder() -> Self {
        Self::new(())
    }
}

impl<T: AssociatedArray> Stack<T> {
    pub fn new(children: T) -> Self {
        Self {
            is_positioned: children.create_array_with(false),
            positions: children.create_array_with(Point::zero()),
            child_rects: children.create_array_with(Rectangle::zero()),
            children,
            alignment: Alignment::default(),
            sizing: None,
        }
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

impl<T: WidgetTuple> Stack<T> {

    /// Add a non-positioned widget to the stack
    pub fn push<W: crate::DynWidget>(self, widget: W) -> Stack<T::Add<W>>
    where
        T: WidgetTuple
    {
        let new_children = self.children.add(widget);

        // Copy over existing arrays and add new entry
        let mut new_is_positioned = new_children.create_array_with(false);
        let mut new_positions = new_children.create_array_with(Point::zero());

        new_is_positioned.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.is_positioned.as_ref());
        new_positions.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.positions.as_ref());

        // New child is non-positioned
        new_is_positioned.as_mut()[T::TUPLE_LEN] = false;

        Stack {
            is_positioned: new_is_positioned,
            positions: new_positions,
            child_rects: new_children.create_array_with(Rectangle::zero()),
            children: new_children,
            alignment: self.alignment,
            sizing: None,
        }
    }

    /// Add a positioned widget to the stack at a specific location
    pub fn push_positioned<W: crate::DynWidget>(self, widget: W, x: i32, y: i32) -> Stack<T::Add<W>>
    where
        T: WidgetTuple
    {
        let new_children = self.children.add(widget);

        // Copy over existing arrays and add new entry
        let mut new_is_positioned = new_children.create_array_with(false);
        let mut new_positions = new_children.create_array_with(Point::zero());

        new_is_positioned.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.is_positioned.as_ref());
        new_positions.as_mut()[..T::TUPLE_LEN].copy_from_slice(self.positions.as_ref());

        // New child is positioned
        new_is_positioned.as_mut()[T::TUPLE_LEN] = true;
        new_positions.as_mut()[T::TUPLE_LEN] = Point::new(x, y);

        Stack {
            is_positioned: new_is_positioned,
            positions: new_positions,
            child_rects: new_children.create_array_with(Rectangle::zero()),
            children: new_children,
            alignment: self.alignment,
            sizing: None,
        }
    }

    /// Add a widget with alignment-based positioning
    /// The widget will be positioned according to the alignment within the Stack's bounds
    pub fn push_aligned<W: crate::DynWidget>(self, widget: W, alignment: Alignment) -> Stack<T::Add<W>>
    where
        T: WidgetTuple
    {
        let new_children = self.children.add(widget);

        // Copy over existing arrays and add new entry
        let mut new_is_positioned = new_children.create_array_with(false);
        let mut new_positions = new_children.create_array_with(Point::zero());

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
            is_positioned: new_is_positioned,
            positions: new_positions,
            child_rects: new_children.create_array_with(Rectangle::zero()),
            children: new_children,
            alignment: self.alignment,
            sizing: None,
        }
    }
}

// Macro to implement Widget for Stack with tuples of different sizes
macro_rules! impl_stack_for_tuple {
    ($len:literal, $($t:ident),+) => {
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
