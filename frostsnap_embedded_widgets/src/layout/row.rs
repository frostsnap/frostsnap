use super::{CrossAxisAlignment, MainAxisAlignment, MainAxisSize};
use crate::super_draw_target::SuperDrawTarget;
use crate::{
    widget_tuple::{AssociatedArray, WidgetTuple},
    Instant, Widget,
};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    prelude::*,
    primitives::Rectangle,
};

/// A row widget that arranges its children horizontally
///
/// The Row widget takes a tuple of child widgets and arranges them horizontally.
/// You can control the distribution of children along the horizontal axis using `MainAxisAlignment`
/// and their vertical alignment using `CrossAxisAlignment`.
///
/// # Example
/// ```ignore
/// let row = Row::new((widget1, widget2, widget3))
///     .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
///     .with_cross_axis_alignment(CrossAxisAlignment::Center);
/// ```
#[derive(PartialEq)]
pub struct Row<T: AssociatedArray> {
    pub children: T,
    pub cross_axis_alignment: CrossAxisAlignment,
    pub main_axis_alignment: MainAxisAlignment,
    pub main_axis_size: MainAxisSize,
    pub(crate) child_rects: T::Array<Rectangle>,
    pub(crate) debug_borders: bool,
    pub(crate) sizing: Option<crate::Sizing>,
    /// Spacing to add before each child (indexed by child position)
    pub(crate) spacing_before: T::Array<u32>,
    /// Flex scores for each child (0 means not flexible)
    pub(crate) flex_scores: T::Array<u32>,
}

/// Helper to start building a Row with no children
impl Row<()> {
    pub fn builder() -> Self {
        Self::new(())
    }
}

impl<T: AssociatedArray> Row<T> {
    pub fn new(children: T) -> Self {
        // Don't extract sizes here - wait for set_constraints to be called
        Self {
            child_rects: children.create_array_with(Rectangle::zero()),
            spacing_before: children.create_array_with(0),
            flex_scores: children.create_array_with(0),
            children,
            cross_axis_alignment: CrossAxisAlignment::Center,
            main_axis_alignment: MainAxisAlignment::Start,
            main_axis_size: MainAxisSize::Min, // Start alignment defaults to Min
            debug_borders: false,
            sizing: None,
        }
    }

    /// Set the gap before a specific child (in pixels)
    pub fn set_gap(&mut self, child_index: usize, gap: u32) {
        if child_index < self.spacing_before.as_ref().len() {
            self.spacing_before.as_mut()[child_index] = gap;
        }
    }

    /// Set the same gap before all children except the first
    pub fn set_uniform_gap(&mut self, gap: u32) {
        let spacing = self.spacing_before.as_mut();
        if !spacing.is_empty() {
            spacing[0] = 0; // No gap before first child
            for space in spacing.iter_mut().skip(1) {
                *space = gap;
            }
        }
    }

    /// Set a gap before the last added widget
    pub fn gap(mut self, gap: u32) -> Self {
        let len = T::len(&self.children);
        if len > 0 {
            self.spacing_before.as_mut()[len - 1] = gap;
        }
        self
    }

    /// Set the flex score for the last added widget
    pub fn flex(mut self, score: u32) -> Self {
        let len = T::len(&self.children);
        if len > 0 {
            self.flex_scores.as_mut()[len - 1] = score;
        }
        self
    }

    pub fn with_cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }

    pub fn with_main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        // Start alignment uses Min size by default, all others use Max
        if matches!(alignment, MainAxisAlignment::Start) {
            self.main_axis_size = MainAxisSize::Min;
        } else {
            self.main_axis_size = MainAxisSize::Max;
        }
        self
    }

    pub fn with_main_axis_size(mut self, size: MainAxisSize) -> Self {
        self.main_axis_size = size;
        self
    }

    pub fn with_debug_borders(mut self, enabled: bool) -> Self {
        self.debug_borders = enabled;
        self
    }
}

impl<T: WidgetTuple> Row<T> {
    /// Add a widget to the row
    pub fn push<W: crate::DynWidget>(self, widget: W) -> Row<T::Add<W>>
    where
        T: WidgetTuple,
    {
        let new_children = self.children.add(widget);

        // Copy over existing values and add new ones
        let mut new_spacing = new_children.create_array_with(0);
        let old_spacing = self.spacing_before.as_ref();
        new_spacing.as_mut()[..T::TUPLE_LEN].copy_from_slice(old_spacing);
        new_spacing.as_mut()[T::TUPLE_LEN] = 0; // Default gap is 0

        let mut new_flex = new_children.create_array_with(0);
        let old_flex = self.flex_scores.as_ref();
        new_flex.as_mut()[..T::TUPLE_LEN].copy_from_slice(old_flex);
        new_flex.as_mut()[T::TUPLE_LEN] = 0; // Default flex is 0 (not flexible)

        Row {
            child_rects: new_children.create_array_with(Rectangle::zero()),
            spacing_before: new_spacing,
            flex_scores: new_flex,
            children: new_children,
            cross_axis_alignment: self.cross_axis_alignment,
            main_axis_alignment: self.main_axis_alignment,
            main_axis_size: self.main_axis_size,
            debug_borders: self.debug_borders,
            sizing: None,
        }
    }
}

// Macro to implement Widget for Row with tuples of different sizes
macro_rules! impl_row_for_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: Widget<Color = C>),+, C: crate::WidgetColor> Widget for Row<($($t,)+)> {
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

                // Draw each child in its pre-computed rectangle
                let mut child_index = 0;
                $(
                    {
                        $t.draw(&mut target.clone().crop(self.child_rects[child_index]), current_time)?;

                        // Draw debug border if enabled
                        if self.debug_borders {
                            super::draw_debug_rect(target, self.child_rects[child_index])?;
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
impl_row_for_tuple!(13, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_row_for_tuple!(14, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_row_for_tuple!(15, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_row_for_tuple!(16, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);
impl_row_for_tuple!(17, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17);
impl_row_for_tuple!(
    18, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18
);
impl_row_for_tuple!(
    19, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19
);
impl_row_for_tuple!(
    20, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20
);

// Generic DynWidget implementation for Row
impl<T> crate::DynWidget for Row<T>
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
            if flex_scores[i] == 0 {
                if let Some(child) = self.children.get_dyn_child(i) {
                    // Set constraints on non-flex child with remaining available width
                    child.set_constraints(Size::new(remaining_width, max_size.height));
                    let sizing = child.sizing();
                    remaining_width = remaining_width.saturating_sub(sizing.width);
                    max_child_height = max_child_height.max(sizing.height);
                    self.child_rects.as_mut()[i].size = sizing.into();
                }
            }
        }

        let total_flex_width = remaining_width;

        // Second pass: set constraints on flex children and update cached rects with sizes
        for i in 0..len {
            if flex_scores[i] > 0 {
                if let Some(child) = self.children.get_dyn_child(i) {
                    // Calculate width for this flex child based on its flex score
                    let flex_width = (total_flex_width * flex_scores[i]) / total_flex;
                    // Set constraints on flex child with calculated width
                    child.set_constraints(Size::new(flex_width, max_size.height));
                    let sizing = child.sizing();
                    remaining_width = remaining_width.saturating_sub(sizing.width);
                    max_child_height = max_child_height.max(sizing.height);
                    self.child_rects.as_mut()[i].size = sizing.into();
                }
            }
        }

        // Now compute positions based on alignment
        let (mut x_offset, spacing) = match self.main_axis_alignment {
            MainAxisAlignment::Start => (0u32, 0u32),
            MainAxisAlignment::Center => (remaining_width / 2, 0),
            MainAxisAlignment::End => (remaining_width, 0),
            MainAxisAlignment::SpaceBetween => {
                if len > 1 {
                    (0, remaining_width / (len as u32 - 1))
                } else {
                    (0, 0)
                }
            }
            MainAxisAlignment::SpaceAround => {
                let spacing = remaining_width / (len as u32);
                (spacing / 2, spacing)
            }
            MainAxisAlignment::SpaceEvenly => {
                let spacing = remaining_width / (len as u32 + 1);
                (spacing, spacing)
            }
        };

        // Third pass: Set positions for all children
        let spacing_before = self.spacing_before.as_ref();
        let child_rects = self.child_rects.as_mut();

        for i in 0..len {
            // Add spacing BEFORE this child
            x_offset = x_offset.saturating_add(spacing_before[i]);

            let size = child_rects[i].size;

            let y_offset = match self.cross_axis_alignment {
                CrossAxisAlignment::Start => 0,
                CrossAxisAlignment::Center => {
                    let available_height = max_child_height.saturating_sub(size.height);
                    (available_height / 2) as i32
                }
                CrossAxisAlignment::End => {
                    let available_height = max_child_height.saturating_sub(size.height);
                    available_height as i32
                }
            };
            child_rects[i].top_left = Point::new(x_offset as i32, y_offset);
            // Add the child width and alignment spacing
            x_offset = x_offset.saturating_add(size.width).saturating_add(spacing);
        }

        // Compute and store sizing based on MainAxisSize
        let width = match self.main_axis_size {
            MainAxisSize::Min => x_offset,       // Only as wide as needed
            MainAxisSize::Max => max_size.width, // Take full available width
        };

        self.sizing = Some(crate::Sizing {
            width,
            height: max_child_height,
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

        for i in 0..len {
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

// Widget implementation for Row<Vec<W>>
impl<W, C> Widget for Row<Vec<W>>
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
                super::draw_debug_rect(target, self.child_rects[i])?;
            }
        }

        Ok(())
    }
}

// Widget implementation for Row<[W; N]>
impl<W, C, const N: usize> Widget for Row<[W; N]>
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
                super::draw_debug_rect(target, self.child_rects[i])?;
            }
        }

        Ok(())
    }
}
