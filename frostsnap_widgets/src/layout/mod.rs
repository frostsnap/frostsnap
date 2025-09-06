mod alignment;
mod center;
mod collections;
mod column;
mod container;
mod padding;
mod row;
mod sized_box;
mod stack;

pub use alignment::{Align, Alignment, HorizontalAlignment, VerticalAlignment};
pub use center::Center;
pub use column::Column;
pub use container::Container;
pub use padding::Padding;
pub use row::Row;
pub use sized_box::SizedBox;
pub use stack::{Positioned, Positioning, Stack};

/// Trait for types that have an associated array type for storing auxiliary data
pub trait AssociatedArray {
    /// Generic associated type for auxiliary arrays (for storing rectangles, spacing, etc)
    type Array<T>: AsRef<[T]> + AsMut<[T]>;

    /// Create an array filled with a specific value, sized according to self
    fn create_array_with<T: Copy>(&self, value: T) -> Self::Array<T>;

    /// Get a child widget as a dyn DynWidget reference by index
    fn get_dyn_child(&mut self, index: usize) -> Option<&mut dyn crate::DynWidget>;

    /// Get the number of children
    fn len(&self) -> usize;

    /// Is the array empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Trait for types that can have widgets pushed to them
pub trait PushWidget<W: crate::DynWidget>: AssociatedArray {
    /// The resulting type after pushing a widget
    type Output: AssociatedArray;

    /// Push a widget to this collection
    fn push_widget(self, widget: W) -> Self::Output;
}

/// Alignment options for the cross axis (horizontal for Column, vertical for Row)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    /// Align children to the start (left/top) of the cross axis
    Start,
    /// Center children along the cross axis
    Center,
    /// Align children to the end (right/bottom) of the cross axis
    End,
}

/// Defines how children are distributed along the main axis (vertical for Column, horizontal for Row)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisAlignment {
    /// Place children at the start with no spacing between them
    Start,
    /// Center children with no spacing between them
    Center,
    /// Place children at the end with no spacing between them
    End,
    /// Place children with equal spacing between them, with no space before the first or after the last child
    /// Example with 3 children: [Child1]--space--[Child2]--space--[Child3]
    SpaceBetween,
    /// Place children with equal spacing around them, with half spacing before the first and after the last child
    /// Example with 3 children: -half-[Child1]-full-[Child2]-full-[Child3]-half-
    SpaceAround,
    /// Place children with equal spacing between and around them
    /// Example with 3 children: --space--[Child1]--space--[Child2]--space--[Child3]--space--
    SpaceEvenly,
}

/// Controls how much space the widget should take in its main axis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisSize {
    /// Take up as much space as possible in the main axis
    Max,
    /// Take up only as much space as needed by the children
    Min,
}

/// Helper function to draw debug borders around rectangles
/// Automatically determines the appropriate WHITE color based on the color type's bit depth
pub(crate) fn draw_debug_rect<D, C>(
    target: &mut D,
    rect: embedded_graphics::primitives::Rectangle,
) -> Result<(), D::Error>
where
    D: embedded_graphics::draw_target::DrawTarget<Color = C>,
    C: crate::WidgetColor,
{
    use embedded_graphics::pixelcolor::raw::RawData;
    use embedded_graphics::pixelcolor::{Gray2, Gray4, Rgb565};
    use embedded_graphics::prelude::{GrayColor, RgbColor};
    use embedded_graphics::primitives::{Primitive, PrimitiveStyle};
    use embedded_graphics::Drawable;

    let debug_color = match C::Raw::BITS_PER_PIXEL {
        16 => {
            // Assume Rgb565 for 16-bit
            Some(unsafe { *(&Rgb565::WHITE as *const Rgb565 as *const C) })
        }
        4 => {
            // Assume Gray4 for 4-bit
            Some(unsafe { *(&Gray4::WHITE as *const Gray4 as *const C) })
        }
        2 => {
            // Assume Gray2 for 2-bit
            Some(unsafe { *(&Gray2::WHITE as *const Gray2 as *const C) })
        }
        _ => None,
    };

    if let Some(color) = debug_color {
        rect.into_styled(PrimitiveStyle::with_stroke(color, 1))
            .draw(target)?;
    }

    Ok(())
}
