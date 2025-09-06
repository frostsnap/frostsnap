use super::{Alignment, AssociatedArray};
use crate::super_draw_target::SuperDrawTarget;
use crate::{Instant, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};

/// Positioning mode for a child in the Stack
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Positioning {
    /// Uses the Stack's default alignment
    Default,
    /// Positioned at absolute coordinates
    Absolute(Point),
    /// Positioned with a specific alignment
    Aligned(Alignment),
}

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
    /// Positioning information for each child
    pub(crate) positioning: T::Array<Positioning>,
    /// Cached rectangles for each child
    pub(crate) child_rects: T::Array<Rectangle>,
    pub(crate) sizing: Option<crate::Sizing>,
    /// Optional index for showing only one child (IndexedStack behavior)
    pub(crate) index: Option<usize>,
    /// Track if we've cleared non-indexed children
    pub(crate) cleared: bool,
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
            positioning: children.create_array_with(Positioning::Default),
            child_rects: children.create_array_with(Rectangle::zero()),
            children,
            alignment: Alignment::default(),
            sizing: None,
            index: None,
            cleared: false,
        }
    }

    /// Set the index of the only child to show (None shows all children)
    pub fn set_index(&mut self, new_index: Option<usize>) {
        if self.index != new_index {
            self.index = new_index;
            self.cleared = false;
            // Force redraw of the newly visible widget
            if let Some(idx) = new_index {
                if let Some(child) = self.children.get_dyn_child(idx) {
                    child.force_full_redraw();
                }
            } else {
                // If switching back to showing all, force redraw all
                let len = self.children.len();
                for i in 0..len {
                    if let Some(child) = self.children.get_dyn_child(i) {
                        child.force_full_redraw();
                    }
                }
            }
        }
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

impl<T: AssociatedArray> Stack<T> {
    /// Add a non-positioned widget to the stack
    pub fn push<W>(self, widget: W) -> Stack<<T as crate::layout::PushWidget<W>>::Output>
    where
        T: crate::layout::PushWidget<W>,
        W: crate::DynWidget,
    {
        if self.sizing.is_some() {
            panic!("Cannot push widgets after set_constraints has been called");
        }

        let old_len = self.children.len();
        let new_children = self.children.push_widget(widget);

        // Copy over existing array and add new entry
        let mut new_positioning = new_children.create_array_with(Positioning::Default);
        new_positioning.as_mut()[..old_len].copy_from_slice(self.positioning.as_ref());

        // New child uses default positioning
        new_positioning.as_mut()[old_len] = Positioning::Default;

        Stack {
            positioning: new_positioning,
            child_rects: new_children.create_array_with(Rectangle::zero()),
            children: new_children,
            alignment: self.alignment,
            sizing: None,
            index: self.index,
            cleared: self.cleared,
        }
    }

    /// Add a positioned widget to the stack at a specific location
    pub fn push_positioned<W>(
        self,
        widget: W,
        x: i32,
        y: i32,
    ) -> Stack<<T as crate::layout::PushWidget<W>>::Output>
    where
        T: crate::layout::PushWidget<W>,
        W: crate::DynWidget,
    {
        if self.sizing.is_some() {
            panic!("Cannot push widgets after set_constraints has been called");
        }

        let old_len = self.children.len();
        let new_children = self.children.push_widget(widget);

        // Copy over existing array and add new entry
        let mut new_positioning = new_children.create_array_with(Positioning::Default);
        new_positioning.as_mut()[..old_len].copy_from_slice(self.positioning.as_ref());

        // New child is positioned at absolute coordinates
        new_positioning.as_mut()[old_len] = Positioning::Absolute(Point::new(x, y));

        Stack {
            positioning: new_positioning,
            child_rects: new_children.create_array_with(Rectangle::zero()),
            children: new_children,
            alignment: self.alignment,
            sizing: None,
            index: self.index,
            cleared: self.cleared,
        }
    }

    /// Add a widget with alignment-based positioning
    /// The widget will be positioned according to the alignment within the Stack's bounds
    pub fn push_aligned<W>(
        self,
        widget: W,
        alignment: Alignment,
    ) -> Stack<<T as crate::layout::PushWidget<W>>::Output>
    where
        T: crate::layout::PushWidget<W>,
        W: crate::DynWidget,
    {
        if self.sizing.is_some() {
            panic!("Cannot push widgets after set_constraints has been called");
        }

        let old_len = self.children.len();
        let new_children = self.children.push_widget(widget);

        // Copy over existing array and add new entry
        let mut new_positioning = new_children.create_array_with(Positioning::Default);
        new_positioning.as_mut()[..old_len].copy_from_slice(self.positioning.as_ref());

        // New child is positioned with specific alignment
        new_positioning.as_mut()[old_len] = Positioning::Aligned(alignment);

        Stack {
            positioning: new_positioning,
            child_rects: new_children.create_array_with(Rectangle::zero()),
            children: new_children,
            alignment: self.alignment,
            sizing: None,
            index: self.index,
            cleared: self.cleared,
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
                let size = self.sizing.unwrap();

                // Clear the entire stack area once when index changes
                if self.index.is_some() && !self.cleared {
                    let stack_rect = Rectangle::new(Point::zero(), size.into());
                    target.clear_area(&stack_rect)?;
                    self.cleared = true;
                }

                // Get mutable references to children
                #[allow(non_snake_case, unused_variables)]
                let ($(ref mut $t,)+) = self.children;

                // Draw each child in order (first to last, so later children appear on top)
                let mut child_index = 0;
                $(
                    {
                        let rect = self.child_rects[child_index];
                        if self.index.is_none() || self.index == Some(child_index) {
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

// Generic DynWidget implementation for Stack
impl<T> crate::DynWidget for Stack<T>
where
    T: AssociatedArray,
{
    fn set_constraints(&mut self, max_size: Size) {
        let len = self.children.len();

        if len == 0 {
            self.sizing = Some(crate::Sizing {
                width: 0,
                height: 0,
            });
            return;
        }

        let mut max_width = 0u32;
        let mut max_height = 0u32;

        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                // Set constraints on each child with full available size
                child.set_constraints(max_size);
                let sizing = child.sizing();

                // Calculate position based on positioning mode
                let position = match self.positioning.as_ref()[i] {
                    Positioning::Default => {
                        // Use the stack's default alignment
                        self.alignment.offset(max_size, sizing.into())
                    }
                    Positioning::Absolute(pos) => {
                        // Use the absolute position
                        pos
                    }
                    Positioning::Aligned(alignment) => {
                        // Use the specific alignment
                        alignment.offset(max_size, sizing.into())
                    }
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
        self.sizing
            .expect("set_constraints must be called before sizing")
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let child_rects = self.child_rects.as_ref();
        let len = self.children.len();

        // If index is set, only handle touch for that child
        if let Some(idx) = self.index {
            if idx < len {
                if let Some(child) = self.children.get_dyn_child(idx) {
                    let area = child_rects[idx];
                    if area.contains(point) || is_release {
                        let relative_point =
                            Point::new(point.x - area.top_left.x, point.y - area.top_left.y);
                        if let Some(mut key_touch) =
                            child.handle_touch(relative_point, current_time, is_release)
                        {
                            // Translate the KeyTouch rectangle back to parent coordinates
                            key_touch.translate(area.top_left);
                            return Some(key_touch);
                        }
                    }
                }
            }
        } else {
            // Handle touches in reverse order (top-most children first)
            // Check from last to first since later children are drawn on top
            for i in (0..len).rev() {
                if let Some(child) = self.children.get_dyn_child(i) {
                    let area = child_rects[i];
                    if area.contains(point) || is_release {
                        let relative_point =
                            Point::new(point.x - area.top_left.x, point.y - area.top_left.y);
                        if let Some(mut key_touch) =
                            child.handle_touch(relative_point, current_time, is_release)
                        {
                            // Translate the KeyTouch rectangle back to parent coordinates
                            key_touch.translate(area.top_left);
                            return Some(key_touch);
                        }
                    }
                }
            }
        }
        None
    }

    #[allow(clippy::needless_range_loop)]
    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32, is_release: bool) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.handle_vertical_drag(start_y, current_y, is_release);
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn force_full_redraw(&mut self) {
        let len = self.children.len();
        for i in 0..len {
            if let Some(child) = self.children.get_dyn_child(i) {
                child.force_full_redraw();
            }
        }
    }
}

// Widget implementation for Stack<Vec<W>>
impl<W, C> Widget for Stack<Vec<W>>
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
        let size = self.sizing.unwrap();

        // Clear the entire stack area once when index changes
        if self.index.is_some() && !self.cleared {
            let stack_rect = Rectangle::new(Point::zero(), size.into());
            target.clear_area(&stack_rect)?;
            self.cleared = true;
        }

        // Draw each child in its pre-computed rectangle
        // For stacks, we draw in order (bottom to top)
        for (i, child) in self.children.iter_mut().enumerate() {
            let rect = self.child_rects[i];
            if self.index.is_none() || self.index == Some(i) {
                child.draw(&mut target.clone().crop(rect), current_time)?;
            }
        }

        Ok(())
    }
}

// Widget implementation for Stack<[W; N]>
impl<W, C, const N: usize> Widget for Stack<[W; N]>
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
        let size = self.sizing.unwrap();

        // Clear the entire stack area once when index changes
        if self.index.is_some() && !self.cleared {
            let stack_rect = Rectangle::new(Point::zero(), size.into());
            target.clear_area(&stack_rect)?;
            self.cleared = true;
        }

        // Draw each child in its pre-computed rectangle
        // For stacks, we draw in order (bottom to top)
        for (i, child) in self.children.iter_mut().enumerate() {
            let rect = self.child_rects[i];
            if self.index.is_none() || self.index == Some(i) {
                child.draw(&mut target.clone().crop(rect), current_time)?;
            }
        }

        Ok(())
    }
}
