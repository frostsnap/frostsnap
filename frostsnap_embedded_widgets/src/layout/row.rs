use super::{CrossAxisAlignment, MainAxisAlignment};
use crate::super_draw_target::SuperDrawTarget;
use crate::{widget_tuple::{AssociatedArray, WidgetTuple}, Instant, Widget};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
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
    pub(crate) child_rects: T::Array<Rectangle>,
    pub(crate) debug_borders: bool,
    pub(crate) sizing: Option<crate::Sizing>,
    /// Spacing to add before each child (indexed by child position)
    pub(crate) spacing_before: T::Array<u32>,
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
            children,
            cross_axis_alignment: CrossAxisAlignment::Center,
            main_axis_alignment: MainAxisAlignment::Start,
            debug_borders: false,
            sizing: None,
        }
    }
}

impl<T: WidgetTuple> Row<T> {
    /// Add a widget to the row
    pub fn push<W: crate::DynWidget>(self, widget: W) -> Row<T::Add<W>> 
    where
        T: WidgetTuple
    {
        self.push_with_gap(widget, 0)
    }

    /// Add a widget with a gap before it
    pub fn push_with_gap<W: crate::DynWidget>(self, widget: W, gap: u32) -> Row<T::Add<W>> 
    where
        T: WidgetTuple
    {
        let new_children = self.children.add(widget);

        // Copy over existing spacing values and add new one
        let mut new_spacing = new_children.create_array_with(0);
        let old_spacing = self.spacing_before.as_ref();
        // copy_from_slice for the existing values
        new_spacing.as_mut()[..T::TUPLE_LEN].copy_from_slice(old_spacing);
        new_spacing.as_mut()[T::TUPLE_LEN] = gap; // Gap BEFORE this widget

        Row {
            child_rects: new_children.create_array_with(Rectangle::zero()),
            spacing_before: new_spacing,
            children: new_children,
            cross_axis_alignment: self.cross_axis_alignment,
            main_axis_alignment: self.main_axis_alignment,
            debug_borders: self.debug_borders,
            sizing: None,
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
