use crate::{DynWidget, Widget, Instant};
use embedded_graphics::{
    draw_target::{DrawTarget, DrawTargetExt},
    geometry::{Point, Size},
    pixelcolor::PixelColor,
};

/// A simple widget that translates (offsets) its child widget
/// This is a non-animated version for use in transitions
pub struct SimpleTranslate<T> {
    pub child: T,
    offset: Point,
}

impl<T> SimpleTranslate<T> {
    pub fn new(child: T, offset: Point) -> Self {
        Self { child, offset }
    }
    
    pub fn set_offset(&mut self, offset: Point) {
        self.offset = offset;
    }
    
    pub fn offset(&self) -> Point {
        self.offset
    }
}

impl<T: DynWidget> DynWidget for SimpleTranslate<T> {
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size)
    }
    
    fn sizing(&self) -> crate::Sizing {
        self.child.sizing()
    }
    
    fn flex(&self) -> bool {
        self.child.flex()
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Adjust touch point by offset
        let adjusted_point = Point::new(
            point.x - self.offset.x,
            point.y - self.offset.y,
        );
        self.child.handle_touch(adjusted_point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    
    fn force_full_redraw(&mut self) {
        self.child.force_full_redraw()
    }
}

impl<T: Widget> Widget for SimpleTranslate<T>
where
    T::Color: PixelColor,
{
    type Color = T::Color;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Create a translated draw target
        let mut translated = target.translated(self.offset);
        self.child.draw(&mut translated, current_time)
    }
}