use super::{CrossAxisAlignment, MainAxisAlignment};
use crate::super_draw_target::SuperDrawTarget;
use crate::{widget_tuple::WidgetTuple, Instant, Widget};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
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
#[derive(PartialEq)]
pub struct Column<T: WidgetTuple> {
    pub children: T,
    pub cross_axis_alignment: CrossAxisAlignment,
    pub main_axis_alignment: MainAxisAlignment,
    child_rects: T::Array<Rectangle>,
    debug_borders: bool,
    sizing: Option<crate::Sizing>,
    /// Spacing to add before each child (indexed by child position)
    spacing_before: T::Array<u32>,
}

/// Helper to start building a Column with no children
impl Column<()> {
    pub fn builder() -> Self {
        Self::new(())
    }
}

impl<T: WidgetTuple> Column<T> {
    pub fn new(children: T) -> Self {
        // Don't extract sizes here - wait for set_constraints to be called
        // child_rects are already initialized to zero in the struct creation above

        Self {
            children,
            cross_axis_alignment: CrossAxisAlignment::Center,
            main_axis_alignment: MainAxisAlignment::Start,
            child_rects: T::create_array_with(Rectangle::zero()),
            debug_borders: false,
            sizing: None,
            spacing_before: T::create_array_with(0),
        }
    }

    /// Add a widget to the column
    pub fn push<W>(self, widget: W) -> Column<T::Add<W>> {
        self.push_with_gap(widget, 0)
    }

    /// Add a widget with a gap before it
    pub fn push_with_gap<W>(self, widget: W, gap: u32) -> Column<T::Add<W>> {
        let new_children = self.children.add(widget);

        // Copy over existing spacing values and add new one
        let mut new_spacing = <T::Add<W>>::create_array_with(0);
        let old_spacing = self.spacing_before.as_ref();
        // copy_from_slice for the existing values
        new_spacing.as_mut()[..T::TUPLE_LEN].copy_from_slice(old_spacing);
        new_spacing.as_mut()[T::TUPLE_LEN] = gap; // Gap BEFORE this widget

        Column {
            children: new_children,
            cross_axis_alignment: self.cross_axis_alignment,
            main_axis_alignment: self.main_axis_alignment,
            child_rects: <T::Add<W>>::create_array_with(Rectangle::zero()),
            debug_borders: self.debug_borders,
            sizing: None,
            spacing_before: new_spacing,
        }
    }

    pub fn with_cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }

    pub fn with_main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    pub fn with_debug_borders(mut self, enabled: bool) -> Self {
        self.debug_borders = enabled;
        self
    }
}

// Macro to implement Widget for Column with tuples of different sizes
macro_rules! impl_column_for_tuple {
    ($len:literal, $($t:ident),+) => {
        impl<$($t: Widget<Color = C>),+, C: PixelColor> crate::DynWidget for Column<($($t,)+)> {
            #[allow(unused_assignments)]
            fn set_constraints(&mut self, max_size: Size) {

                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;

                // First pass: query flex status and set initial constraints on non-flex children
                let mut is_flex = [false; $len];
                let mut flex_count = 0;
                let mut child_index = 0;
                let mut remaining_height = max_size.height;
                let mut max_child_width = 0u32;

                // Account for all spacing in remaining height
                let total_spacing: u32 = self.spacing_before[..$len].iter().sum();
                remaining_height = remaining_height.saturating_sub(total_spacing);

                $(
                    {
                        let flex = $t.flex();
                        is_flex[child_index] = flex;
                        if flex {
                            flex_count += 1;
                        } else {
                            // Set constraints on non-flex child with remaining available height
                            $t.set_constraints(Size::new(max_size.width, remaining_height));
                            let sizing = $t.sizing();
                            remaining_height = remaining_height.saturating_sub(sizing.height);
                            max_child_width = max_child_width.max(sizing.width);
                        }
                        child_index += 1;
                    }
                )+

                // Calculate height for each flex child
                let flex_height = if flex_count > 0 {
                    remaining_height / flex_count as u32
                } else {
                    0
                };

                // Second pass: set constraints on flex children and update cached rects with sizes
                let mut child_index = 0;
                $(
                    {
                        if is_flex[child_index] {
                            // Set constraints on flex child with calculated height
                            $t.set_constraints(Size::new(max_size.width, flex_height));
                            let sizing = $t.sizing();
                            self.child_rects[child_index].size = sizing.into();
                            remaining_height = remaining_height.saturating_sub(sizing.height);
                            max_child_width = max_child_width.max(sizing.width);
                        } else {
                            // Non-flex child already has constraints set, just get final size
                            let sizing = $t.sizing();
                            self.child_rects[child_index].size = sizing.into();
                        }
                        child_index += 1;
                    }
                )+

                // Now compute positions based on alignment
                // remaining_height now has any leftover space for alignment

                let (mut y_offset, spacing) = match self.main_axis_alignment {
                    MainAxisAlignment::Start => (0u32, 0u32),
                    MainAxisAlignment::Center => {
                        (remaining_height / 2, 0)
                    }
                    MainAxisAlignment::End => {
                        (remaining_height, 0)
                    }
                    MainAxisAlignment::SpaceBetween => {
                        if $len > 1 {
                            (0, remaining_height / ($len as u32 - 1))
                        } else {
                            (0, 0)
                        }
                    }
                    MainAxisAlignment::SpaceAround => {
                        let spacing = remaining_height / ($len as u32);
                        (spacing / 2, spacing)
                    }
                    MainAxisAlignment::SpaceEvenly => {
                        let spacing = remaining_height / ($len as u32 + 1);
                        (spacing, spacing)
                    }
                };

                // Set positions for all children
                let mut total_child_height = 0u32;
                for i in 0..$len {
                    // Add spacing BEFORE this child
                    y_offset = y_offset.saturating_add(self.spacing_before[i]);

                    let size = self.child_rects[i].size;
                    total_child_height += size.height;

                    let x_offset = match self.cross_axis_alignment {
                        CrossAxisAlignment::Start => 0,
                        CrossAxisAlignment::Center => {
                            let available_width = max_child_width.saturating_sub(size.width);
                            (available_width / 2) as i32
                        }
                        CrossAxisAlignment::End => {
                            let available_width = max_child_width.saturating_sub(size.width);
                            available_width as i32
                        }
                    };
                    self.child_rects[i].top_left = Point::new(x_offset, y_offset as i32);
                    // Add the child height and alignment spacing
                    y_offset = y_offset.saturating_add(size.height)
                        .saturating_add(spacing);
                }

                // Compute and store sizing
                // Width is always the maximum child width (cross-axis doesn't affect sizing)
                let width = max_child_width;

                // Height depends on main axis alignment
                let height = match self.main_axis_alignment {
                    MainAxisAlignment::Start => total_child_height + total_spacing,
                    _ => max_size.height,  // All other alignments need full height for spacing
                };

                self.sizing = Some(crate::Sizing { width, height });
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
                // Use cached rectangles
                if $len > 0 {
                    #[allow(non_snake_case)]
                    let ($(ref mut $t,)+) = self.children;

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


            fn force_full_redraw(&mut self) {
                #[allow(non_snake_case)]
                let ($(ref mut $t,)+) = self.children;

                $(
                    $t.force_full_redraw();
                )+
            }
        }

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
                            use embedded_graphics::primitives::PrimitiveStyle;
                            use embedded_graphics::pixelcolor::{Rgb565, Gray4, Gray2};
                            use embedded_graphics::pixelcolor::raw::RawData;

                            let rect = self.child_rects[child_index];

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
