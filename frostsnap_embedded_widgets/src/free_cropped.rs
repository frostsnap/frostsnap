use embedded_graphics::{
    draw_target::{DrawTarget, DrawTargetExt, Translated},
    geometry::{OriginDimensions, Size},
    primitives::Rectangle,
    Pixel,
};

/// Extension trait to add free_cropped() method to all DrawTargets
pub trait FreeCrop: DrawTarget + Sized {
    /// Creates a draw target for a rectangular subregion without constraining to parent bounds.
    /// 
    /// Unlike the standard `cropped()` method, this doesn't take the intersection with the
    /// parent's bounding box. The child will see exactly the size specified, even if it
    /// extends beyond the parent's bounds.
    fn free_cropped<'a>(&'a mut self, area: &Rectangle) -> FreeCropped<'a, Self> {
        FreeCropped::new(self, area)
    }
}

// Implement FreeCrop for all DrawTarget types
impl<T: DrawTarget> FreeCrop for T {}

/// A DrawTarget that acts like Cropped but doesn't take intersection with parent bounds.
/// Based on embedded-graphics Cropped implementation but without the intersection.
pub struct FreeCropped<'a, T>
where
    T: DrawTarget,
{
    parent: Translated<'a, T>,
    size: Size,
}

impl<'a, T> FreeCropped<'a, T>
where
    T: DrawTarget,
{
    pub fn new(parent: &'a mut T, area: &Rectangle) -> Self {
        // Unlike Cropped, we DON'T take the intersection here
        // let area = area.intersection(&parent.bounding_box());
        
        Self {
            parent: parent.translated(area.top_left),
            size: area.size,
        }
    }
}

impl<T> DrawTarget for FreeCropped<'_, T>
where
    T: DrawTarget,
{
    type Color = T::Color;
    type Error = T::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.parent.draw_iter(pixels)
    }
}

impl<T> OriginDimensions for FreeCropped<'_, T>
where
    T: DrawTarget,
{
    fn size(&self) -> Size {
        self.size
    }
}