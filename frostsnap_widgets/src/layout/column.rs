use super::{AssociatedArray, CrossAxisAlignment, MainAxisAlignment, MainAxisSize};
use crate::super_draw_target::SuperDrawTarget;
use crate::{Instant, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    primitives::Rectangle,
};

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
pub struct Column<T: AssociatedArray> {
    pub children: T,
    pub cross_axis_alignment: CrossAxisAlignment,
    pub main_axis_alignment: MainAxisAlignment,
    pub main_axis_size: MainAxisSize,
    pub(crate) child_rects: T::Array<Rectangle>,
    pub(crate) debug_borders: bool,
    pub(crate) sizing: Option<crate::Sizing>,
    /// Spacing to add after each child (indexed by child position)
    pub(crate) spacing_after: T::Array<u32>,
    /// Flex scores for each child (0 means not flexible)
    pub(crate) flex_scores: T::Array<u32>,
}

/// Helper to start building a Column with no children
impl Column<()> {
    pub fn builder() -> Self {
        Self::new(())
    }
}

impl<T: AssociatedArray> Column<T> {
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

impl<T: AssociatedArray> Column<T> {
    pub fn new(children: T) -> Self {
        // Don't extract sizes here - wait for set_constraints to be called
        Self {
            child_rects: children.create_array_with(Rectangle::zero()),
            spacing_after: children.create_array_with(0),
            flex_scores: children.create_array_with(0),
            children,
            cross_axis_alignment: CrossAxisAlignment::Center,
            main_axis_alignment: MainAxisAlignment::Start,
            main_axis_size: MainAxisSize::Min, // Start alignment defaults to Min
            debug_borders: false,
            sizing: None,
        }
    }

    /// Set the gap after a specific child (in pixels)
    pub fn set_gap(&mut self, child_index: usize, gap: u32) {
        if child_index < self.spacing_after.as_ref().len() {
            self.spacing_after.as_mut()[child_index] = gap;
        }
    }

    /// Set the same gap after all children except the last
    pub fn set_uniform_gap(&mut self, gap: u32) {
        let spacing = self.spacing_after.as_mut();
        let len = spacing.len();
        if len > 0 {
            for space in spacing.iter_mut().take(len - 1) {
                *space = gap;
            }
            spacing[len - 1] = 0; // No gap after last child
        }
    }

    /// Set all children to have the same flex score
    pub fn set_all_flex(&mut self, flex: u32) {
        for score in self.flex_scores.as_mut() {
            *score = flex;
        }
    }

    /// Set a gap after the last added widget
    pub fn gap(mut self, gap: u32) -> Self {
        let len = self.children.len();
        if len > 0 {
            self.spacing_after.as_mut()[len - 1] = gap;
        }
        self
    }

    /// Set the flex score for the last added widget
    pub fn flex(mut self, score: u32) -> Self {
        let len = self.children.len();
        if len > 0 {
            self.flex_scores.as_mut()[len - 1] = score;
        }
        self
    }
}

impl<T: AssociatedArray> Column<T> {
    /// Add a widget to the column
    pub fn push<W>(self, widget: W) -> Column<<T as crate::layout::PushWidget<W>>::Output>
    where
        T: crate::layout::PushWidget<W>,
        W: crate::DynWidget,
    {
        if self.sizing.is_some() {
            panic!("Cannot push widgets after set_constraints has been called");
        }

        let old_len = self.children.len();
        let new_children = self.children.push_widget(widget);

        // Copy over existing values and add new ones
        let mut new_spacing = new_children.create_array_with(0);
        let old_spacing = self.spacing_after.as_ref();
        new_spacing.as_mut()[..old_len].copy_from_slice(old_spacing);
        new_spacing.as_mut()[old_len] = 0; // Default gap is 0

        let mut new_flex = new_children.create_array_with(0);
        let old_flex = self.flex_scores.as_ref();
        new_flex.as_mut()[..old_len].copy_from_slice(old_flex);
        new_flex.as_mut()[old_len] = 0; // Default flex is 0 (not flexible)

        Column {
            child_rects: new_children.create_array_with(Rectangle::zero()),
            spacing_after: new_spacing,
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

// Generic implementation for any type with AssociatedArray
impl<T> crate::DynWidget for Column<T>
where
    T: AssociatedArray,
{
    fn set_constraints(&mut self, max_size: Size) {
        let len = self.children.len();

        if len == 0 {
            self.sizing = Some(crate::Sizing {
                width: 0,
                height: 0,
                dirty_rect: None,
            });
            return;
        }

        let mut remaining_height = max_size.height;
        let mut max_child_width = 0u32;

        // Account for all spacing in remaining height (but not after the last child)
        let spacing_after = self.spacing_after.as_ref();
        let total_spacing: u32 = if len > 0 {
            spacing_after[..len - 1].iter().sum()
        } else {
            0
        };
        remaining_height = remaining_height.saturating_sub(total_spacing);

        // Get flex scores and calculate total flex
        let flex_scores = self.flex_scores.as_ref();
        let total_flex: u32 = flex_scores.iter().sum();

        // Create dirty_rects array that we'll populate as we go
        let mut dirty_rects = self.child_rects.clone();

        // First pass: set constraints on non-flex children
        for (i, &flex_score) in flex_scores.iter().enumerate() {
            if flex_score == 0 {
                if let Some(child) = self.children.get_dyn_child(i) {
                    // Set constraints on non-flex child with remaining available height
                    child.set_constraints(Size::new(max_size.width, remaining_height));
                    let sizing = child.sizing();
                    remaining_height = remaining_height.saturating_sub(sizing.height);
                    max_child_width = max_child_width.max(sizing.width);
                    self.child_rects.as_mut()[i].size = sizing.into();

                    // Set dirty rect based on child's actual dirty rect or full size
                    if let Some(child_dirty) = sizing.dirty_rect {
                        dirty_rects.as_mut()[i] = child_dirty;
                    } else {
                        dirty_rects.as_mut()[i] = self.child_rects.as_ref()[i];
                    }
                }
            }
        }

        let total_flex_height = remaining_height;

        // Second pass: set constraints on flex children and update cached rects with sizes
        for (i, &flex_score) in flex_scores.iter().enumerate() {
            if flex_score > 0 {
                if let Some(child) = self.children.get_dyn_child(i) {
                    // Calculate height for this flex child based on its flex score
                    let flex_height = (total_flex_height * flex_score) / total_flex;
                    // Set constraints on flex child with calculated height
                    child.set_constraints(Size::new(max_size.width, flex_height));
                    let sizing = child.sizing();
                    remaining_height = remaining_height.saturating_sub(sizing.height);
                    max_child_width = max_child_width.max(sizing.width);
                    self.child_rects.as_mut()[i].size = sizing.into();

                    // Set dirty rect based on child's actual dirty rect or full size
                    if let Some(child_dirty) = sizing.dirty_rect {
                        dirty_rects.as_mut()[i] = child_dirty;
                    } else {
                        dirty_rects.as_mut()[i] = self.child_rects.as_ref()[i];
                    }
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

        // Third pass: Set positions for all children and update dirty_rects positions
        let spacing_after = self.spacing_after.as_ref();
        let child_rects = self.child_rects.as_mut();

        for i in 0..len {
            let size = child_rects[i].size;

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
            let position = Point::new(x_offset, y_offset as i32);
            child_rects[i].top_left = position;

            // Update the dirty rect position
            dirty_rects.as_mut()[i].top_left += position;

            // Add the child height
            y_offset = y_offset.saturating_add(size.height);

            // Add spacing after this child (but not after the last)
            if i < len - 1 {
                y_offset = y_offset.saturating_add(spacing_after[i]);
            }

            // Add alignment spacing
            y_offset = y_offset.saturating_add(spacing);
        }

        // Compute and store sizing based on MainAxisSize
        let height = match self.main_axis_size {
            MainAxisSize::Min => y_offset,        // Only as tall as needed
            MainAxisSize::Max => max_size.height, // Take full available height
        };

        // Compute the dirty rect - the actual area where children will draw
        let dirty_rect = super::bounding_rect(dirty_rects);

        self.sizing = Some(crate::Sizing {
            width: max_child_width,
            height,
            dirty_rect,
        });
    }

    fn sizing(&self) -> crate::Sizing {
        self.sizing
            .expect("set_constraints must be called before sizing")
    }

    #[allow(clippy::needless_range_loop)]
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

// Widget implementation for Column<Vec<W>>
impl<W, C> Widget for Column<Vec<W>>
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

// Widget implementation for Column<[W; N]>
impl<W, C, const N: usize> Widget for Column<[W; N]>
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

// Macro to implement Widget for Column with tuples of different sizes
macro_rules! impl_column_for_tuple {
    ($len:literal, $($t:ident),+) => {
        // Implementation for tuple directly
        impl<$($t: Widget<Color = C>),+, C: crate::WidgetColor> Widget for Column<($($t,)+)> {
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

        // Implementation for Box<tuple>
        impl<$($t: Widget<Color = C>),+, C: crate::WidgetColor> Widget for Column<alloc::boxed::Box<($($t,)+)>> {
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

                // Get mutable references to children through dereferencing
                #[allow(non_snake_case, unused_variables)]
                let ($(ref mut $t,)+) = &mut *self.children;

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
impl_column_for_tuple!(1, T1);
impl_column_for_tuple!(2, T1, T2);
impl_column_for_tuple!(3, T1, T2, T3);
impl_column_for_tuple!(4, T1, T2, T3, T4);
impl_column_for_tuple!(5, T1, T2, T3, T4, T5);
impl_column_for_tuple!(6, T1, T2, T3, T4, T5, T6);
impl_column_for_tuple!(7, T1, T2, T3, T4, T5, T6, T7);
impl_column_for_tuple!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_column_for_tuple!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_column_for_tuple!(10, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_column_for_tuple!(11, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_column_for_tuple!(12, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_column_for_tuple!(13, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_column_for_tuple!(14, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_column_for_tuple!(15, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_column_for_tuple!(16, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);
impl_column_for_tuple!(
    17, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17
);
impl_column_for_tuple!(
    18, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18
);
impl_column_for_tuple!(
    19, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19
);
impl_column_for_tuple!(
    20, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16, T17, T18, T19, T20
);
